//! Classic OptiFine/Iris format test shaderpack bundle and installer.

use std::io::Write;
use std::path::{Path, PathBuf};

/// The file name of the default classic test shaderpack.
pub const SHADERPACK_NAME: &str = "test-classic-shader.zip";

const SHADERS_PROPERTIES: &str =
    include_str!("../../../assets/shaderpacks/test-classic-shader/shaders/shaders.properties");
const LANG_EN_US: &str =
    include_str!("../../../assets/shaderpacks/test-classic-shader/shaders/lang/en_US.lang");
const GBUFFERS_TERRAIN_VSH: &str =
    include_str!("../../../assets/shaderpacks/test-classic-shader/shaders/gbuffers_terrain.vsh");
const GBUFFERS_TERRAIN_FSH: &str =
    include_str!("../../../assets/shaderpacks/test-classic-shader/shaders/gbuffers_terrain.fsh");
const COMPOSITE_VSH: &str =
    include_str!("../../../assets/shaderpacks/test-classic-shader/shaders/composite.vsh");
const COMPOSITE_FSH: &str =
    include_str!("../../../assets/shaderpacks/test-classic-shader/shaders/composite.fsh");
const FINAL_VSH: &str =
    include_str!("../../../assets/shaderpacks/test-classic-shader/shaders/final.vsh");
const FINAL_FSH: &str =
    include_str!("../../../assets/shaderpacks/test-classic-shader/shaders/final.fsh");

/// Builds in-memory ZIP bytes containing the standard classic shaderpack files under `shaders/`.
pub fn build_shaderpack_zip_bytes() -> Result<Vec<u8>, String> {
    let mut writer = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    let options = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);

    let entries: [(&str, &str); 8] = [
        ("shaders/shaders.properties", SHADERS_PROPERTIES),
        ("shaders/lang/en_US.lang", LANG_EN_US),
        ("shaders/gbuffers_terrain.vsh", GBUFFERS_TERRAIN_VSH),
        ("shaders/gbuffers_terrain.fsh", GBUFFERS_TERRAIN_FSH),
        ("shaders/composite.vsh", COMPOSITE_VSH),
        ("shaders/composite.fsh", COMPOSITE_FSH),
        ("shaders/final.vsh", FINAL_VSH),
        ("shaders/final.fsh", FINAL_FSH),
    ];

    for (name, content) in entries {
        writer
            .start_file(name, options)
            .map_err(|e| format!("failed to start zip entry '{name}': {e}"))?;
        writer
            .write_all(content.as_bytes())
            .map_err(|e| format!("failed to write zip entry '{name}': {e}"))?;
    }

    let cursor = writer
        .finish()
        .map_err(|e| format!("failed to finalize zip archive: {e}"))?;
    Ok(cursor.into_inner())
}

/// Checks whether an existing ZIP file at `path` contains the required options/language file.
fn is_shaderpack_up_to_date(path: &Path) -> bool {
    let file = match std::fs::File::open(path) {
        Ok(f) => f,
        Err(_) => return false,
    };
    let mut archive = match zip::ZipArchive::new(file) {
        Ok(a) => a,
        Err(_) => return false,
    };
    archive.by_name("shaders/lang/en_US.lang").is_ok()
}

/// Ensures the default test shaderpack is installed in `target_dir` (e.g. `shaderpacks/`).
///
/// If `target_dir/test-classic-shader.zip` already exists and is up to date, returns its path.
/// Otherwise, builds the updated ZIP and writes it to disk.
pub fn ensure_default_shaderpack_installed(target_dir: &Path) -> Result<PathBuf, String> {
    if !target_dir.exists() {
        std::fs::create_dir_all(target_dir)
            .map_err(|e| format!("failed to create directory '{}': {e}", target_dir.display()))?;
    }

    let pack_path = target_dir.join(SHADERPACK_NAME);
    let needs_install = !pack_path.exists() || !is_shaderpack_up_to_date(&pack_path);

    if needs_install {
        let zip_bytes = build_shaderpack_zip_bytes()?;
        std::fs::write(&pack_path, zip_bytes)
            .map_err(|e| format!("failed to write shaderpack to '{}': {e}", pack_path.display()))?;
    }

    Ok(pack_path)
}
