//! Virtual sound asset index linker for zero-warning Minecraft audio parity.
//!
//! Reconstructs the virtual sound tree (`sounds.json` and `sounds/**/*.ogg`) from
//! Mojang asset object hashes indexed in `assets/indexes/34.json`.

use rayon::prelude::*;
use serde::Deserialize;
use std::collections::HashMap;
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug)]
pub enum AssetLinkerError {
    IndexNotFound(PathBuf),
    Io(std::io::Error),
    Json(serde_json::Error),
}

impl fmt::Display for AssetLinkerError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::IndexNotFound(p) => write!(f, "Asset index not found in {}", p.display()),
            Self::Io(e) => write!(f, "I/O error during asset linking: {e}"),
            Self::Json(e) => write!(f, "JSON error during asset index parsing: {e}"),
        }
    }
}

impl std::error::Error for AssetLinkerError {}

impl From<std::io::Error> for AssetLinkerError {
    fn from(e: std::io::Error) -> Self {
        Self::Io(e)
    }
}

impl From<serde_json::Error> for AssetLinkerError {
    fn from(e: serde_json::Error) -> Self {
        Self::Json(e)
    }
}

#[derive(Debug, Deserialize)]
struct RawAssetIndex {
    #[serde(default)]
    objects: HashMap<String, serde_json::Value>,
}

/// Checks whether an indexed asset key belongs to the sound tree.
pub fn is_sound_asset(name: &str) -> bool {
    name == "minecraft/sounds.json" || name.starts_with("minecraft/sounds/")
}

/// Attempts to hard link `src` to `dest`. If `dest` already exists, it is unlinked first.
/// If hard linking fails (e.g. cross-device boundaries on Windows), falls back to file copy.
pub fn link_or_copy(src: &Path, dest: &Path) -> std::io::Result<()> {
    if let Some(parent) = dest.parent() {
        fs::create_dir_all(parent)?;
    }

    if dest.exists() {
        let _ = fs::remove_file(dest);
    }

    match fs::hard_link(src, dest) {
        Ok(()) => Ok(()),
        Err(err) => {
            log::debug!(
                "Hard link failed ({err}), falling back to file copy for {}",
                dest.display()
            );
            fs::copy(src, dest).map(|_| ())
        }
    }
}

/// Locates the asset index JSON file (preferring `34.json`, or the newest/highest version JSON in `indexes/`).
fn find_index_file(assets_dir: &Path) -> Result<PathBuf, AssetLinkerError> {
    if assets_dir.is_file() {
        return Ok(assets_dir.to_path_buf());
    }

    let indexes_dir = if assets_dir.file_name().and_then(|s| s.to_str()) == Some("indexes") {
        assets_dir.to_path_buf()
    } else {
        assets_dir.join("indexes")
    };

    if !indexes_dir.exists() {
        return Err(AssetLinkerError::IndexNotFound(indexes_dir));
    }

    let preferred = indexes_dir.join("34.json");
    if preferred.is_file() {
        return Ok(preferred);
    }

    let entries = fs::read_dir(&indexes_dir).map_err(AssetLinkerError::Io)?;
    let mut json_files: Vec<PathBuf> = entries
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().and_then(|s| s.to_str()) == Some("json"))
        .collect();

    json_files.sort_by(|a, b| b.cmp(a));

    json_files
        .into_iter()
        .next()
        .ok_or(AssetLinkerError::IndexNotFound(indexes_dir))
}

/// Resolves the `objects` directory containing hashed assets (`objects/<hash[0..2]>/<hash>`).
fn find_objects_dir(assets_dir: &Path) -> PathBuf {
    if assets_dir.file_name().and_then(|s| s.to_str()) == Some("objects") {
        return assets_dir.to_path_buf();
    }
    let direct = assets_dir.join("objects");
    if direct.is_dir() {
        return direct;
    }
    if let Some(parent) = assets_dir.parent() {
        let parent_objects = parent.join("objects");
        if parent_objects.is_dir() {
            return parent_objects;
        }
    }
    direct
}

/// Links virtual sound assets (`sounds.json` and `.ogg` files) from `assets/objects`
/// into the designated target roots.
///
/// Returns the number of distinct sound asset items successfully linked.
pub fn link_sound_assets(assets_dir: &Path, target_roots: &[&Path]) -> Result<usize, AssetLinkerError> {
    let index_file = find_index_file(assets_dir)?;
    let objects_dir = find_objects_dir(assets_dir);

    let index_bytes = fs::read(&index_file)?;
    let raw_index: RawAssetIndex = serde_json::from_slice(&index_bytes)?;

    let fallback_targets = [assets_dir];
    let effective_targets = if target_roots.is_empty() {
        &fallback_targets[..]
    } else {
        target_roots
    };

    // Filter sound entries and verify object files exist
    let mut sound_items: Vec<(String, PathBuf)> = Vec::new();

    for (name, value) in raw_index.objects {
        if !is_sound_asset(&name) {
            continue;
        }

        let hash = match value.get("hash").and_then(|h| h.as_str()) {
            Some(h) if h.len() >= 2 => h,
            _ => {
                log::warn!("Invalid or missing hash in asset index for '{name}'");
                continue;
            }
        };

        let obj_path = objects_dir.join(&hash[..2]).join(hash);
        if !obj_path.is_file() {
            log::warn!(
                "Object file missing for sound asset '{name}' (hash {hash}) at {}",
                obj_path.display()
            );
            continue;
        }

        sound_items.push((name, obj_path));
    }

    log::info!(
        "Found {} sound assets to link into {} target root(s)",
        sound_items.len(),
        effective_targets.len()
    );

    // Parallel linking across sound objects using Rayon
    let linked_count: usize = sound_items
        .par_iter()
        .filter_map(|(virtual_path, obj_path)| {
            let mut linked_any = false;
            for target_root in effective_targets {
                let target_base = if target_root.file_name().and_then(|s| s.to_str()) == Some("assets") {
                    target_root.to_path_buf()
                } else {
                    target_root.join("assets")
                };

                let dest = target_base.join(virtual_path);
                match link_or_copy(obj_path, &dest) {
                    Ok(()) => {
                        linked_any = true;
                    }
                    Err(err) => {
                        log::warn!(
                            "Failed to link {} to {}: {}",
                            obj_path.display(),
                            dest.display(),
                            err
                        );
                    }
                }
            }

            if linked_any {
                Some(1)
            } else {
                None
            }
        })
        .sum();

    Ok(linked_count)
}
