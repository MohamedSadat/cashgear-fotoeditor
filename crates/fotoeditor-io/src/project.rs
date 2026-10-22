//! Versioned, bounded sparse RGBA8 projects. No browser or GPU types.
//! Each step handles one tile; callers yield and can cancel before atomic commit.
use crate::MAX_FILE_BYTES;
use fotoeditor_core::{Document, Residency, Rgba8, TileCoord, TILE_PIXELS, TILE_SIZE};
use std::collections::HashSet;

const MAGIC: &[u8; 8] = b"CGFOTO\r\n";
const VERSION: u32 = 1;
const HEADER: usize = 36;
const TILE_BYTES: usize = TILE_PIXELS * 4;
const RECORD: usize = 8 + TILE_BYTES;

pub struct ProjectWriter {
    document: Document,
    coordinates: Vec<TileCoord>,
    next: usize,
    bytes: Vec<u8>,
    checksum: crc32fast::Hasher,
}
impl ProjectWriter {
    pub fn new(document: &Document, residency: Residency) -> Result<Self, String> {
        let length = HEADER + document.raster.tiles().len() * RECORD + 4;
        if length as u64 > MAX_FILE_BYTES {
            return Err("Project exceeds the 128 MiB file limit".into());
        }
        // Output and WASM/JS bridge copy coexist; Arcs share the unchanged document.
        residency.check_cpu(2 * length as u64 + RECORD as u64)?;
        let mut bytes = Vec::with_capacity(length);
        bytes.extend_from_slice(MAGIC);
        bytes.extend_from_slice(&VERSION.to_le_bytes());
        bytes.extend_from_slice(&document.width.to_le_bytes());
        bytes.extend_from_slice(&document.height.to_le_bytes());
        bytes.extend_from_slice(&document.dpi.to_le_bytes());
        bytes.extend_from_slice(&TILE_SIZE.to_le_bytes());
        bytes.extend_from_slice(&(document.raster.tiles().len() as u32).to_le_bytes());
        let mut coordinates: Vec<_> = document.raster.tiles().keys().copied().collect();
        coordinates.sort_unstable();
        let mut checksum = crc32fast::Hasher::new();
        checksum.update(&bytes);
        Ok(Self {
            document: document.clone(),
            coordinates,
            next: 0,
            bytes,
            checksum,
        })
    }
    pub fn temporary_bytes(&self) -> u64 {
        self.bytes.capacity() as u64 + (self.coordinates.capacity() * size_of::<TileCoord>()) as u64
    }
    pub fn step(&mut self) -> bool {
        if let Some(coord) = self.coordinates.get(self.next) {
            let start = self.bytes.len();
            self.bytes.extend_from_slice(&coord.0.to_le_bytes());
            self.bytes.extend_from_slice(&coord.1.to_le_bytes());
            let pixel_start = self.bytes.len();
            self.bytes.resize(pixel_start + TILE_BYTES, 0);
            for (target, p) in self.bytes[pixel_start..]
                .as_chunks_mut::<4>()
                .0
                .iter_mut()
                .zip(&self.document.raster.tiles()[coord].pixels)
            {
                *target = p.0;
            }
            self.checksum.update(&self.bytes[start..]);
            self.next += 1;
        }
        self.next == self.coordinates.len()
    }
    pub fn finish(mut self) -> Result<Vec<u8>, String> {
        if self.next != self.coordinates.len() {
            return Err("Project save is incomplete".into());
        }
        self.bytes
            .extend_from_slice(&self.checksum.finalize().to_le_bytes());
        Ok(self.bytes)
    }
}

