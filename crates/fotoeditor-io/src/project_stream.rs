//! Bounded v2 project framing. Pixel/manifest validation is performed by the
//! document adapter before atomic commit; a valid frame alone is not a document.
use fotoeditor_core::Residency;
use std::collections::HashSet;

pub const MAX_PROJECT_BYTES: u64 = 4 * 1024 * 1024 * 1024;
pub const PROJECT_CHUNK_BYTES: usize = 1024 * 1024;
const SIGNATURE: &[u8; 12] = b"CGFOTO\r\n\x02\0\0\0";
const PREFIX: usize = 20;
const FOOTER_BYTES: usize = 16;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u32)]
pub enum ProjectRecordKind {
    Manifest = 1,
    RgbaTile = 2,
    GrayTile = 3,
}
impl ProjectRecordKind {
    fn validate_length(self, length: usize) -> bool {
        match self {
            Self::Manifest => (1..=PROJECT_CHUNK_BYTES).contains(&length),
            Self::RgbaTile => length == 256 * 256 * 4,
            Self::GrayTile => length == 256 * 256,
        }
    }
}
#[derive(Debug)]
pub struct ProjectRecord {
    pub kind: ProjectRecordKind,
    pub id: u64,
    pub payload: Vec<u8>,
    /// Raw payload position in the immutable backing project stream.
    pub file_offset: u64,
}
fn u32_at(bytes: &[u8], at: usize) -> u32 {
    u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap())
}
fn u64_at(bytes: &[u8], at: usize) -> u64 {
    u64::from_le_bytes(bytes[at..at + 8].try_into().unwrap())
}
fn packet(kind: u32, id: u64, payload: &[u8]) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(PREFIX + payload.len());
    bytes.extend_from_slice(&kind.to_le_bytes());
    bytes.extend_from_slice(&id.to_le_bytes());
    bytes.extend_from_slice(&(payload.len() as u32).to_le_bytes());
    bytes.extend_from_slice(&crc32fast::hash(payload).to_le_bytes());
    bytes.extend_from_slice(payload);
    bytes
}
#[derive(Default)]
struct Directory {
    manifest: bool,
    ids: HashSet<u64>,
    count: u32,
}
impl Directory {
    fn accept(&mut self, kind: u32, id: u64, length: usize) -> Result<ProjectRecordKind, String> {
        let kind = match kind {
            1 => ProjectRecordKind::Manifest,
            2 => ProjectRecordKind::RgbaTile,
            3 => ProjectRecordKind::GrayTile,
            _ => return Err("Unknown project record type".into()),
        };
        if !kind.validate_length(length) {
            return Err("Invalid project record length".into());
        }
        if kind == ProjectRecordKind::Manifest {
            if self.manifest || self.count != 0 || id != 0 {
                return Err("Project must begin with exactly one manifest".into());
            }
            self.manifest = true;
        } else if !self.manifest || id == 0 || !self.ids.insert(id) {
            return Err("Missing manifest or duplicate/invalid tile generation ID".into());
        }
        self.count += 1;
        Ok(kind)
    }
}
/// Produces one bounded packet at a time. Callers stream each packet to storage
/// and release it before requesting another. The signature is written first.
pub struct ProjectStreamWriter {
    directory: Directory,
    length: u64,
    checksum: crc32fast::Hasher,
}
impl ProjectStreamWriter {
    pub fn new(residency: Residency) -> Result<Self, String> {
        // Input payload, framed output and one bridge copy may coexist.
        residency.check_cpu(3 * (PROJECT_CHUNK_BYTES + PREFIX) as u64)?;
        let mut checksum = crc32fast::Hasher::new();
        checksum.update(SIGNATURE);
        Ok(Self {
            directory: Directory::default(),
            length: SIGNATURE.len() as u64,
            checksum,
        })
    }
    pub fn signature(&self) -> &'static [u8] {
        SIGNATURE
    }
    pub fn record(
        &mut self,
        kind: ProjectRecordKind,
        id: u64,
        payload: &[u8],
    ) -> Result<Vec<u8>, String> {
        if self.length + PREFIX as u64 + payload.len() as u64 + (PREFIX + FOOTER_BYTES) as u64
            > MAX_PROJECT_BYTES
        {
            return Err("Project exceeds the 4 GiB streamed file limit".into());
        }
        self.directory.accept(kind as u32, id, payload.len())?;
        let bytes = packet(kind as u32, id, payload);
        self.checksum.update(&bytes);
        self.length += bytes.len() as u64;
        Ok(bytes)
    }
    pub fn finish(self) -> Result<Vec<u8>, String> {
        if !self.directory.manifest {
            return Err("Project has no manifest".into());
        }
        let mut payload = Vec::with_capacity(FOOTER_BYTES);
        payload.extend_from_slice(&self.directory.count.to_le_bytes());
        payload.extend_from_slice(&(self.length + (PREFIX + FOOTER_BYTES) as u64).to_le_bytes());
        payload.extend_from_slice(&self.checksum.finalize().to_le_bytes());
        Ok(packet(255, 0, &payload))
    }
}
/// Incremental framing validation. `feed` returns at most one record so adapters
/// can validate/store it before reading more. Only `finish` authorizes a candidate
/// to proceed to document-level validation; no partial project is committed here.
pub struct ProjectStreamReader {
    expected: u64,
    consumed: u64,
    signature: bool,
    ended: bool,
    failed: bool,
    buffer: Vec<u8>,
    target: usize,
    directory: Directory,
    checksum: crc32fast::Hasher,
}
impl ProjectStreamReader {
    pub fn new(expected: u64, residency: Residency) -> Result<Self, String> {
        if !(SIGNATURE.len() as u64 + (2 * PREFIX + 1 + FOOTER_BYTES) as u64..=MAX_PROJECT_BYTES)
            .contains(&expected)
        {
            return Err("Invalid streamed project length (maximum 4 GiB)".into());
        }
        // A worker input slice, parser packet and one transfer/decode buffer.
        residency.check_cpu(3 * (PROJECT_CHUNK_BYTES + PREFIX) as u64)?;
        Ok(Self {
            expected,
            consumed: 0,
            signature: false,
            ended: false,
            failed: false,
            buffer: Vec::with_capacity(SIGNATURE.len()),
            target: SIGNATURE.len(),
            directory: Directory::default(),
            checksum: crc32fast::Hasher::new(),
        })
    }
    pub fn temporary_bytes(&self) -> u64 {
        self.buffer.capacity() as u64
    }
    pub fn feed(&mut self, input: &[u8]) -> Result<(usize, Option<ProjectRecord>), String> {
        if self.failed {
            return Err("Project parser has failed; discard the candidate".into());
        }
        let result = self.feed_inner(input);
        if result.is_err() {
            self.failed = true;
        }
        result
    }
    fn feed_inner(&mut self, input: &[u8]) -> Result<(usize, Option<ProjectRecord>), String> {
        if input.len() > PROJECT_CHUNK_BYTES {
            return Err("Project input chunks must be at most 1 MiB".into());
        }
        if self.ended {
            return if input.is_empty() {
                Ok((0, None))
            } else {
                Err("Unexpected data after project end".into())
            };
        }
        let mut used = 0;
        loop {
            let n = (self.target - self.buffer.len()).min(input.len() - used);
            if self.consumed + n as u64 > self.expected {
                return Err("Project exceeds its declared file length".into());
            }
            self.buffer.extend_from_slice(&input[used..used + n]);
            used += n;
            self.consumed += n as u64;
            if self.buffer.len() < self.target {
                return Ok((used, None));
            }
            if !self.signature {
                if self.buffer != SIGNATURE {
                    return Err("Unsupported or invalid streamed project signature".into());
                }
                self.signature = true;
                self.checksum.update(&self.buffer);
                self.buffer.clear();
                self.target = PREFIX;
            } else if self.target == PREFIX {
                let kind = u32_at(&self.buffer, 0);
                let id = u64_at(&self.buffer, 4);
                let length = u32_at(&self.buffer, 12) as usize;
                if kind == 255 {
                    if id != 0 || length != FOOTER_BYTES || !self.directory.manifest {
                        return Err("Invalid project end record".into());
                    }
                } else {
                    // Validate before reserving space for an untrusted length.
                    self.directory.accept(kind, id, length)?;
                }
                self.target = PREFIX + length;
                if self.consumed + length as u64 > self.expected {
                    return Err("Truncated project record".into());
                }
                // Reserve exactly the bounded packet rather than geometric growth.
                self.buffer.reserve_exact(length);
            } else {
                let kind = u32_at(&self.buffer, 0);
                let id = u64_at(&self.buffer, 4);
                if crc32fast::hash(&self.buffer[PREFIX..]) != u32_at(&self.buffer, 16) {
                    return Err("Project record checksum failed".into());
                }
                if kind == 255 {
                    let footer = &self.buffer[PREFIX..];
                    if self.consumed != self.expected
                        || u32_at(footer, 0) != self.directory.count
                        || u64_at(footer, 4) != self.expected
                        || u32_at(footer, 12) != self.checksum.clone().finalize()
                        || used != input.len()
                    {
                        return Err("Project stream integrity check failed".into());
                    }
                    self.ended = true;
                    self.buffer.clear();
                    return Ok((used, None));
                }
                self.checksum.update(&self.buffer);
                let mut payload = std::mem::take(&mut self.buffer);
                payload.copy_within(PREFIX.., 0);
                payload.truncate(payload.len() - PREFIX);
                self.target = PREFIX;
                let kind = match kind {
                    1 => ProjectRecordKind::Manifest,
                    2 => ProjectRecordKind::RgbaTile,
                    3 => ProjectRecordKind::GrayTile,
                    _ => unreachable!("validated prefix"),
                };
                let file_offset = self.consumed - payload.len() as u64;
                return Ok((
                    used,
                    Some(ProjectRecord {
                        kind,
                        id,
                        payload,
                        file_offset,
                    }),
                ));
            }
        }
    }
    pub fn finish(self) -> Result<(), String> {
        if self.failed || !self.ended || self.consumed != self.expected {
            return Err("Project stream is incomplete or corrupt".into());
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn example() -> Vec<u8> {
        let mut writer = ProjectStreamWriter::new(Residency::default()).unwrap();
        let mut bytes = writer.signature().to_vec();
        bytes.extend(
            writer
                .record(ProjectRecordKind::Manifest, 0, b"manifest placeholder")
                .unwrap(),
        );
        bytes.extend(
            writer
                .record(ProjectRecordKind::RgbaTile, 1, &vec![19; 262144])
                .unwrap(),
        );
        bytes.extend(
            writer
                .record(ProjectRecordKind::GrayTile, 2, &vec![77; 65536])
                .unwrap(),
        );
        bytes.extend(writer.finish().unwrap());
        bytes
    }
    fn read(bytes: &[u8], chunk: usize) -> Result<Vec<ProjectRecord>, String> {
        let mut reader = ProjectStreamReader::new(bytes.len() as u64, Residency::default())?;
        let mut records = Vec::new();
        for chunk in bytes.chunks(chunk) {
            let mut offset = 0;
            while offset < chunk.len() {
                let (used, record) = reader.feed(&chunk[offset..])?;
                assert!(used > 0);
                offset += used;
                if let Some(record) = record {
                    records.push(record);
                }
                assert!(reader.temporary_bytes() <= (PROJECT_CHUNK_BYTES + PREFIX) as u64);
            }
        }
        reader.finish()?;
        Ok(records)
    }
    #[test]
    fn records_cross_arbitrary_boundaries_and_preserve_payloads() {
        let bytes = example();
        for chunk in [1, 7, 12, 19, 20, 21, 257, 65537, PROJECT_CHUNK_BYTES] {
            let records = read(&bytes, chunk).unwrap();
            assert_eq!(records.len(), 3);
            assert_eq!(records[0].kind, ProjectRecordKind::Manifest);
            assert_eq!(records[1].id, 1);
            assert_eq!(records[1].payload, vec![19; 262144]);
            assert_eq!(records[2].payload, vec![77; 65536]);
        }
        assert_eq!(bytes, example());
    }
    #[test]
    fn corruption_truncation_and_trailing_data_never_finish() {
        let bytes = example();
        for offset in [0, 8, 12, 24, 31, 87, bytes.len() - 1] {
            let mut damaged = bytes.clone();
            damaged[offset] ^= 1;
            assert!(read(&damaged, 173).is_err(), "offset {offset}");
        }
        for length in [0, 12, 33, bytes.len() - 1, bytes.len() - 36] {
            assert!(read(&bytes[..length], 173).is_err());
        }
        let mut trailing = bytes;
        trailing.push(0);
        assert!(read(&trailing, 173).is_err());
    }
    #[test]
    fn invalid_ids_order_lengths_and_limits_are_rejected_before_payload_allocation() {
        let mut writer = ProjectStreamWriter::new(Residency::default()).unwrap();
        assert!(writer
            .record(ProjectRecordKind::GrayTile, 1, &vec![0; 65536])
            .is_err());
        writer.record(ProjectRecordKind::Manifest, 0, b"m").unwrap();
        assert!(writer.record(ProjectRecordKind::Manifest, 0, b"m").is_err());
        assert!(writer
            .record(ProjectRecordKind::RgbaTile, 1, &[0; 4])
            .is_err());
        writer
            .record(ProjectRecordKind::GrayTile, 1, &vec![0; 65536])
            .unwrap();
        assert!(writer
            .record(ProjectRecordKind::GrayTile, 1, &vec![0; 65536])
            .is_err());
        // Counter boundary only: this is not a multi-gigabyte I/O/performance test.
        writer.length = MAX_PROJECT_BYTES - (PREFIX + FOOTER_BYTES) as u64;
        assert!(writer
            .record(ProjectRecordKind::GrayTile, 2, &vec![0; 65536])
            .is_err());
        assert!(ProjectStreamReader::new(MAX_PROJECT_BYTES + 1, Residency::default()).is_err());
        let full = Residency {
            temporary: fotoeditor_core::CPU_BUDGET,
            ..Residency::default()
        };
        assert!(ProjectStreamReader::new(100, full).is_err());
        assert!(ProjectStreamWriter::new(full).is_err());
    }
}
