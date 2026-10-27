//! Reject excessive metadata before codec constructors allocate their directories.
use crate::color::MAX_ICC_BYTES;
use image::ImageFormat;

const MAX_METADATA: usize = 8 * 1024 * 1024;

pub fn preflight(bytes: &[u8], format: ImageFormat) -> Result<bool, String> {
    match format {
        ImageFormat::Tiff => tiff(bytes),
        ImageFormat::Jpeg => jpeg(bytes),
        ImageFormat::WebP => webp(bytes),
        ImageFormat::Png => png(bytes),
        ImageFormat::Gif => gif(bytes),
        _ => Ok(false),
    }
}

fn tiff(bytes: &[u8]) -> Result<bool, String> {
    let little = match bytes.get(..4) {
        Some(b"II\x2a\0") => true,
        Some(b"MM\0\x2a") => false,
        _ => return Err("Expected a classic 8-bit TIFF; BigTIFF is not supported".into()),
    };
    let read16 = |at: usize| -> Result<u16, String> {
        let value = bytes
            .get(at..at + 2)
            .ok_or("Truncated TIFF directory")?
            .try_into()
            .unwrap();
        Ok(if little {
            u16::from_le_bytes(value)
        } else {
            u16::from_be_bytes(value)
        })
    };
    let read32 = |at: usize| -> Result<u32, String> {
        let value = bytes
            .get(at..at + 4)
            .ok_or("Truncated TIFF directory")?
            .try_into()
            .unwrap();
        Ok(if little {
            u32::from_le_bytes(value)
        } else {
            u32::from_be_bytes(value)
        })
    };
    let offset = read32(4)? as usize;
    let count = read16(offset)? as usize;
    if offset < 8
        || count > 4096
        || offset
            .checked_add(2 + count * 12 + 4)
            .is_none_or(|end| end > bytes.len())
    {
        return Err("Invalid or excessive TIFF directory".into());
    }
    let mut total = 0usize;
    let mut profile = false;
    let mut tags = std::collections::HashSet::new();
    for i in 0..count {
        let at = offset + 2 + i * 12;
        let tag = read16(at)?;
        profile |= tag == 34675;
        if !tags.insert(tag) {
            return Err("Duplicate TIFF directory tag".into());
        }
        let unit = match read16(at + 2)? {
            1 | 2 | 6 | 7 => 1usize,
            3 | 8 => 2,
            4 | 9 | 11 | 13 => 4,
            5 | 10 | 12 | 16 | 17 | 18 => 8,
            _ => return Err("Unsupported TIFF directory field type".into()),
        };
        let length = (read32(at + 4)? as usize)
            .checked_mul(unit)
            .ok_or("TIFF field size overflow")?;
        if tag != 34675 {
            total = total.checked_add(length).ok_or("TIFF metadata overflow")?;
        }
        if total > 1024 * 1024 || tag == 34675 && length > MAX_ICC_BYTES {
            return Err("TIFF non-ICC metadata exceeds 1 MiB or ICC profile exceeds 4 MiB".into());
        }
        if length > 4 {
            let start = read32(at + 8)? as usize;
            if start
                .checked_add(length)
                .is_none_or(|end| end > bytes.len())
            {
                return Err("Truncated TIFF field data".into());
            }
        }
    }
    Ok(profile)
}

fn jpeg(bytes: &[u8]) -> Result<bool, String> {
    let mut at = 2usize;
    let mut profile = 0usize;
    while at < bytes.len() {
        if bytes[at] != 255 {
            return Err("Invalid JPEG marker before scan data".into());
        }
        while bytes.get(at) == Some(&255) {
            at += 1;
        }
        let marker = *bytes.get(at).ok_or("Truncated JPEG marker")?;
        at += 1;
        if matches!(marker, 0xda | 0xd9) {
            return Ok(profile != 0);
        }
        if matches!(marker, 0x01 | 0xd0..=0xd8) {
            continue;
        }
        let length = u16::from_be_bytes(
            bytes
                .get(at..at + 2)
                .ok_or("Truncated JPEG segment")?
                .try_into()
                .unwrap(),
        ) as usize;
        if length < 2 || at.checked_add(length).is_none_or(|end| end > bytes.len()) {
            return Err("Invalid JPEG segment length".into());
        }
        if marker == 0xe2 && bytes[at + 2..at + length].starts_with(b"ICC_PROFILE\0") {
            profile += length;
        }
        at += length;
        if at > MAX_METADATA || profile > MAX_ICC_BYTES {
            return Err("JPEG metadata exceeds 8 MiB or ICC profile exceeds 4 MiB".into());
        }
    }
    Err("JPEG contains no image scan".into())
}