pub struct ProjectReader {
    document: Document,
    bytes: Vec<u8>,
    count: usize,
    next: usize,
    seen: HashSet<TileCoord>,
    checksum: crc32fast::Hasher,
    materialize: bool,
}
fn u32_at(bytes: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap())
}
impl ProjectReader {
    pub fn new(bytes: &[u8], residency: Residency) -> Result<Self, String> {
        Self::read(bytes, residency, true)
    }
    pub fn validator(bytes: &[u8], residency: Residency) -> Result<Self, String> {
        Self::read(bytes, residency, false)
    }
    fn read(bytes: &[u8], residency: Residency, materialize: bool) -> Result<Self, String> {
        if bytes.len() < HEADER + 4 || bytes.len() as u64 > MAX_FILE_BYTES || &bytes[..8] != MAGIC {
            return Err("Invalid CashGear project (or file exceeds 128 MiB)".into());
        }
        if u32_at(bytes, 8) != VERSION {
            return Err("Unsupported project version; this editor supports version 1".into());
        }
        let width = u32_at(bytes, 12);
        let height = u32_at(bytes, 16);
        let dpi = f64::from_le_bytes(bytes[20..28].try_into().unwrap());
        let count = u32_at(bytes, 32) as usize;
        fotoeditor_core::validate_size(width, height, dpi)?;
        if u32_at(bytes, 28) != TILE_SIZE
            || count > (width.div_ceil(TILE_SIZE) * height.div_ceil(TILE_SIZE)) as usize
            || HEADER
                .checked_add(count * RECORD)
                .and_then(|n| n.checked_add(4))
                != Some(bytes.len())
        {
            return Err("Invalid or truncated project tile directory".into());
        }
        // Bridge input and retained parser bytes plus pending tile samples.
        // The worker accounts for its JS input in external residency before calling.
        residency.check_cpu(
            2 * bytes.len() as u64
                + if materialize {
                    count as u64 * TILE_BYTES as u64
                } else {
                    0
                }
                + RECORD as u64,
        )?;
        let mut checksum = crc32fast::Hasher::new();
        checksum.update(&bytes[..HEADER]);
        Ok(Self {
            document: Document::blank(width, height, dpi)?,
            bytes: bytes.to_vec(),
            count,
            next: 0,
            seen: HashSet::new(),
            checksum,
            materialize,
        })
    }
    pub fn temporary_bytes(&self) -> u64 {
        self.bytes.capacity() as u64 + self.document.raster.bytes()
    }
    pub fn step(&mut self) -> Result<bool, String> {
        if self.next < self.count {
            let start = HEADER + self.next * RECORD;
            let record = &self.bytes[start..start + RECORD];
            let coord = (u32_at(record, 0), u32_at(record, 4));
            if coord.0 >= self.document.width.div_ceil(TILE_SIZE)
                || coord.1 >= self.document.height.div_ceil(TILE_SIZE)
                || !self.seen.insert(coord)
            {
                return Err("Invalid or duplicate project tile coordinate".into());
            }
            if self.materialize {
                let pixels = record[8..]
                    .as_chunks::<4>()
                    .0
                    .iter()
                    .map(|p| Rgba8(*p))
                    .collect();
                self.document.raster.insert(coord, pixels);
            }
            self.checksum.update(record);
            self.next += 1;
        }
        if self.next == self.count
            && self.checksum.clone().finalize() != u32_at(&self.bytes, self.bytes.len() - 4)
        {
            return Err("Project checksum failed; the file is incomplete or corrupt".into());
        }
        Ok(self.next == self.count)
    }
    pub fn finish(self) -> Result<Document, String> {
        if !self.materialize
            || self.next != self.count
            || self.checksum.finalize() != u32_at(&self.bytes, self.bytes.len() - 4)
        {
            return Err("Project load is incomplete or corrupt".into());
        }
        Ok(self.document)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn encode(doc: &Document) -> Vec<u8> {
        let mut job = ProjectWriter::new(doc, Residency::default()).unwrap();
        while !job.step() {}
        job.finish().unwrap()
    }
    fn decode(bytes: &[u8]) -> Result<Document, String> {
        let mut job = ProjectReader::new(bytes, Residency::default())?;
        while !job.step()? {}
        job.finish()
    }
    #[test]
    fn project_roundtrip_is_exact_sparse_and_deterministic() {
        let mut d = Document::blank(513, 259, 299.123456789).unwrap();
        for (x, y, p) in [
            (0, 0, [87, 33, 19, 0]),
            (255, 256, [201, 3, 7, 1]),
            (512, 258, [3, 4, 5, 255]),
        ] {
            d.raster.write(x, y, Rgba8(p));
        }
        let bytes = encode(&d);
        let reopened = decode(&bytes).unwrap();
        assert_eq!(reopened.dpi.to_bits(), d.dpi.to_bits());
        assert_eq!(reopened.raster.tiles().len(), 3);
        assert_eq!(bytes, encode(&reopened));
        for y in 0..259 {
            for x in 0..513 {
                assert_eq!(d.raster.sample(x, y), reopened.raster.sample(x, y));
            }
        }
        let empty = encode(&Document::blank(16384, 16384, 72.0).unwrap());
        assert_eq!(empty.len(), HEADER + 4);
        assert_eq!(decode(&empty).unwrap().raster.bytes(), 0);
    }
    #[test]
    fn invalid_projects_never_complete() {
        let mut d = Document::blank(257, 1, 72.0).unwrap();
        d.raster.write(0, 0, Rgba8([4, 5, 6, 0]));
        d.raster.write(256, 0, Rgba8([7, 8, 9, 1]));
        let bytes = encode(&d);
        for n in [0, 7, 35, 36, 100, bytes.len() - 1] {
            assert!(decode(&bytes[..n]).is_err());
        }
        for offset in [8, 12, 16, 20, 28, 32, 36, 100, bytes.len() - 1] {
            let mut bad = bytes.clone();
            bad[offset] ^= 255;
            assert!(decode(&bad).is_err(), "{offset}");
        }
        let mut duplicate = bytes.clone();
        duplicate[HEADER + RECORD..HEADER + RECORD + 8].copy_from_slice(&bytes[HEADER..HEADER + 8]);
        assert!(decode(&duplicate).is_err());
    }
}
