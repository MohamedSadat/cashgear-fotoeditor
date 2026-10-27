//! Restricted interoperable ZIP32 framing for streamed projects. Entries are
//! stored; tile payloads contain bounded LZ4 blocks. No filesystem paths are used.
use crate::{MAX_PROJECT_BYTES, PROJECT_CHUNK_BYTES};
use fotoeditor_core::Residency;
use std::collections::HashSet;

pub const ZIP_SCRATCH: u64 = 32 * 1024 * 1024;
pub const MAX_ZIP_DIRECTORY: usize = 8 * 1024 * 1024;
pub const MAX_ZIP_ENTRIES: usize = 60_000;

fn valid_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 64
        && name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"/._-".contains(&b))
        && name
            .split('/')
            .all(|s| !s.is_empty() && s != "." && s != "..")
}
fn u16_at(bytes: &[u8], at: usize) -> Result<u16, String> {
    Ok(u16::from_le_bytes(
        bytes
            .get(at..at + 2)
            .ok_or("Truncated ZIP header")?
            .try_into()
            .unwrap(),
    ))
}
fn u32_at(bytes: &[u8], at: usize) -> Result<u32, String> {
    Ok(u32::from_le_bytes(
        bytes
            .get(at..at + 4)
            .ok_or("Truncated ZIP header")?
            .try_into()
            .unwrap(),
    ))
}
fn p16(bytes: &mut Vec<u8>, value: u16) {
    bytes.extend(value.to_le_bytes());
}
fn p32(bytes: &mut Vec<u8>, value: u32) {
    bytes.extend(value.to_le_bytes());
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ZipEntry {
    pub name: String,
    pub length: u32,
    pub checksum: u32,
    pub local_offset: u32,
    flags: u16,
}
impl ZipEntry {
    pub fn header_length(&self) -> usize {
        30 + self.name.len()
    }
    pub fn payload_offset(&self) -> u64 {
        self.local_offset as u64 + self.header_length() as u64
    }
    pub fn validate_header(&self, bytes: &[u8]) -> Result<(), String> {
        if bytes.len() != self.header_length()
            || u32_at(bytes, 0)? != 0x04034b50
            || u16_at(bytes, 4)? != 20
            || u16_at(bytes, 6)? != self.flags
            || u16_at(bytes, 8)? != 0
            || u32_at(bytes, 14)? != self.checksum
            || u32_at(bytes, 18)? != self.length
            || u32_at(bytes, 22)? != self.length
            || u16_at(bytes, 26)? as usize != self.name.len()
            || u16_at(bytes, 28)? != 0
            || &bytes[30..] != self.name.as_bytes()
        {
            return Err("ZIP local header disagrees with its directory".into());
        }
        Ok(())
    }
    pub fn validate_payload(&self, payload: &[u8]) -> Result<(), String> {
        if payload.len() != self.length as usize || crc32fast::hash(payload) != self.checksum {
            return Err("ZIP entry length or checksum failed".into());
        }
        Ok(())
    }
}

/// Obtain the central-directory range using only the final 22 bytes. Our project
/// profile has no comments, encryption, ZIP64, extras, data descriptors or disks.
#[derive(Clone, Copy, Debug)]
pub struct ZipDirectoryRange {
    pub offset: u32,
    pub length: u32,
    pub entries: u16,
}
impl ZipDirectoryRange {
    pub fn inspect(tail: &[u8], file_length: u64) -> Result<Self, String> {
        if tail.len() != 22
            || !(22..=MAX_PROJECT_BYTES).contains(&file_length)
            || u32_at(tail, 0)? != 0x06054b50
            || u16_at(tail, 4)? != 0
            || u16_at(tail, 6)? != 0
            || u16_at(tail, 8)? != u16_at(tail, 10)?
            || u16_at(tail, 20)? != 0
        {
            return Err("Invalid or unsupported project ZIP end record".into());
        }
        let range = Self {
            entries: u16_at(tail, 10)?,
            length: u32_at(tail, 12)?,
            offset: u32_at(tail, 16)?,
        };
        if range.entries == 0
            || range.entries as usize > MAX_ZIP_ENTRIES
            || range.length as usize > MAX_ZIP_DIRECTORY
            || range.offset as u64 + range.length as u64 + 22 != file_length
        {
            return Err("Invalid or excessive project ZIP directory".into());
        }
        Ok(range)
    }
    pub fn parse(&self, bytes: &[u8], residency: Residency) -> Result<Vec<ZipEntry>, String> {
        residency.check_cpu(ZIP_SCRATCH)?;
        if bytes.len() != self.length as usize
            || bytes.len() > MAX_ZIP_DIRECTORY
            || self.entries as usize > MAX_ZIP_ENTRIES
        {
            return Err("ZIP directory exceeds its declared bounds".into());
        }
        let mut offset = 0usize;
        let mut local = 0u64;
        let mut entries = Vec::new();
        let mut names = HashSet::new();
        for _ in 0..self.entries {
            let header = bytes
                .get(offset..offset + 46)
                .ok_or("Truncated ZIP directory entry")?;
            let name_len = u16_at(header, 28)? as usize;
            let flags = u16_at(header, 8)?;
            if u32_at(header, 0)? != 0x02014b50
                || u16_at(header, 6)? != 20
                || flags & !0x800 != 0
                || u16_at(header, 10)? != 0
                || u16_at(header, 30)? != 0
                || u16_at(header, 32)? != 0
                || u16_at(header, 34)? != 0
                || u32_at(header, 20)? != u32_at(header, 24)?
                || name_len > 64
            {
                return Err("Unsupported project ZIP entry flags, method, extras or sizes".into());
            }
            let name = std::str::from_utf8(
                bytes
                    .get(offset + 46..offset + 46 + name_len)
                    .ok_or("Truncated ZIP entry name")?,
            )
            .map_err(|_| "Invalid ZIP entry name")?
            .to_owned();
            if !valid_name(&name) || !names.insert(name.clone()) {
                return Err("Unsafe or duplicate project ZIP name".into());
            }
            let entry = ZipEntry {
                name,
                flags,
                checksum: u32_at(header, 16)?,
                length: u32_at(header, 24)?,
                local_offset: u32_at(header, 42)?,
            };
            if entry.local_offset as u64 != local {
                return Err("ZIP entries overlap or leave unvalidated gaps".into());
            }
            local = entry.payload_offset() + entry.length as u64;
            if local > self.offset as u64 {
                return Err("ZIP entry overlaps the directory".into());
            }
            entries.push(entry);
            offset += 46 + name_len;
        }
        if offset != bytes.len() || local != self.offset as u64 {
            return Err("Unexpected project ZIP directory or local data".into());
        }
        Ok(entries)
    }
}

pub struct ZipStreamWriter {
    entries: Vec<ZipEntry>,
    names: HashSet<String>,
    offset: u64,
    active: Option<(ZipEntry, u64, crc32fast::Hasher)>,
    failed: bool,
    directory_bytes: usize,
}
impl ZipStreamWriter {
    pub fn new(residency: Residency) -> Result<Self, String> {
        residency.check_cpu(ZIP_SCRATCH)?;
        Ok(Self {
            entries: Vec::new(),
            names: HashSet::new(),
            offset: 0,
            active: None,
            failed: false,
            directory_bytes: 0,
        })
    }
    pub fn start(&mut self, name: &str, length: u32, checksum: u32) -> Result<Vec<u8>, String> {
        if self.failed
            || self.active.is_some()
            || !valid_name(name)
            || self.names.contains(name)
            || self.entries.len() >= MAX_ZIP_ENTRIES
        {
            return Err("Invalid, duplicate or unfinished ZIP entry".into());
        }
        let prospective_directory = (self.directory_bytes + 46 + name.len()) as u64;
        if prospective_directory > MAX_ZIP_DIRECTORY as u64
            || self.offset + 30 + name.len() as u64 + length as u64 + prospective_directory + 22
                > MAX_PROJECT_BYTES
        {
            return Err("ZIP project exceeds its directory or 4 GiB file limit".into());
        }
        let entry = ZipEntry {
            name: name.into(),
            length,
            checksum,
            local_offset: self
                .offset
                .try_into()
                .map_err(|_| "ZIP offset exceeds 32 bits")?,
            flags: 0x800,
        };
        let mut header = Vec::with_capacity(entry.header_length());
        p32(&mut header, 0x04034b50);
        p16(&mut header, 20);
        p16(&mut header, entry.flags);
        p16(&mut header, 0);
        p16(&mut header, 0);
        p16(&mut header, 0);
        p32(&mut header, checksum);
        p32(&mut header, length);
        p32(&mut header, length);
        p16(&mut header, name.len() as u16);
        p16(&mut header, 0);
        header.extend(name.as_bytes());
        self.offset += header.len() as u64;
        self.active = Some((entry, 0, crc32fast::Hasher::new()));
        Ok(header)
    }
    pub fn data(&mut self, bytes: &[u8]) -> Result<(), String> {
        let (entry, written, checksum) = self.active.as_mut().ok_or("ZIP entry has not begun")?;
        if self.failed
            || bytes.len() > PROJECT_CHUNK_BYTES
            || *written + bytes.len() as u64 > entry.length as u64
        {
            self.failed = true;
            return Err("ZIP entry chunk exceeds its bound".into());
        }
        checksum.update(bytes);
        *written += bytes.len() as u64;
        self.offset += bytes.len() as u64;
        Ok(())
    }
    pub fn end(&mut self) -> Result<(), String> {
        let (entry, written, checksum) = self.active.take().ok_or("ZIP entry has not begun")?;
        if self.failed || written != entry.length as u64 || checksum.finalize() != entry.checksum {
            self.failed = true;
            return Err("Incomplete or changed ZIP entry".into());
        }
        self.names.insert(entry.name.clone());
        self.directory_bytes += 46 + entry.name.len();
        self.entries.push(entry);
        Ok(())
    }
    pub fn record(&mut self, name: &str, bytes: &[u8]) -> Result<Vec<u8>, String> {
        if bytes.len() > PROJECT_CHUNK_BYTES {
            return Err("Use bounded chunks for large ZIP entries".into());
        }
        let mut packet = self.start(name, bytes.len() as u32, crc32fast::hash(bytes))?;
        self.data(bytes)?;
        self.end()?;
        packet.extend(bytes);
        Ok(packet)
    }
    pub fn finish(self) -> Result<Vec<u8>, String> {
        if self.failed || self.active.is_some() || self.entries.is_empty() {
            return Err("Incomplete project ZIP".into());
        }
        let mut directory = Vec::new();
        for e in &self.entries {
            p32(&mut directory, 0x02014b50);
            p16(&mut directory, 20);
            p16(&mut directory, 20);
            p16(&mut directory, e.flags);
            p16(&mut directory, 0);
            p16(&mut directory, 0);
            p16(&mut directory, 0);
            p32(&mut directory, e.checksum);
            p32(&mut directory, e.length);
            p32(&mut directory, e.length);
            p16(&mut directory, e.name.len() as u16);
            p16(&mut directory, 0);
            p16(&mut directory, 0);
            p16(&mut directory, 0);
            p16(&mut directory, 0);
            p32(&mut directory, 0);
            p32(&mut directory, e.local_offset);
            directory.extend(e.name.as_bytes());
        }
        let length = directory.len() as u32;
        p32(&mut directory, 0x06054b50);
        p16(&mut directory, 0);
        p16(&mut directory, 0);
        p16(&mut directory, self.entries.len() as u16);
        p16(&mut directory, self.entries.len() as u16);
        p32(&mut directory, length);
        p32(&mut directory, self.offset as u32);
        p16(&mut directory, 0);
        Ok(directory)
    }
}

pub fn encode_lz4_tile(bytes: &[u8]) -> Result<Vec<u8>, String> {
    if !matches!(bytes.len(), 65536 | 262144) {
        return Err("Invalid project tile sample length".into());
    }
    Ok(lz4_flex::compress_prepend_size(bytes))
}
pub fn decode_lz4_tile(
    bytes: &[u8],
    expected: usize,
    residency: Residency,
) -> Result<Vec<u8>, String> {
    if !matches!(expected, 65536 | 262144)
        || bytes.len() > lz4_flex::block::get_maximum_output_size(expected) + 4
        || u32_at(bytes, 0)? as usize != expected
    {
        return Err("Invalid compressed project tile length".into());
    }
    residency.check_cpu((expected as u64) * 3)?;
    let mut output = vec![0; expected];
    let written = lz4_flex::decompress_into(&bytes[4..], &mut output).map_err(|e| e.to_string())?;
    if written != expected {
        return Err("Incomplete LZ4 project tile".into());
    }
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn archive() -> Vec<u8> {
        let mut writer = ZipStreamWriter::new(Residency::default()).unwrap();
        let mut bytes = writer.record("manifest.json", b"{}").unwrap();
        bytes.extend(
            writer
                .record(
                    "tiles/1.rgba.lz4",
                    &encode_lz4_tile(&vec![79; 262144]).unwrap(),
                )
                .unwrap(),
        );
        bytes.extend(writer.finish().unwrap());
        bytes
    }
    #[test]
    fn stored_zip_and_lz4_roundtrip_with_bounded_headers_and_payloads() {
        let bytes = archive();
        let range =
            ZipDirectoryRange::inspect(&bytes[bytes.len() - 22..], bytes.len() as u64).unwrap();
        let entries = range
            .parse(
                &bytes[range.offset as usize..][..range.length as usize],
                Residency::default(),
            )
            .unwrap();
        assert_eq!(entries.len(), 2);
        for entry in &entries {
            let at = entry.local_offset as usize;
            entry
                .validate_header(&bytes[at..at + entry.header_length()])
                .unwrap();
            let payload = &bytes[entry.payload_offset() as usize..][..entry.length as usize];
            entry.validate_payload(payload).unwrap();
        }
        let e = &entries[1];
        assert_eq!(
            decode_lz4_tile(
                &bytes[e.payload_offset() as usize..][..e.length as usize],
                262144,
                Residency::default()
            )
            .unwrap(),
            vec![79; 262144]
        );
        assert_eq!(bytes, archive());
    }
    #[test]
    fn malformed_zip_names_sizes_overlap_and_lz4_expansion_are_rejected() {
        let bytes = archive();
        let tail = &bytes[bytes.len() - 22..];
        let range = ZipDirectoryRange::inspect(tail, bytes.len() as u64).unwrap();
        let directory = &bytes[range.offset as usize..][..range.length as usize];
        for at in [0, 6, 8, 10, 20, 28, 30, 32, 34, 42] {
            let mut changed = directory.to_vec();
            changed[at] ^= 1;
            assert!(range.parse(&changed, Residency::default()).is_err(), "{at}");
        }
        for length in 0..22 {
            assert!(ZipDirectoryRange::inspect(&tail[..length], bytes.len() as u64).is_err());
        }
        for name in ["../bad", "/absolute", "a//b", "a/../b", "a\\b", ""] {
            assert!(ZipStreamWriter::new(Residency::default())
                .unwrap()
                .record(name, b"x")
                .is_err());
        }
        assert!(decode_lz4_tile(&u32::MAX.to_le_bytes(), 262144, Residency::default()).is_err());
        let mut compressed = encode_lz4_tile(&vec![0; 65536]).unwrap();
        compressed.truncate(5);
        assert!(decode_lz4_tile(&compressed, 65536, Residency::default()).is_err());
        let mut writer = ZipStreamWriter::new(Residency::default()).unwrap();
        writer.start("x", 3, 0).unwrap();
        assert!(writer.end().is_err());
        assert!(writer.finish().is_err());
    }
}
