//! Bounded matrix/TRC ICC conversion. RGB samples remain straight, including at A=0.
use image::ColorType;
use qcms::{DataType, Intent, Profile, Transform};
use std::sync::OnceLock;

pub const COLOR_SCRATCH: u64 = 32 * 1024 * 1024;
pub const MAX_ICC_BYTES: usize = 4 * 1024 * 1024;

fn number(bytes: &[u8], offset: usize) -> Result<u32, String> {
    Ok(u32::from_be_bytes(
        bytes
            .get(offset..offset + 4)
            .ok_or("Truncated ICC profile")?
            .try_into()
            .unwrap(),
    ))
}

fn validate_profile(bytes: &[u8]) -> Result<(), String> {
    if !(132..=MAX_ICC_BYTES).contains(&bytes.len())
        || number(bytes, 0)? as usize != bytes.len()
        || &bytes[36..40] != b"acsp"
    {
        return Err("Invalid ICC profile or profile exceeds 4 MiB".into());
    }
    let count = number(bytes, 128)? as usize;
    if count > 128 || 132 + count * 12 > bytes.len() {
        return Err("Invalid or excessive ICC tag directory".into());
    }
    for index in 0..count {
        let at = 132 + index * 12;
        let signature = &bytes[at..at + 4];
        if matches!(
            signature,
            b"A2B0" | b"A2B1" | b"A2B2" | b"B2A0" | b"B2A1" | b"B2A2" | b"D2B0" | b"B2D0"
        ) {
            return Err(
                "LUT ICC profiles are not supported; use an RGB/gray matrix/TRC profile".into(),
            );
        }
        let offset = number(bytes, at + 4)? as usize;
        let length = number(bytes, at + 8)? as usize;
        if offset < 132 + count * 12
            || offset
                .checked_add(length)
                .is_none_or(|end| end > bytes.len())
        {
            return Err("ICC tag data lies outside the profile".into());
        }
        if matches!(signature, b"rTRC" | b"gTRC" | b"bTRC" | b"kTRC") {
            if length < 12 {
                return Err("Truncated ICC tone curve".into());
            }
            if &bytes[offset..offset + 4] == b"curv" && number(bytes, offset + 8)? > 16384 {
                return Err("ICC tone curve exceeds 16,384 samples".into());
            }
        }
    }
    Ok(())
}

pub(crate) enum IccTransform {
    Rgb(Transform),
    Gray(Transform),
}
impl IccTransform {
    pub fn new(bytes: &[u8], decoded_type: ColorType) -> Result<Option<Self>, String> {
        validate_profile(bytes)?;
        let input =
            Profile::new_from_slice(bytes, false).ok_or("Invalid or unsupported ICC profile")?;
        // Our canonical exported profile is independently parseable and identifies
        // the exact working space. Avoid an unnecessary quantized round trip.
        if bytes == srgb_icc() {
            return Ok(None);
        }
        let output = Profile::new_sRGB();
        let transform = match &bytes[16..20] {
            b"RGB " => Self::Rgb(
                Transform::new(&input, &output, DataType::RGBA8, Intent::Perceptual)
                    .ok_or("Cannot convert the embedded RGB ICC profile to sRGB")?,
            ),
            b"GRAY" if matches!(decoded_type, ColorType::L8 | ColorType::La8) => Self::Gray(
                Transform::new_to(
                    &input,
                    &output,
                    DataType::GrayA8,
                    DataType::RGBA8,
                    Intent::Perceptual,
                )
                .ok_or("Cannot convert the embedded gray ICC profile to sRGB")?,
            ),
            _ => {
                return Err(
                    "Unsupported ICC sample space; RGB and grayscale profiles are supported".into(),
                )
            }
        };
        Ok(Some(transform))
    }
    /// At most one tile row, so conversion never monopolizes a complete image job.
    pub fn convert(&self, row: &mut [u8]) {
        assert!(row.len() <= 256 * 4 && row.len().is_multiple_of(4));
        match self {
            Self::Rgb(transform) => transform.apply(row),
            Self::Gray(transform) => {
                let mut gray = [0u8; 512];
                for (src, dst) in row
                    .as_chunks::<4>()
                    .0
                    .iter()
                    .zip(gray.as_chunks_mut::<2>().0)
                {
                    dst.copy_from_slice(&[src[0], src[3]]);
                }
                transform.convert(&gray[..row.len() / 2], row);
            }
        }
    }
}

fn xyz(values: [f64; 3]) -> Vec<u8> {
    let mut bytes = b"XYZ \0\0\0\0".to_vec();
    for value in values {
        bytes.extend(((value * 65536.0).round() as i32).to_be_bytes());
    }
    bytes
}

