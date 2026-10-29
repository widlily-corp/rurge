use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use clap::Args;
use rurge_core::manifest::ModManifest;
use rurge_loader::{current_target_triple, RmodArchive};
use walkdir::WalkDir;
use zip::write::SimpleFileOptions;
use zip::ZipWriter;

use crate::CargoRurgeError;

/// Arguments for `cargo rurge pack`.
#[derive(Args, Debug, Clone, Default)]
pub struct PackArgs {
    /// Path to project root or rurge.toml manifest (defaults to current directory)
    #[arg(short, long)]
    pub path: Option<PathBuf>,

    /// Destination directory for the generated .rmod archive
    #[arg(short, long)]
    pub out: Option<PathBuf>,

    /// Look for binary in target/release instead of target/debug
    #[arg(short, long)]
    pub release: bool,

    /// Target triple for binary placement (defaults to host triple)
    #[arg(short, long)]
    pub target: Option<String>,

    /// Explicit path to the compiled cdylib binary
    #[arg(short, long)]
    pub binary: Option<PathBuf>,
}

/// Discovers the path to the compiled cdylib shared library.
fn locate_binary(
    project_dir: &Path,
    crate_name: &str,
    target_triple: &str,
    release: bool,
) -> Option<PathBuf> {
    let profile = if release { "release" } else { "debug" };
    let fallback_profile = if release { "debug" } else { "release" };

    let candidate_dirs = [
        project_dir.join("target").join(target_triple).join(profile),
        project_dir.join("target").join(profile),
        // Workspace root fallbacks
        project_dir.join("..").join("target").join(target_triple).join(profile),
        project_dir.join("..").join("target").join(profile),
        project_dir.join("../..").join("target").join(target_triple).join(profile),
        project_dir.join("../..").join("target").join(profile),
        // Fallback profile dirs
        project_dir.join("target").join(target_triple).join(fallback_profile),
        project_dir.join("target").join(fallback_profile),
    ];

    let is_dylib = |name: &str| {
        let l = name.to_lowercase();
        l.ends_with(".dll") || l.ends_with(".so") || l.ends_with(".dylib")
    };

    let normalized_name = crate_name.replace('-', "_");

    for dir in &candidate_dirs {
        if !dir.is_dir() {
            continue;
        }

        if let Ok(entries) = fs::read_dir(dir) {
            let mut dylibs = Vec::new();
            for entry in entries.filter_map(|e| e.ok()) {
                let path = entry.path();
                if path.is_file() {
                    let file_name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
                    if is_dylib(file_name) {
                        if file_name.contains(&normalized_name) {
                            return Some(path);
                        }
                        dylibs.push(path);
                    }
                }
            }
            // If only one dynamic library was built in this directory, use it
            if dylibs.len() == 1 {
                return Some(dylibs.remove(0));
            }
        }
    }

    None
}

/// Packs a Rurge mod project into a `.rmod` zip package.
pub fn execute(args: PackArgs) -> Result<PathBuf, CargoRurgeError> {
    let project_dir = match args.path {
        Some(ref p) if p.is_file() => p.parent().unwrap_or(Path::new(".")).to_path_buf(),
        Some(p) => p,
        None => std::env::current_dir()?,
    };

    let manifest_path = project_dir.join("rurge.toml");
    if !manifest_path.is_file() {
        return Err(CargoRurgeError::ManifestNotFound(manifest_path));
    }

    let manifest = ModManifest::from_file(&manifest_path)?;

    let default_target = current_target_triple();
    let target_triple: &str = args.target.as_deref().unwrap_or(default_target);

    let binary_path = match args.binary {
        Some(b) => {
            if !b.is_file() {
                return Err(CargoRurgeError::BinaryNotFound {
                    target: target_triple.to_string(),
                    searched: vec![b],
                });
            }
            b
        }
        None => locate_binary(&project_dir, &manifest.id, target_triple, args.release)
            .ok_or_else(|| CargoRurgeError::BinaryNotFound {
                target: target_triple.to_string(),
                searched: vec![project_dir.join("target")],
            })?,
    };

    let binary_file_name = binary_path
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or_else(|| CargoRurgeError::Custom("Invalid binary file name".to_string()))?;

    // Determine output directory
    let out_dir = match args.out {
        Some(out) => out,
        None => {
            let target_dir = project_dir.join("target");
            if target_dir.exists() {
                target_dir
            } else {
                project_dir.clone()
            }
        }
    };
    fs::create_dir_all(&out_dir)?;

    let rmod_file_name = format!("{}-{}.rmod", manifest.id, manifest.version);
    let rmod_path = out_dir.join(rmod_file_name);

    let zip_file = File::create(&rmod_path)?;
    let mut zip = ZipWriter::new(zip_file);
    let options = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);

    // 1. Add rurge.toml to root of zip
    zip.start_file("rurge.toml", options)?;
    let mut manifest_bytes = Vec::new();
    File::open(&manifest_path)?.read_to_end(&mut manifest_bytes)?;
    zip.write_all(&manifest_bytes)?;

    // 2. Add binary to binaries/<target_triple>/<filename>
    let zip_binary_path = format!("binaries/{}/{}", target_triple, binary_file_name);
    zip.start_file(&zip_binary_path, options)?;
    let mut bin_bytes = Vec::new();
    File::open(&binary_path)?.read_to_end(&mut bin_bytes)?;
    zip.write_all(&bin_bytes)?;

    // 3. Add assets/ folder recursively if it exists
    let assets_dir = project_dir.join("assets");
    if assets_dir.is_dir() {
        for entry in WalkDir::new(&assets_dir).into_iter().filter_map(|e| e.ok()) {
            let path = entry.path();
            if path.is_file() {
                let relative = path.strip_prefix(&project_dir).map_err(|e| {
                    CargoRurgeError::Custom(format!("Failed to determine relative path: {e}"))
                })?;
                let zip_entry_path = relative.to_string_lossy().replace('\\', "/");
                zip.start_file(&zip_entry_path, options)?;
                let mut data = Vec::new();
                File::open(path)?.read_to_end(&mut data)?;
                zip.write_all(&data)?;
            }
        }
    }

    // 4. Add data/ folder recursively if it exists
    let data_dir = project_dir.join("data");
    if data_dir.is_dir() {
        for entry in WalkDir::new(&data_dir).into_iter().filter_map(|e| e.ok()) {
            let path = entry.path();
            if path.is_file() {
                let relative = path.strip_prefix(&project_dir).map_err(|e| {
                    CargoRurgeError::Custom(format!("Failed to determine relative path: {e}"))
                })?;
                let zip_entry_path = relative.to_string_lossy().replace('\\', "/");
                zip.start_file(&zip_entry_path, options)?;
                let mut data = Vec::new();
                File::open(path)?.read_to_end(&mut data)?;
                zip.write_all(&data)?;
            }
        }
    }

    zip.finish()?;

    // 5. Validate the archive using rurge-loader
    let archive = RmodArchive::open(&rmod_path)?;
    let parsed_manifest = archive.read_manifest()?;
    if parsed_manifest.id != manifest.id {
        return Err(CargoRurgeError::Custom(format!(
            "Packed archive validation failed: manifest ID mismatch ({} != {})",
            parsed_manifest.id, manifest.id
        )));
    }

    println!("Successfully packed mod package: {}", rmod_path.display());
    Ok(rmod_path)
}
