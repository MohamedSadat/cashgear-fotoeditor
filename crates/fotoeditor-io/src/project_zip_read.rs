//! Request-driven ZIP to immutable raw backing conversion. The worker transports
//! requested byte ranges and writes returned packets; Rust validates all framing,
//! CRCs, LZ4 bounds and preview PNG chunks before authorizing a complete stream.
use crate::{
    decode_lz4_tile, ProjectRecordKind, ProjectStreamWriter, ZipDirectoryRange, ZipEntry,
    LAYER_PROJECT_SCRATCH, MAX_FILE_BYTES, MAX_PROJECT_BYTES, PROJECT_CHUNK_BYTES, ZIP_SCRATCH,
};
use fotoeditor_core::Residency;

#[derive(Clone, Copy, Debug)]
pub struct ZipReadRequest {
    pub offset: u64,
    pub length: usize,
}

#[derive(Default)]
struct PngCheck {
    state: u8,
    buffer: Vec<u8>,
    remaining: usize,
    kind: [u8; 4],
    checksum: crc32fast::Hasher,
    chunks: u32,
    idat: bool,
    ended: bool,
    thumbnail: bool,
}
impl PngCheck {
    fn feed(&mut self, mut bytes: &[u8]) -> Result<(), String> {
        while !bytes.is_empty() {
            if self.ended {
                return Err("Trailing bytes in project PNG preview".into());
            }
            if self.state == 2 {
                let n = self.remaining.min(bytes.len());
                self.checksum.update(&bytes[..n]);
                if &self.kind == b"IHDR" {
                    self.buffer.extend(&bytes[..n]);
                }
                self.remaining -= n;
                bytes = &bytes[n..];
                if self.remaining == 0 {
                    if &self.kind == b"IHDR" {
                        let w = u32::from_be_bytes(self.buffer[..4].try_into().unwrap());
                        let h = u32::from_be_bytes(self.buffer[4..8].try_into().unwrap());
                        fotoeditor_core::validate_size(w, h, 72.0)?;
                        if self.thumbnail && (w > 64 || h > 64)
                            || self.buffer[8..] != [8, 6, 0, 0, 0]
                        {
                            return Err(
                                "Unsupported project PNG preview dimensions or sample format"
                                    .into(),
                            );
                        }
                    }
                    self.buffer.clear();
                    self.state = 3;
                }
                continue;
            }
            let target = if self.state == 3 { 4 } else { 8 };
            let n = (target - self.buffer.len()).min(bytes.len());
            self.buffer.extend(&bytes[..n]);
            bytes = &bytes[n..];
            if self.buffer.len() != target {
                continue;
            }
            match self.state {
                0 => {
                    if self.buffer != b"\x89PNG\r\n\x1a\n" {
                        return Err("Invalid project PNG preview".into());
                    }
                    self.state = 1;
                }
                1 => {
                    self.remaining =
                        u32::from_be_bytes(self.buffer[..4].try_into().unwrap()) as usize;
                    self.kind.copy_from_slice(&self.buffer[4..8]);
                    if self.remaining as u64 > MAX_FILE_BYTES
                        || self.chunks == 65536
                        || self.chunks == 0 && (&self.kind != b"IHDR" || self.remaining != 13)
                        || self.chunks != 0 && &self.kind == b"IHDR"
                        || &self.kind == b"IEND" && self.remaining != 0
                    {
                        return Err("Invalid or excessive project PNG chunks".into());
                    }
                    self.chunks += 1;
                    self.idat |= &self.kind == b"IDAT";
                    self.checksum = crc32fast::Hasher::new();
                    self.checksum.update(&self.kind);
                    self.state = if self.remaining == 0 { 3 } else { 2 };
                }
                3 => {
                    if u32::from_be_bytes(self.buffer[..4].try_into().unwrap())
                        != self.checksum.clone().finalize()
                    {
                        return Err("Project PNG preview checksum failed".into());
                    }
                    self.ended = &self.kind == b"IEND";
                    self.state = 1;
                }
                _ => unreachable!(),
            }
            self.buffer.clear();
        }
        Ok(())
    }
    fn finish(&self) -> Result<(), String> {
        if !self.ended || !self.idat || !self.buffer.is_empty() {
            return Err("Incomplete project PNG preview".into());
        }
        Ok(())
    }
}

