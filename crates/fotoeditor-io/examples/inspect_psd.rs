//! Inspect candidate fixture framing/provenance. This does not decode or validate
//! Photoshop rendering. Keep corpus origin, license and expected output separately.
use fotoeditor_io::{PsdPreflight, MAX_FILE_BYTES};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    for file in std::env::args().skip(1) {
        if std::fs::metadata(&file)?.len() > MAX_FILE_BYTES {
            return Err("PSD fixture exceeds 128 MiB".into());
        }
        let bytes = std::fs::read(&file)?;
        match PsdPreflight::inspect(&bytes) {
            Ok(info) => println!(
                "{}",
                serde_json::json!({
                    "file": file, "bytes": bytes.len(), "framing": "accepted",
                    "width": info.header.width, "height": info.header.height,
                    "channels": info.header.channels, "depth": info.header.depth,
                    "colorMode": info.header.color_mode, "layers": info.layer_count,
                    "mergedTransparency": info.merged_transparency,
                    "iccBytes": info.icc_profile.map_or(0, |p| p.len()),
                    "versionInfoBytes": info.version_info.map_or(0, |p| p.len()),
                    "decoded": false, "photoshopValidated": false,
                })
            ),
            Err(error) => println!(
                "{}",
                serde_json::json!({"file": file, "bytes": bytes.len(), "framing": "rejected", "error": error, "decoded": false, "photoshopValidated": false})
            ),
        }
    }
    Ok(())
}
