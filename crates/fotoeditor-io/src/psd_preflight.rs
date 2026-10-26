//! Allocation-free PSD framing validation before any channel decoder is invoked.
//! Passing this inspection establishes framing only, never Photoshop compatibility.
use crate::MAX_FILE_BYTES;
use fotoeditor_core::{validate_size, MAX_LAYERS};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PsdHeader {
    pub channels: u16,
    pub width: u32,
    pub height: u32,
    pub depth: u16,
    pub color_mode: u16,
}

#[derive(Debug)]
pub struct PsdPreflight<'a> {
    pub header: PsdHeader,
    pub color_mode_data: &'a [u8],
    pub image_resources: &'a [u8],
    pub layer_and_mask_data: &'a [u8],
    pub image_data: &'a [u8],
    pub layer_count: u16,
    pub merged_transparency: bool,
    pub icc_profile: Option<&'a [u8]>,
    /// Raw Adobe VersionInfo resource, for fixture provenance inspection.
    pub version_info: Option<&'a [u8]>,
}

struct Cursor<'a> {
    remaining: &'a [u8],
}
impl<'a> Cursor<'a> {
    fn take(&mut self, length: usize) -> Result<&'a [u8], String> {
        let (bytes, rest) = self
            .remaining
            .split_at_checked(length)
            .ok_or("Truncated PSD section")?;
        self.remaining = rest;
        Ok(bytes)
    }
    fn u16(&mut self) -> Result<u16, String> {
        Ok(u16::from_be_bytes(self.take(2)?.try_into().unwrap()))
    }
    fn u32(&mut self) -> Result<u32, String> {
        Ok(u32::from_be_bytes(self.take(4)?.try_into().unwrap()))
    }
    fn section(&mut self) -> Result<&'a [u8], String> {
        let length = self.u32()? as usize;
        self.take(length)
    }
}