fn rgb_profile(curve: impl Fn(f64) -> f64) -> Vec<u8> {
    // ICC v2 display profile using D50-adapted sRGB primaries, D50 media white,
    // and a shared sampled TRC. Generated here; no third-party profile is copied.
    let description = b"CashGear sRGB\0";
    let mut desc = b"desc\0\0\0\0".to_vec();
    desc.extend((description.len() as u32).to_be_bytes());
    desc.extend(description);
    desc.extend([0; 8 + 2 + 1 + 67]);
    let mut trc = b"curv\0\0\0\0".to_vec();
    trc.extend(4096u32.to_be_bytes());
    for i in 0..4096 {
        trc.extend(((curve(i as f64 / 4095.0) * 65535.0).round() as u16).to_be_bytes());
    }
    let tags = [
        (*b"desc", desc),
        (
            *b"cprt",
            b"text\0\0\0\0Generated by CashGear FotoEditor\0".to_vec(),
        ),
        (*b"wtpt", xyz([0.9642, 1.0, 0.8249])),
        (*b"rXYZ", xyz([0.4360747, 0.2225045, 0.0139322])),
        (*b"gXYZ", xyz([0.3850649, 0.7168786, 0.0971045])),
        (*b"bXYZ", xyz([0.1430804, 0.0606169, 0.7141733])),
        (*b"rTRC", trc.clone()),
        (*b"gTRC", trc.clone()),
        (*b"bTRC", trc),
    ];
    let mut bytes = vec![0; 132 + tags.len() * 12];
    bytes[4..8].copy_from_slice(b"CGFE");
    bytes[8..12].copy_from_slice(&0x02400000u32.to_be_bytes());
    bytes[12..16].copy_from_slice(b"mntr");
    bytes[16..20].copy_from_slice(b"RGB ");
    bytes[20..24].copy_from_slice(b"XYZ ");
    for (i, v) in [2026u16, 10, 27, 0, 0, 0].into_iter().enumerate() {
        bytes[24 + i * 2..26 + i * 2].copy_from_slice(&v.to_be_bytes());
    }
    bytes[36..40].copy_from_slice(b"acsp");
    bytes[40..44].copy_from_slice(b"MSFT");
    bytes[68..80].copy_from_slice(&xyz([0.9642, 1.0, 0.8249])[8..]);
    bytes[80..84].copy_from_slice(b"CGFE");
    bytes[128..132].copy_from_slice(&(tags.len() as u32).to_be_bytes());
    let mut stored: Vec<(Vec<u8>, usize)> = Vec::new();
    for (i, (signature, data)) in tags.into_iter().enumerate() {
        let offset = if let Some((_, offset)) = stored.iter().find(|(b, _)| *b == data) {
            *offset
        } else {
            while !bytes.len().is_multiple_of(4) {
                bytes.push(0);
            }
            let offset = bytes.len();
            bytes.extend(&data);
            stored.push((data.clone(), offset));
            offset
        };
        let at = 132 + i * 12;
        bytes[at..at + 4].copy_from_slice(&signature);
        bytes[at + 4..at + 8].copy_from_slice(&(offset as u32).to_be_bytes());
        bytes[at + 8..at + 12].copy_from_slice(&(data.len() as u32).to_be_bytes());
    }
    while !bytes.len().is_multiple_of(4) {
        bytes.push(0);
    }
    let length = bytes.len() as u32;
    bytes[..4].copy_from_slice(&length.to_be_bytes());
    bytes
}

pub fn srgb_icc() -> &'static [u8] {
    static PROFILE: OnceLock<Vec<u8>> = OnceLock::new();
    PROFILE.get_or_init(|| {
        rgb_profile(|value| {
            if value <= 0.04045 {
                value / 12.92
            } else {
                ((value + 0.055) / 1.055).powf(2.4)
            }
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn canonical_profile_is_valid_and_linear_conversion_preserves_every_alpha() {
        validate_profile(srgb_icc()).unwrap();
        assert!(Profile::new_from_slice(srgb_icc(), false).is_some());
        assert!(IccTransform::new(srgb_icc(), ColorType::Rgba8)
            .unwrap()
            .is_none());
        let linear = rgb_profile(|v| v);
        let transform = IccTransform::new(&linear, ColorType::Rgba8)
            .unwrap()
            .unwrap();
        let mut pixels: Vec<u8> = (0..256).flat_map(|v| [v as u8, 51, 127, v as u8]).collect();
        transform.convert(&mut pixels);
        for (value, pixel) in pixels.as_chunks::<4>().0.iter().enumerate() {
            assert_eq!(pixel[3], value as u8);
            let v = value as f64 / 255.0;
            let expected = if v <= 0.0031308 {
                12.92 * v
            } else {
                1.055 * v.powf(1.0 / 2.4) - 0.055
            };
            assert!(
                (pixel[0] as f64 - expected * 255.0).abs() <= 2.0,
                "{value}: {pixel:?}"
            );
            assert!((pixel[1] as i16 - 124).abs() <= 2);
            assert!((pixel[2] as i16 - 187).abs() <= 2);
        }
        assert_ne!(&pixels[..3], &[0, 51, 127]); // Hidden RGB is converted, not erased.
    }
    #[test]
    fn corrupt_profiles_and_unsupported_luts_are_explicit_errors() {
        for length in [0, 4, 127, 131, srgb_icc().len() - 1] {
            assert!(IccTransform::new(&srgb_icc()[..length], ColorType::Rgba8).is_err());
        }
        let mut invalid = srgb_icc().to_vec();
        invalid[132..136].copy_from_slice(b"A2B0");
        assert!(IccTransform::new(&invalid, ColorType::Rgba8).is_err());
        invalid = srgb_icc().to_vec();
        invalid[136..140].copy_from_slice(&u32::MAX.to_be_bytes());
        assert!(IccTransform::new(&invalid, ColorType::Rgba8).is_err());
    }
}