fn webp(bytes: &[u8]) -> Result<bool, String> {
    if bytes.len() < 12
        || u32::from_le_bytes(bytes[4..8].try_into().unwrap()) as u64 + 8 != bytes.len() as u64
    {
        return Err("Invalid WebP RIFF length".into());
    }
    let mut at = 12usize;
    let mut metadata = 0usize;
    let mut profile = false;
    while at < bytes.len() {
        let header = bytes.get(at..at + 8).ok_or("Truncated WebP chunk")?;
        let length = u32::from_le_bytes(header[4..8].try_into().unwrap()) as usize;
        if &header[..4] == b"ICCP" {
            if profile {
                return Err("Duplicate WebP ICC profile".into());
            }
            profile = true;
        }
        if &header[..4] == b"ICCP" && length > MAX_ICC_BYTES {
            return Err("WebP ICC profile exceeds 4 MiB".into());
        }
        if matches!(&header[..4], b"ICCP" | b"EXIF" | b"XMP ") {
            metadata = metadata
                .checked_add(length)
                .ok_or("WebP metadata overflow")?;
        }
        if metadata > MAX_METADATA {
            return Err("WebP metadata exceeds 8 MiB".into());
        }
        at = at
            .checked_add(8)
            .and_then(|n| n.checked_add(length))
            .and_then(|n| n.checked_add(length % 2))
            .ok_or("WebP chunk overflow")?;
        if at > bytes.len() {
            return Err("Truncated WebP chunk data".into());
        }
    }
    Ok(profile)
}

fn png(bytes: &[u8]) -> Result<bool, String> {
    let mut at = 8usize;
    let mut profile = false;
    while at < bytes.len() {
        let header = bytes.get(at..at + 8).ok_or("Truncated PNG chunk")?;
        let length = u32::from_be_bytes(header[..4].try_into().unwrap()) as usize;
        if &header[4..8] == b"IDAT" {
            return Ok(profile);
        }
        if &header[4..8] == b"iCCP" {
            if profile {
                return Err("Duplicate PNG ICC profile".into());
            }
            profile = true;
        }
        at = at
            .checked_add(12)
            .and_then(|n| n.checked_add(length))
            .ok_or("PNG chunk length overflow")?;
        if at > bytes.len() || at > MAX_METADATA {
            return Err("PNG metadata is truncated or exceeds 8 MiB".into());
        }
    }
    Err("PNG contains no image data".into())
}

fn gif(bytes: &[u8]) -> Result<bool, String> {
    let header = bytes.get(..13).ok_or("Truncated GIF header")?;
    let mut at = 13usize;
    if header[10] & 0x80 != 0 {
        at += 3 * (2usize << (header[10] & 7));
    }
    let mut profile = false;
    loop {
        let kind = *bytes.get(at).ok_or("Truncated GIF header extensions")?;
        at += 1;
        if kind == 0x2c {
            return Ok(profile);
        }
        if kind != 0x21 {
            return Err("GIF contains no first image frame".into());
        }
        let label = *bytes.get(at).ok_or("Truncated GIF extension")?;
        at += 1;
        let mut first = true;
        let mut icc = false;
        let mut profile_bytes = 0usize;
        loop {
            let length = *bytes.get(at).ok_or("Truncated GIF extension block")? as usize;
            at += 1;
            if length == 0 {
                break;
            }
            let block = bytes
                .get(at..at + length)
                .ok_or("Truncated GIF extension payload")?;
            if first && label == 0xff && block.starts_with(b"ICCRGBG1") {
                if profile {
                    return Err("Duplicate GIF ICC profile".into());
                }
                profile = true;
                icc = true;
            }
            first = false;
            at += length;
            if icc {
                profile_bytes += length;
            }
            if at > MAX_METADATA || profile_bytes > MAX_ICC_BYTES {
                return Err("GIF metadata exceeds 8 MiB or ICC exceeds 4 MiB".into());
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn untrusted_directory_lengths_fail_before_codec_construction() {
        let mut tiff = b"II\x2a\0\x08\0\0\0\x01\0".to_vec();
        tiff.extend(34675u16.to_le_bytes());
        tiff.extend(1u16.to_le_bytes());
        tiff.extend(u32::MAX.to_le_bytes());
        tiff.extend(26u32.to_le_bytes());
        tiff.extend([0; 4]);
        assert!(preflight(&tiff, ImageFormat::Tiff).is_err());
        let mut webp = b"RIFF\0\0\0\0WEBPICCP".to_vec();
        webp.extend(u32::MAX.to_le_bytes());
        assert!(preflight(&webp, ImageFormat::WebP).is_err());
        assert!(preflight(b"\xff\xd8\xff\xe2\xff\xff", ImageFormat::Jpeg).is_err());
    }
}
