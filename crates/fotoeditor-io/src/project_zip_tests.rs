use crate::*;
use fotoeditor_core::{
    CanvasRect, Document, Gray8, Guide, GuideAxis, LayerMask, LayerRenderJob, Residency, Rgba8,
    TileSurface, CPU_BUDGET,
};
use std::sync::Arc;

fn document() -> Document {
    let mut doc = Document::blank(513, 259, 299.123456789).unwrap();
    doc.raster_mut().write(0, 0, Rgba8([79, 31, 203, 0]));
    doc.raster_mut().write(256, 1, Rgba8([201, 7, 79, 128]));
    doc.raster_mut().write(512, 258, Rgba8([31, 203, 9, 255]));
    let mut mask = TileSurface::with_default(513, 259, Gray8(255));
    mask.write(256, 1, Gray8(127));
    doc.layers[0].mask = Some(LayerMask {
        enabled: true,
        samples: Arc::new(mask),
    });
    doc.canvas = Some(CanvasRect {
        x: 255,
        y: 1,
        width: 257,
        height: 257,
    });
    doc.guides = vec![Guide {
        axis: GuideAxis::Vertical,
        position: 256.123456789,
    }];
    doc
}
fn legacy(doc: &Document) -> Vec<u8> {
    let mut writer = LayerProjectWriter::new(doc, Residency::default()).unwrap();
    let mut bytes = Vec::new();
    while let Some(packet) = writer.next_packet(Residency::default()).unwrap() {
        bytes.extend(packet);
    }
    bytes
}
fn zip(doc: &Document) -> Vec<u8> {
    let mut render = LayerRenderJob::new(doc.clone()).unwrap();
    while !render.step(Residency::default()).unwrap() {}
    let mut writer = ZipProjectWriter::new(doc, &render.output, Residency::default()).unwrap();
    let mut bytes = Vec::new();
    while let Some(packet) = writer.next_packet(Residency::default()).unwrap() {
        assert!(packet.len() <= PROJECT_CHUNK_BYTES);
        bytes.extend(packet);
    }
    assert_eq!(writer.written_bytes(), bytes.len() as u64);
    bytes
}
fn unpack(bytes: &[u8]) -> Result<Vec<u8>, String> {
    let mut reader = ZipProjectReader::new(bytes.len() as u64, Residency::default())?;
    let mut output = Vec::new();
    loop {
        if let Some(packet) = reader.take_output() {
            assert!(packet.len() <= PROJECT_CHUNK_BYTES + 20);
            output.extend(packet);
        }
        let Some(request) = reader.request()? else {
            break;
        };
        assert!(request.length <= PROJECT_CHUNK_BYTES);
        let data = bytes
            .get(request.offset as usize..)
            .and_then(|b| b.get(..request.length))
            .ok_or("Truncated test input")?;
        reader.feed(data, Residency::default())?;
    }
    assert_eq!(reader.finish()?, output.len() as u64);
    Ok(output)
}
fn entries(bytes: &[u8]) -> Vec<ZipEntry> {
    let range = ZipDirectoryRange::inspect(&bytes[bytes.len() - 22..], bytes.len() as u64).unwrap();
    range
        .parse(
            &bytes[range.offset as usize..][..range.length as usize],
            Residency::default(),
        )
        .unwrap()
}
fn repack(bytes: &[u8], change: impl Fn(&str, &mut Vec<u8>)) -> Vec<u8> {
    let mut writer = ZipStreamWriter::new(Residency::default()).unwrap();
    let mut out = Vec::new();
    for e in entries(bytes) {
        let mut data = bytes[e.payload_offset() as usize..][..e.length as usize].to_vec();
        change(&e.name, &mut data);
        out.extend(writer.record(&e.name, &data).unwrap());
    }
    out.extend(writer.finish().unwrap());
    out
}

#[test]
fn zip_lz4_restores_identical_document_bytes_including_hidden_crop_mask_and_guides() {
    let source = document();
    let bytes = zip(&source);
    let raw = unpack(&bytes).unwrap();
    assert_eq!(raw, legacy(&source));
    let mut reader = LayerProjectReader::new(raw.len() as u64, 1, Residency::default()).unwrap();
    for chunk in raw.chunks(1019) {
        let mut at = 0;
        while at < chunk.len() {
            at += reader.feed(&chunk[at..]).unwrap();
        }
    }
    let decoded = reader.finish().unwrap();
    assert_eq!(decoded.bytes(), 0);
    let mut tiles = Vec::new();
    decoded.visit_tiles(&mut |t| tiles.push(StoredProjectTile::from(t)));
    for tile in tiles {
        let b = tile.backing().unwrap();
        tile.hydrate(
            &raw[b.offset as usize..][..b.length as usize],
            Residency::default(),
        )
        .unwrap();
    }
    assert_eq!(decoded.raster().sample(0, 0), Rgba8([79, 31, 203, 0]));
    assert_eq!(decoded.canvas, source.canvas);
    assert_eq!(decoded.guides, source.guides);
    assert_eq!(legacy(&decoded), raw);
    let names: Vec<_> = entries(&bytes).into_iter().map(|e| e.name).collect();
    assert_eq!(names[0], "manifest.json");
    assert_eq!(
        &names[names.len() - 2..],
        &["thumbnail.png", "composite.png"]
    );
    for e in entries(&bytes)
        .into_iter()
        .filter(|e| e.name.ends_with(".png"))
    {
        let png = &bytes[e.payload_offset() as usize..][..e.length as usize];
        let image = image::load_from_memory(png).unwrap();
        if e.name == "thumbnail.png" {
            assert_eq!((image.width(), image.height()), (64, 64));
        } else {
            assert_eq!((image.width(), image.height()), (257, 257));
        }
    }
}

#[test]
fn corrupted_previews_or_tiles_cannot_authorize_a_complete_project_stream() {
    let bytes = zip(&document());
    let preview = repack(&bytes, |name, data| {
        if name == "composite.png" {
            data[20] ^= 1;
        }
    });
    assert!(unpack(&preview).is_err());
    let tile = repack(&bytes, |name, data| {
        if name.ends_with(".lz4") {
            data[..4].copy_from_slice(&u32::MAX.to_le_bytes());
        }
    });
    assert!(unpack(&tile).is_err());
    for length in [0, 21, bytes.len() - 1, bytes.len() - 22] {
        assert!(unpack(&bytes[..length]).is_err());
    }
    let mut reader = ZipProjectReader::new(bytes.len() as u64, Residency::default()).unwrap();
    assert!(reader.feed(&[0; 21], Residency::default()).is_err());
    assert!(reader.request().is_err());
    assert!(reader.finish().is_err());
    let full = Residency {
        document: CPU_BUDGET,
        ..Residency::default()
    };
    assert!(ZipProjectReader::new(bytes.len() as u64, full).is_err());
    let doc = document();
    assert!(ZipProjectWriter::new(&doc, &doc, full).is_err());
}
