//! Reproducible adversarial corpus for the legacy and streamed readers.
//! This complements, rather than replaces, coverage-guided fuzzing of P8 readers.
use crate::{
    LayerProjectReader, LayerProjectWriter, ProjectReader, ProjectRecordKind, ProjectStreamWriter,
    ProjectWriter, LAYER_PROJECT_SCRATCH, PROJECT_CHUNK_BYTES,
};
use fotoeditor_core::{Document, Residency, Rgba8, CPU_BUDGET};
use serde_json::{json, Value};

fn streamed(bytes: &[u8], chunk_size: usize) -> Result<Document, String> {
    let mut reader = LayerProjectReader::new(bytes.len() as u64, 1, Residency::default())?;
    for chunk in bytes.chunks(chunk_size) {
        let mut offset = 0;
        while offset < chunk.len() {
            let used = reader.feed(&chunk[offset..])?;
            assert!(used > 0 && used <= chunk.len() - offset);
            assert!(reader.temporary_bytes() <= LAYER_PROJECT_SCRATCH);
            offset += used;
        }
    }
    reader.finish()
}

fn legacy(bytes: &[u8]) -> Result<Document, String> {
    let mut reader = ProjectReader::new(bytes, Residency::default())?;
    while !reader.step()? {}
    reader.finish()
}

fn encode(document: &Document) -> Vec<u8> {
    let mut writer = LayerProjectWriter::new(document, Residency::default()).unwrap();
    let mut bytes = Vec::new();
    while let Some(packet) = writer.next_packet(Residency::default()).unwrap() {
        bytes.extend(packet);
    }
    bytes
}

fn manifest_bytes(manifest: &Value) -> Vec<u8> {
    let mut writer = ProjectStreamWriter::new(Residency::default()).unwrap();
    let mut bytes = writer.signature().to_vec();
    bytes.extend(
        writer
            .record(
                ProjectRecordKind::Manifest,
                0,
                &serde_json::to_vec(manifest).unwrap(),
            )
            .unwrap(),
    );
    bytes.extend(writer.finish().unwrap());
    bytes
}

fn manifest() -> Value {
    let bytes = encode(&Document::blank(257, 259, 299.123456789).unwrap());
    let size = u32::from_le_bytes(bytes[24..28].try_into().unwrap()) as usize;
    serde_json::from_slice(&bytes[32..32 + size]).unwrap()
}

#[test]
fn every_small_file_truncation_and_single_bit_mutation_is_rejected() {
    let doc = Document::blank(257, 259, 299.123456789).unwrap();
    let bytes = encode(&doc);
    assert!(streamed(&bytes, 7).is_ok());
    for length in 0..bytes.len() {
        assert!(streamed(&bytes[..length], 7).is_err(), "length {length}");
    }
    for offset in 0..bytes.len() {
        for bit in 0..8 {
            let mut damaged = bytes.clone();
            damaged[offset] ^= 1 << bit;
            assert!(streamed(&damaged, 13).is_err(), "byte {offset}, bit {bit}");
        }
    }
    let mut writer = ProjectWriter::new(&doc, Residency::default()).unwrap();
    while !writer.step() {}
    let bytes = writer.finish().unwrap();
    assert!(legacy(&bytes).is_ok());
    for length in 0..bytes.len() {
        assert!(legacy(&bytes[..length]).is_err());
    }
    for offset in 0..bytes.len() {
        for bit in 0..8 {
            let mut damaged = bytes.clone();
            damaged[offset] ^= 1 << bit;
            assert!(legacy(&damaged).is_err(), "byte {offset}, bit {bit}");
        }
    }
}

#[test]
fn valid_checksums_do_not_authorize_invalid_document_metadata() {
    let base = manifest();
    assert!(streamed(&manifest_bytes(&base), 17).is_ok());
    let mutations = [
        ("/schema", json!(0)),
        ("/schema", json!(5)),
        ("/width", json!(0)),
        ("/width", json!(16385)),
        ("/height", json!(u32::MAX)),
        ("/dpi", json!(0)),
        ("/dpi", json!(2401)),
        ("/blend_space", json!("unknown")),
        ("/layers/0/id", json!(0)),
        ("/layers/0/id", json!(2)),
        ("/layers/0/properties/opacity", json!(-0.01)),
        ("/layers/0/properties/fill", json!(1.01)),
        ("/layers/0/properties/blend", json!("unknown")),
        ("/layers/0/content/kind", json!("unknown")),
        (
            "/layers/0/content/tiles",
            json!([{"x": 0, "y": 0, "generation": 1}]),
        ),
        (
            "/canvas",
            json!({"x": 256, "y": 0, "width": 2, "height": 1}),
        ),
        (
            "/canvas",
            json!({"x": u32::MAX, "y": 0, "width": 1, "height": 1}),
        ),
    ];
    for (pointer, value) in mutations {
        let mut changed = base.clone();
        *changed.pointer_mut(pointer).unwrap() = value;
        let bytes = manifest_bytes(&changed);
        for chunk in [1, 19, PROJECT_CHUNK_BYTES] {
            assert!(streamed(&bytes, chunk).is_err(), "{pointer}, {chunk}");
        }
    }
    // Deleting every layer is supported; an empty tree is a valid transparent document.
    let mut empty = base.clone();
    empty["layers"] = json!([]);
    assert!(streamed(&manifest_bytes(&empty), 11)
        .unwrap()
        .layers
        .is_empty());
    let mut unknown_field = base;
    unknown_field["unrecognized"] = json!(true);
    assert!(streamed(&manifest_bytes(&unknown_field), 11).is_err());
}

#[test]
fn seeded_tile_mutations_never_publish_a_candidate_and_valid_cold_reads_stay_bounded() {
    let mut doc = Document::blank(257, 259, 72.0).unwrap();
    doc.raster_mut().write(256, 258, Rgba8([79, 31, 203, 0]));
    let bytes = encode(&doc);
    for chunk in [1, 31, 65537, PROJECT_CHUNK_BYTES] {
        let candidate = streamed(&bytes, chunk).unwrap();
        assert_eq!(candidate.bytes(), 0, "candidate tiles must remain cold");
    }
    let mut seed = 0x8c0d_ef17u64;
    for case in 0..1024 {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        let offset = seed as usize % bytes.len();
        let mut damaged = bytes.clone();
        damaged[offset] ^= 1 << (seed >> 32 & 7);
        let chunk = 1 + (seed >> 16) as usize % 4096;
        assert!(
            streamed(&damaged, chunk).is_err(),
            "case {case}, byte {offset}"
        );
    }
    let full = Residency {
        temporary: CPU_BUDGET,
        ..Residency::default()
    };
    assert!(LayerProjectReader::new(bytes.len() as u64, 1, full).is_err());
    assert!(LayerProjectWriter::new(&doc, full).is_err());
    assert_eq!(doc.raster().sample(256, 258), Rgba8([79, 31, 203, 0]));
}
