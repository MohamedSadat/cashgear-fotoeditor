//! CPU-only synchronous codecs. The WASM job host supplies cancellation/heap limits.
use crate::{srgb_icc, ImageExportSource, MAX_FILE_BYTES};
use image::ImageEncoder;
use serde::{Deserialize, Serialize};
use std::io::Write;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ImageExportFormat {
    Png,
    Jpeg,
    Webp,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields, rename_all = "camelCase")]
pub struct ImageExportOptions {
    pub format: ImageExportFormat,
    pub source: ImageExportSource,
    pub quality: u8,
    pub lossless: bool,
    pub embed_icc: bool,
    pub matte: [u8; 3],
}
impl Default for ImageExportOptions {
    fn default() -> Self {
        Self {
            format: ImageExportFormat::Png,
            source: ImageExportSource::Full,
            quality: 85,
            lossless: false,
            embed_icc: true,
            matte: [255; 3],
        }
    }
}
impl ImageExportOptions {
    pub fn validate(&self) -> Result<(), String> {
        if (self.format == ImageExportFormat::Jpeg
            || self.format == ImageExportFormat::Webp && !self.lossless)
            && !(1..=100).contains(&self.quality)
        {
            return Err("Image quality must be between 1 and 100".into());
        }
        Ok(())
    }
}
#[derive(Default)]
struct BoundedOutput(Vec<u8>);
impl Write for BoundedOutput {
    fn write(&mut self, data: &[u8]) -> std::io::Result<usize> {
        let needed = self
            .0
            .len()
            .checked_add(data.len())
            .ok_or_else(|| std::io::Error::other("Image output length overflow"))?;
        if needed as u64 > MAX_FILE_BYTES {
            return Err(std::io::Error::other("Image export exceeds 128 MiB"));
        }
        if needed > self.0.capacity() {
            let target = needed
                .max(self.0.capacity().saturating_mul(2))
                .max(4096)
                .min(MAX_FILE_BYTES as usize);
            self.0
                .try_reserve_exact(target - self.0.len())
                .map_err(|_| std::io::Error::other("Not enough codec memory for this export"))?;
        }
        self.0.extend_from_slice(data);
        Ok(data.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

pub fn encode_image_rgba(
    rgba: &[u8],
    width: u32,
    height: u32,
    dpi: f64,
    options: &ImageExportOptions,
) -> Result<Vec<u8>, String> {
    fotoeditor_core::validate_size(width, height, dpi)?;
    options.validate()?;
    if rgba.len() != width as usize * height as usize * 4 {
        return Err("Invalid RGBA image length".into());
    }
    match options.format {
        ImageExportFormat::Png => Err("PNG export uses the streaming scanline job".into()),
        ImageExportFormat::Jpeg => {
            let mut rgb = Vec::with_capacity(width as usize * height as usize * 3);
            for pixel in rgba.as_chunks::<4>().0 {
                let alpha = u32::from(pixel[3]);
                for (channel, matte) in pixel[..3].iter().zip(options.matte) {
                    rgb.push(
                        ((u32::from(*channel) * alpha + u32::from(matte) * (255 - alpha) + 127)
                            / 255) as u8,
                    );
                }
            }
            let mut output = BoundedOutput::default();
            let mut encoder =
                image::codecs::jpeg::JpegEncoder::new_with_quality(&mut output, options.quality);
            encoder.set_pixel_density(image::codecs::jpeg::PixelDensity::dpi(dpi.round() as u16));
            if options.embed_icc {
                encoder
                    .set_icc_profile(srgb_icc().to_vec())
                    .map_err(|e| e.to_string())?;
            }
            encoder
                .encode(&rgb, width, height, image::ExtendedColorType::Rgb8)
                .map_err(|e| e.to_string())?;
            Ok(output.0)
        }
        ImageExportFormat::Webp if options.lossless => {
            let mut output = BoundedOutput::default();
            let mut encoder = image::codecs::webp::WebPEncoder::new_lossless(&mut output);
            if options.embed_icc {
                encoder
                    .set_icc_profile(srgb_icc().to_vec())
                    .map_err(|e| e.to_string())?;
            }
            encoder
                .encode(rgba, width, height, image::ExtendedColorType::Rgba8)
                .map_err(|e| e.to_string())?;
            Ok(output.0)
        }
        ImageExportFormat::Webp => lossy_webp(rgba, width, height, options),
    }
}

fn chunk(output: &mut Vec<u8>, tag: &[u8; 4], bytes: &[u8]) {
    output.extend(tag);
    output.extend((bytes.len() as u32).to_le_bytes());
    output.extend(bytes);
    if !bytes.len().is_multiple_of(2) {
        output.push(0);
    }
}
fn lossy_webp(
    rgba: &[u8],
    width: u32,
    height: u32,
    options: &ImageExportOptions,
) -> Result<Vec<u8>, String> {
    if width > 16383 || height > 16383 {
        return Err(
            "Lossy WebP supports sides up to 16,383 pixels; use PNG or lossless WebP".into(),
        );
    }
    let image = webpkit::ImageRef::new(
        webpkit::Dimensions::new(width, height).map_err(|e| e.to_string())?,
        webpkit::PixelLayout::Rgba8,
        rgba,
    )
    .map_err(|e| e.to_string())?;
    let (_, payload) = webpkit::lossy::encode_vp8(
        image,
        &webpkit::lossy::LossyConfig::new()
            .with_quality(options.quality)
            .with_effort(webpkit::Effort::Balanced),
    )
    .map_err(|e| e.to_string())?;
    let has_alpha = rgba.as_chunks::<4>().0.iter().any(|p| p[3] != 255);
    let profile = options.embed_icc.then(srgb_icc);
    let extended = has_alpha || profile.is_some();
    let alpha_length = width as usize * height as usize + 1;
    let size = |n: usize| 8 + n + n % 2;
    let total = 12
        + if extended { 18 } else { 0 }
        + size(payload.len())
        + profile.map_or(0, |p| size(p.len()))
        + if has_alpha { size(alpha_length) } else { 0 };
    if total as u64 > MAX_FILE_BYTES {
        return Err("Image export exceeds 128 MiB".into());
    }
    let mut output = Vec::with_capacity(total);
    output.extend(b"RIFF");
    output.extend(((total - 8) as u32).to_le_bytes());
    output.extend(b"WEBP");
    if extended {
        let mut header = [0u8; 10];
        header[0] = (u8::from(profile.is_some()) * 0x20) | (u8::from(has_alpha) * 0x10);
        header[4..7].copy_from_slice(&(width - 1).to_le_bytes()[..3]);
        header[7..10].copy_from_slice(&(height - 1).to_le_bytes()[..3]);
        chunk(&mut output, b"VP8X", &header);
    }
    if let Some(profile) = profile {
        chunk(&mut output, b"ICCP", profile);
    }
    if has_alpha {
        // WebP ALPH compression method 0: exact bytes, no additional plane-sized
        // encoder scratch. See https://developers.google.com/speed/webp/docs/riff_container
        output.extend(b"ALPH");
        output.extend((alpha_length as u32).to_le_bytes());
        output.push(0);
        output.extend(rgba.as_chunks::<4>().0.iter().map(|p| p[3]));
        if !alpha_length.is_multiple_of(2) {
            output.push(0);
        }
    }
    chunk(&mut output, b"VP8 ", &payload);
    debug_assert_eq!(output.len(), total);
    Ok(output)
}
