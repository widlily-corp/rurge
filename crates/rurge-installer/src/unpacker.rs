use std::fs::{self, File};
use std::io::{self, Cursor};
use std::path::Path;
use zip::ZipArchive;

/// Extracts all files within `assets/` from an in-memory `client.jar` buffer directly into `target_assets_dir`.
///
/// Returns the number of files extracted.
pub fn unpack_assets_from_jar_bytes(jar_bytes: &[u8], target_assets_dir: &Path) -> io::Result<usize> {
    let cursor = Cursor::new(jar_bytes);
    let mut archive = ZipArchive::new(cursor)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, format!("Invalid zip archive: {e}")))?;

    let mut count = 0;
    fs::create_dir_all(target_assets_dir)?;

    for i in 0..archive.len() {
        let mut file = archive.by_index(i)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, format!("Failed to read zip index {i}: {e}")))?;

        let name = file.name().to_string();
        // Only extract items inside "assets/"
        if let Some(rel_path) = name.strip_prefix("assets/") {
            if rel_path.is_empty() {
                continue;
            }
            let out_path = target_assets_dir.join(rel_path);
            if file.is_dir() {
                fs::create_dir_all(&out_path)?;
            } else {
                if let Some(parent) = out_path.parent() {
                    fs::create_dir_all(parent)?;
                }
                let mut out_file = File::create(&out_path)?;
                io::copy(&mut file, &mut out_file)?;
                count += 1;
            }
        }
    }

    Ok(count)
}