impl<'a> PsdPreflight<'a> {
    /// Inspect supported 8-bit RGB PSD v1 framing, borrowing the original input.
    /// The eventual decoder must separately validate every layer/channel length,
    /// compression stream, blend, mask and metadata feature before atomic commit.
    pub fn inspect(bytes: &'a [u8]) -> Result<Self, String> {
        if bytes.len() as u64 > MAX_FILE_BYTES {
            return Err("PSD image files must be at most 128 MiB".into());
        }
        let mut input = Cursor { remaining: bytes };
        if input.take(4)? != b"8BPS" || input.u16()? != 1 {
            return Err("Expected PSD version 1; PSB is not supported".into());
        }
        if input.take(6)? != [0; 6] {
            return Err("Invalid PSD reserved header bytes".into());
        }
        let channels = input.u16()?;
        let height = input.u32()?;
        let width = input.u32()?;
        let depth = input.u16()?;
        let color_mode = input.u16()?;
        validate_size(width, height, 72.0)?;
        if !(3..=56).contains(&channels) || color_mode != 3 {
            return Err(
                "PSD preflight currently supports RGB documents with 3..56 channels".into(),
            );
        }
        if depth != 8 {
            return Err(
                "PSD import requires 8-bit samples; 16-bit processing belongs to P11".into(),
            );
        }
        let color_mode_data = input.section()?;
        let image_resources = input.section()?;
        let layer_and_mask_data = input.section()?;
        let image_data = input.remaining;
        let mut image = Cursor {
            remaining: image_data,
        };
        if image.u16()? > 3 {
            return Err("Unsupported PSD composite compression method".into());
        }

        let mut resources = Cursor {
            remaining: image_resources,
        };
        let mut icc_profile = None;
        let mut version_info = None;
        while !resources.remaining.is_empty() {
            if resources.take(4)? != b"8BIM" {
                return Err("Invalid PSD image resource signature".into());
            }
            let id = resources.u16()?;
            let name_length = resources.take(1)?[0] as usize;
            resources.take(name_length)?;
            if !(name_length + 1).is_multiple_of(2) {
                resources.take(1)?;
            }
            let data = resources.section()?;
            if !data.len().is_multiple_of(2) {
                resources.take(1)?;
            }
            let slot = match id {
                1039 => &mut icc_profile,
                1057 => &mut version_info,
                _ => continue,
            };
            if slot.replace(data).is_some() {
                return Err("Duplicate PSD ICC or VersionInfo resource".into());
            }
        }

        let (layer_count, merged_transparency) = if layer_and_mask_data.is_empty() {
            (0, false)
        } else {
            let mut layers = Cursor {
                remaining: layer_and_mask_data,
            };
            let info = layers.section()?;
            if info.is_empty() {
                (0, false)
            } else {
                let mut info = Cursor { remaining: info };
                let signed = info.u16()? as i16;
                let count = signed.unsigned_abs();
                if count as usize > MAX_LAYERS {
                    return Err("PSD layer count exceeds the document layer limit".into());
                }
                (count, signed < 0)
            }
        };
        Ok(Self {
            header: PsdHeader {
                channels,
                width,
                height,
                depth,
                color_mode,
            },
            color_mode_data,
            image_resources,
            layer_and_mask_data,
            image_data,
            layer_count,
            merged_transparency,
            icc_profile,
            version_info,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn section(bytes: &[u8]) -> Vec<u8> {
        let mut output = (bytes.len() as u32).to_be_bytes().to_vec();
        output.extend(bytes);
        output
    }
    fn resource(id: u16, name: &[u8], bytes: &[u8]) -> Vec<u8> {
        let mut output = b"8BIM".to_vec();
        output.extend(id.to_be_bytes());
        output.push(name.len() as u8);
        output.extend(name);
        if !output.len().is_multiple_of(2) {
            output.push(0);
        }
        output.extend(section(bytes));
        if !bytes.len().is_multiple_of(2) {
            output.push(0);
        }
        output
    }
    fn fixture(resources: &[u8], layer_data: &[u8]) -> Vec<u8> {
        let mut bytes = b"8BPS\0\x01\0\0\0\0\0\0\0\x04".to_vec();
        bytes.extend(259u32.to_be_bytes());
        bytes.extend(257u32.to_be_bytes());
        bytes.extend(8u16.to_be_bytes());
        bytes.extend(3u16.to_be_bytes());
        bytes.extend(section(&[]));
        bytes.extend(section(resources));
        bytes.extend(section(layer_data));
        bytes.extend([0, 1]); // Framing only, deliberately no decoded channel data.
        bytes
    }
    #[test]
    fn borrows_profiles_provenance_and_negative_layer_count_without_decoding() {
        let mut resources = resource(1039, b"", b"icc-placeholder");
        resources.extend(resource(1057, b"odd", b"version-placeholder"));
        resources.extend(resource(999, b"even", b"ignored"));
        let bytes = fixture(&resources, &section(&(-2i16).to_be_bytes()));
        let info = PsdPreflight::inspect(&bytes).unwrap();
        assert_eq!(info.header.width, 257);
        assert_eq!(info.header.height, 259);
        assert_eq!(info.layer_count, 2);
        assert!(info.merged_transparency);
        let profile = info.icc_profile.unwrap();
        assert_eq!(profile, b"icc-placeholder");
        assert_eq!(info.version_info.unwrap(), b"version-placeholder");
        assert!(profile.as_ptr() >= bytes.as_ptr());
        assert!(profile.as_ptr_range().end <= bytes.as_ptr_range().end);
    }
    #[test]
    fn rejects_truncation_untrusted_lengths_and_unsupported_headers() {
        let bytes = fixture(&resource(1039, b"", b"icc"), &[]);
        for length in 0..bytes.len() {
            assert!(PsdPreflight::inspect(&bytes[..length]).is_err(), "{length}");
        }
        for (offset, data) in [
            (4, vec![0, 2]),
            (6, vec![1]),
            (12, vec![0, 2]),
            (14, 16385u32.to_be_bytes().to_vec()),
            (18, 0u32.to_be_bytes().to_vec()),
            (22, vec![0, 16]),
            (24, vec![0, 4]),
            (26, u32::MAX.to_be_bytes().to_vec()),
            (30, u32::MAX.to_be_bytes().to_vec()),
            (bytes.len() - 2, vec![0, 4]),
        ] {
            let mut changed = bytes.clone();
            changed[offset..offset + data.len()].copy_from_slice(&data);
            assert!(PsdPreflight::inspect(&changed).is_err(), "offset {offset}");
        }
        let mut duplicates = resource(1039, b"", b"a");
        duplicates.extend(resource(1039, b"", b"b"));
        assert!(PsdPreflight::inspect(&fixture(&duplicates, &[])).is_err());
        let mut missing_padding = resource(1039, b"", b"a");
        missing_padding.pop();
        assert!(PsdPreflight::inspect(&fixture(&missing_padding, &[])).is_err());
        let excessive_layers = section(&i16::MIN.to_be_bytes());
        assert!(PsdPreflight::inspect(&fixture(&[], &excessive_layers)).is_err());
    }
}