fn tile_kind(name: &str) -> Result<(u64, ProjectRecordKind, usize), String> {
    let name = name
        .strip_prefix("tiles/")
        .ok_or("Unknown project ZIP entry")?;
    let (text, kind, length) = if let Some(n) = name.strip_suffix(".rgba.lz4") {
        (n, ProjectRecordKind::RgbaTile, 262144)
    } else if let Some(n) = name.strip_suffix(".gray.lz4") {
        (n, ProjectRecordKind::GrayTile, 65536)
    } else {
        return Err("Unknown project ZIP tile type".into());
    };
    let id: u64 = text
        .parse()
        .map_err(|_| "Invalid project ZIP generation ID")?;
    if id == 0 || id.to_string() != text {
        return Err("Invalid project ZIP generation ID".into());
    }
    Ok((id, kind, length))
}

pub struct ZipProjectReader {
    file_length: u64,
    range: Option<ZipDirectoryRange>,
    entries: Vec<ZipEntry>,
    stage: u8,
    index: usize,
    consumed: usize,
    buffer: Vec<u8>,
    crc: crc32fast::Hasher,
    png: Option<PngCheck>,
    output: Option<Vec<u8>>,
    legacy: Option<ProjectStreamWriter>,
    failed: bool,
    written: u64,
}
impl ZipProjectReader {
    pub fn new(file_length: u64, residency: Residency) -> Result<Self, String> {
        residency.check_cpu(ZIP_SCRATCH + LAYER_PROJECT_SCRATCH)?;
        if !(22..=MAX_PROJECT_BYTES).contains(&file_length) {
            return Err("Invalid project ZIP file size".into());
        }
        Ok(Self {
            file_length,
            range: None,
            entries: Vec::new(),
            stage: 0,
            index: 0,
            consumed: 0,
            buffer: Vec::new(),
            crc: crc32fast::Hasher::new(),
            png: None,
            output: None,
            legacy: None,
            failed: false,
            written: 0,
        })
    }
    pub fn temporary_bytes(&self) -> u64 {
        ZIP_SCRATCH + LAYER_PROJECT_SCRATCH
    }
    pub fn request(&self) -> Result<Option<ZipReadRequest>, String> {
        if self.failed {
            return Err("Project ZIP decoder has failed".into());
        }
        if self.output.is_some() {
            return Err("Write the pending project output before reading more".into());
        }
        Ok(match self.stage {
            0 => Some(ZipReadRequest {
                offset: self.file_length - 22,
                length: 22,
            }),
            1 => {
                let r = self.range.unwrap();
                Some(ZipReadRequest {
                    offset: r.offset as u64 + self.buffer.len() as u64,
                    length: (r.length as usize - self.buffer.len()).min(PROJECT_CHUNK_BYTES),
                })
            }
            2 => {
                let e = &self.entries[self.index];
                Some(ZipReadRequest {
                    offset: e.local_offset as u64,
                    length: e.header_length(),
                })
            }
            3 => {
                let e = &self.entries[self.index];
                Some(ZipReadRequest {
                    offset: e.payload_offset() + self.consumed as u64,
                    length: (e.length as usize - self.consumed).min(PROJECT_CHUNK_BYTES),
                })
            }
            4 => None,
            _ => return Err("Invalid project ZIP reader stage".into()),
        })
    }
    pub fn take_output(&mut self) -> Option<Vec<u8>> {
        let output = self.output.take();
        if let Some(b) = &output {
            self.written += b.len() as u64;
        }
        output
    }
    pub fn feed(&mut self, bytes: &[u8], residency: Residency) -> Result<(), String> {
        if self.failed {
            return Err("Project ZIP decoder has failed".into());
        }
        let result = self.feed_inner(bytes, residency);
        if result.is_err() {
            self.failed = true;
        }
        result
    }
    fn feed_inner(&mut self, bytes: &[u8], residency: Residency) -> Result<(), String> {
        residency.check_cpu(self.temporary_bytes())?;
        let request = self.request()?.ok_or("Project ZIP is already complete")?;
        if bytes.len() != request.length {
            return Err("Incomplete project ZIP input range".into());
        }
        match self.stage {
            0 => {
                self.range = Some(ZipDirectoryRange::inspect(bytes, self.file_length)?);
                self.stage = 1;
            }
            1 => {
                self.buffer.extend(bytes);
                let r = self.range.unwrap();
                if self.buffer.len() == r.length as usize {
                    self.entries = r.parse(&self.buffer, residency)?;
                    self.buffer = Vec::new();
                    if self.entries.len() < 3
                        || self.entries[0].name != "manifest.json"
                        || self.entries[self.entries.len() - 2].name != "thumbnail.png"
                        || self.entries.last().unwrap().name != "composite.png"
                        || self.entries[0].length == 0
                        || self.entries[0].length as usize > PROJECT_CHUNK_BYTES
                    {
                        return Err("ZIP project must contain manifest, tiles, thumbnail and composite in order".into());
                    }
                    let mut decoded = 68 + self.entries[0].length as u64;
                    for (i, e) in self.entries.iter().enumerate().skip(1) {
                        if i >= self.entries.len() - 2 {
                            if e.length == 0
                                || e.length as u64 > MAX_FILE_BYTES
                                || i == self.entries.len() - 2
                                    && e.length as usize > PROJECT_CHUNK_BYTES
                            {
                                return Err("Project preview exceeds its file budget".into());
                            }
                        } else {
                            let (_, _, length) = tile_kind(&e.name)?;
                            if e.length < 4
                                || e.length as usize
                                    > lz4_flex::block::get_maximum_output_size(length) + 4
                            {
                                return Err("Compressed project tile exceeds its bound".into());
                            }
                            decoded += 20 + length as u64;
                        }
                    }
                    if decoded > MAX_PROJECT_BYTES {
                        return Err("Expanded project exceeds the 4 GiB backing limit".into());
                    }
                    let legacy = ProjectStreamWriter::new(residency)?;
                    self.output = Some(legacy.signature().to_vec());
                    self.legacy = Some(legacy);
                    self.stage = 2;
                }
            }
            2 => {
                let e = &self.entries[self.index];
                e.validate_header(bytes)?;
                self.consumed = 0;
                self.crc = crc32fast::Hasher::new();
                self.png = if self.index >= self.entries.len() - 2 {
                    Some(PngCheck {
                        thumbnail: self.index == self.entries.len() - 2,
                        ..PngCheck::default()
                    })
                } else {
                    None
                };
                self.buffer.clear();
                self.stage = 3;
            }
            3 => {
                let e = &self.entries[self.index];
                self.crc.update(bytes);
                self.consumed += bytes.len();
                if let Some(png) = &mut self.png {
                    png.feed(bytes)?;
                } else {
                    self.buffer.extend(bytes);
                }
                if self.consumed == e.length as usize {
                    if self.crc.clone().finalize() != e.checksum {
                        return Err("Project ZIP entry checksum failed".into());
                    }
                    if let Some(png) = self.png.take() {
                        png.finish()?;
                    } else if self.index == 0 {
                        self.output = Some(self.legacy.as_mut().unwrap().record(
                            ProjectRecordKind::Manifest,
                            0,
                            &self.buffer,
                        )?);
                    } else {
                        let (id, kind, length) = tile_kind(&e.name)?;
                        let decoded = decode_lz4_tile(&self.buffer, length, residency)?;
                        self.output =
                            Some(self.legacy.as_mut().unwrap().record(kind, id, &decoded)?);
                    }
                    self.buffer.clear();
                    self.index += 1;
                    if self.index == self.entries.len() {
                        self.output = Some(self.legacy.take().unwrap().finish()?);
                        self.stage = 4;
                    } else {
                        self.stage = 2;
                    }
                }
            }
            _ => return Err("Project ZIP is already complete".into()),
        }
        Ok(())
    }
    pub fn finish(self) -> Result<u64, String> {
        if self.failed || self.stage != 4 || self.output.is_some() {
            return Err("Project ZIP is incomplete or corrupt".into());
        }
        Ok(self.written)
    }
}
