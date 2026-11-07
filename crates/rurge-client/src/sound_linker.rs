//! Sound asset virtual index linker helper for rurge-client.
//!
//! Prepares and links the sound asset hierarchy (`minecraft/sounds.json` and `.ogg` files)
//! before the audio mixer initializes to guarantee zero missing-sound warnings.

pub use rurge_installer::asset_linker::{link_sound_assets, AssetLinkerError};
use std::path::Path;

/// Ensures virtual sound assets are linked from `assets/objects` into both the root
/// `assets/minecraft/sounds/` directory and `.horizon-cache/mod-content/assets/`
/// before the sound mixer starts.
///
/// Returns the number of linked sound objects on success, or logs a warning and returns
/// `Ok(0)` if the index is missing or corrupted (Review Focus 1: non-fatal startup).
pub fn ensure_sound_assets_linked(base_dir: &Path) -> Result<usize, AssetLinkerError> {
    let assets_dir = base_dir.join("assets");
    if !assets_dir.exists() {
        log::warn!(
            "Assets directory not found at {}; skipping sound asset linking",
            assets_dir.display()
        );
        return Ok(0);
    }

    let horizon_mod_content = base_dir.join(".horizon-cache").join("mod-content");
    match link_sound_assets(&assets_dir, &[base_dir, &horizon_mod_content]) {
        Ok(count) => {
            log::info!("Sound asset virtual index linked successfully ({count} objects)");
            Ok(count)
        }
        Err(e) => {
            log::warn!("Non-fatal sound asset linking notice: {e}; proceeding with engine boot");
            Ok(0)
        }
    }
}
