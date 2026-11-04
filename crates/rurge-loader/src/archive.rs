use std::io::Read;
use std::path::{Path, PathBuf};
use rurge_core::ModManifest;

/// Errors that can occur when inspecting or extracting a `.rmod` archive.
#[derive(Debug, thiserror::Error)]
pub enum ArchiveError {
    /// File I/O failure.
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    /// ZIP decompression or archive reading failure.
    #[error("ZIP error: {0}")]
    Zip(#[from] zip::result::ZipError),

    /// Missing `rurge.toml` manifest in the root of the `.rmod` archive.
    #[error("Manifest file 'rurge.toml' missing in archive")]
    ManifestMissing,

    /// Manifest parsing or validation failure.
    #[error("Manifest is invalid: {0}")]
    ManifestInvalid(#[from] rurge_core::ManifestError),

    /// Dynamic library matching the host target triple was not found in the archive.
    #[error("Binary for target '{target}' not found in archive")]
    BinaryNotFound { target: String },
}

/// Represents an on-disk `.rmod` zip archive.
#[derive(Debug, Clone)]
pub struct RmodArchive {
    path: PathBuf,
}

impl RmodArchive {
    /// Opens and validates a `.rmod` zip archive at the specified path.
    pub fn open(path: impl AsRef<Path>) -> Result<Self, ArchiveError> {
        let path = path.as_ref().to_path_buf();
        let file = std::fs::File::open(&path)?;
        // Validate that this is indeed a valid ZIP file.
        zip::ZipArchive::new(file)?;
        Ok(Self { path })
    }

    /// Returns the path to the `.rmod` file on disk.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Reads and parses the `rurge.toml` manifest contained inside the archive.
    pub fn read_manifest(&self) -> Result<ModManifest, ArchiveError> {
        let file = std::fs::File::open(&self.path)?;
        let mut archive = zip::ZipArchive::new(file)?;

        match archive.by_name("rurge.toml") {
            Ok(mut entry) => {
                let mut content = String::new();
                entry.read_to_string(&mut content)?;
                let manifest = ModManifest::from_toml_str(&content)?;
                Ok(manifest)
            }
            Err(zip::result::ZipError::FileNotFound) => Err(ArchiveError::ManifestMissing),
            Err(err) => Err(ArchiveError::Zip(err)),
        }
    }

    /// Checks whether the archive contains declarative `content.json`.
    pub fn has_content_schema(&self) -> bool {
        let Ok(file) = std::fs::File::open(&self.path) else {
            return false;
        };
        let Ok(mut archive) = zip::ZipArchive::new(file) else {
            return false;
        };
        archive.by_name("content.json").is_ok()
    }

    /// Reads `content.json` from the archive if present.
    pub fn read_content_schema(&self) -> Result<Option<String>, ArchiveError> {
        let file = std::fs::File::open(&self.path)?;
        let mut archive = zip::ZipArchive::new(file)?;
        match archive.by_name("content.json") {
            Ok(mut entry) => {
                let mut content = String::new();
                entry.read_to_string(&mut content)?;
                Ok(Some(content))
            }
            Err(zip::result::ZipError::FileNotFound) => Ok(None),
            Err(err) => Err(ArchiveError::Zip(err)),
        }
    }

    /// Checks whether the archive contains a dynamic library for `target_triple` (or fallback).
    pub fn has_binary(&self, target_triple: &str) -> bool {
        let Ok(file) = std::fs::File::open(&self.path) else {
            return false;
        };
        let Ok(mut archive) = zip::ZipArchive::new(file) else {
            return false;
        };
        let prefix_target = format!("binaries/{target_triple}/");
        let is_dylib = |name: &str| {
            let l = name.to_lowercase();
            l.ends_with(".dll") || l.ends_with(".so") || l.ends_with(".dylib")
        };
        for i in 0..archive.len() {
            if let Ok(entry) = archive.by_index(i) {
                if entry.is_dir() {
                    continue;
                }
                let name = entry.name().to_string();
                if name.starts_with(&prefix_target) {
                    return true;
                }
                if let Some(rest) = name.strip_prefix("binaries/")
                    && !rest.is_empty()
                    && !rest.contains('/')
                    && is_dylib(&name)
                {
                    return true;
                }
            }
        }
        false
    }

    /// Extracts the host dynamic library matching `target_triple` to `out_dir`.
    ///
    /// Searches in order:
    /// 1. `binaries/<target_triple>/<filename>`
    /// 2. `binaries/<filename>` (fallback)
    pub fn extract_binary(&self, target_triple: &str, out_dir: &Path) -> Result<PathBuf, ArchiveError> {
        let file = std::fs::File::open(&self.path)?;
        let mut archive = zip::ZipArchive::new(file)?;

        let prefix_target = format!("binaries/{target_triple}/");
        let mut target_candidate = None;
        let mut fallback_candidate = None;

        let is_dylib = |name: &str| {
            let l = name.to_lowercase();
            l.ends_with(".dll") || l.ends_with(".so") || l.ends_with(".dylib")
        };

        for i in 0..archive.len() {
            let entry = archive.by_index(i)?;
            if entry.is_dir() {
                continue;
            }
            let name = entry.name().to_string();
            if name.starts_with(&prefix_target) {
                match &target_candidate {
                    None => target_candidate = Some(name),
                    Some(existing) if !is_dylib(existing) && is_dylib(&name) => {
                        target_candidate = Some(name);
                    }
                    _ => {}
                }
            } else if let Some(rest) = name.strip_prefix("binaries/")
                && !rest.is_empty()
                && !rest.contains('/')
            {
                match &fallback_candidate {
                    None => fallback_candidate = Some(name),
                    Some(existing) if !is_dylib(existing) && is_dylib(&name) => {
                        fallback_candidate = Some(name);
                    }
                    _ => {}
                }
            }
        }

        let chosen_name = target_candidate
            .or(fallback_candidate)
            .ok_or_else(|| ArchiveError::BinaryNotFound {
                target: target_triple.to_string(),
            })?;

        let file_name = Path::new(&chosen_name)
            .file_name()
            .ok_or_else(|| ArchiveError::BinaryNotFound {
                target: target_triple.to_string(),
            })?;

        std::fs::create_dir_all(out_dir)?;
        let dest_path = out_dir.join(file_name);

        let mut entry = archive.by_name(&chosen_name)?;
        let mut out_file = std::fs::File::create(&dest_path)?;
        std::io::copy(&mut entry, &mut out_file)?;

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            if let Ok(meta) = std::fs::metadata(&dest_path) {
                let mut perms = meta.permissions();
                perms.set_mode(0o755);
                let _ = std::fs::set_permissions(&dest_path, perms);
            }
        }

        Ok(dest_path)
    }

    /// Extracts the `assets/` subfolder from the archive into `out_dir`.
    pub fn extract_assets(&self, out_dir: &Path) -> Result<PathBuf, ArchiveError> {
        let file = std::fs::File::open(&self.path)?;
        let mut archive = zip::ZipArchive::new(file)?;

        let assets_dir = if out_dir.ends_with("assets") {
            out_dir.to_path_buf()
        } else {
            out_dir.join("assets")
        };
        std::fs::create_dir_all(&assets_dir)?;

        for i in 0..archive.len() {
            let mut entry = archive.by_index(i)?;
            let name = entry.name().to_string();
            if let Some(rel) = name.strip_prefix("assets/") {
                if rel.is_empty() {
                    continue;
                }
                let target_path = assets_dir.join(rel);

                // Zip slip mitigation: skip paths with parent references
                if target_path.components().any(|c| c == std::path::Component::ParentDir) {
                    continue;
                }

                if entry.is_dir() {
                    std::fs::create_dir_all(&target_path)?;
                } else {
                    if let Some(parent) = target_path.parent() {
                        std::fs::create_dir_all(parent)?;
                    }
                    let mut out_file = std::fs::File::create(&target_path)?;
                    std::io::copy(&mut entry, &mut out_file)?;
                }
            }
        }

        Ok(assets_dir)
    }
}
