use std::fs::File;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tempfile::tempdir;
use zip::write::SimpleFileOptions;
use zip::ZipWriter;

use rurge_blamer::Blamer;
use rurge_core::ResolverError;
use rurge_loader::{
    current_target_triple, load_plugin_from_dylib, ArchiveError, DylibError, LoaderError,
    ModManager, RmodArchive,
};

fn create_test_rmod(
    dir: &Path,
    filename: &str,
    manifest_toml: Option<&str>,
    binaries: &[(&str, &[u8])],
    assets: &[(&str, &[u8])],
) -> PathBuf {
    let rmod_path = dir.join(filename);
    let file = File::create(&rmod_path).expect("Failed to create test .rmod file");
    let mut zip = ZipWriter::new(file);
    let options = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);

    if let Some(toml_content) = manifest_toml {
        zip.start_file("rurge.toml", options)
            .expect("Failed to start rurge.toml");
        zip.write_all(toml_content.as_bytes())
            .expect("Failed to write rurge.toml");
    }

    for (rel_path, content) in binaries {
        zip.start_file(*rel_path, options)
            .expect("Failed to start binary entry");
        zip.write_all(content).expect("Failed to write binary content");
    }

    for (rel_path, content) in assets {
        zip.start_file(*rel_path, options)
            .expect("Failed to start asset entry");
        zip.write_all(content).expect("Failed to write asset content");
    }

    zip.finish().expect("Failed to finish zip archive");
    rmod_path
}

#[test]
fn test_archive_open_and_read_manifest() {
    let tmp = tempdir().unwrap();
    let manifest = r#"
id = "example_mod"
name = "Example Mod"
version = "1.0.0"
authors = ["Tester"]
"#;

    let path = create_test_rmod(tmp.path(), "example_mod.rmod", Some(manifest), &[], &[]);
    let archive = RmodArchive::open(&path).expect("Failed to open archive");

    assert_eq!(archive.path(), path.as_path());

    let parsed = archive.read_manifest().expect("Failed to read manifest");
    assert_eq!(parsed.id, "example_mod");
    assert_eq!(parsed.name, "Example Mod");
    assert_eq!(parsed.version.to_string(), "1.0.0");
    assert_eq!(parsed.authors, vec!["Tester".to_string()]);
}

#[test]
fn test_archive_extract_binary_and_assets() {
    let tmp = tempdir().unwrap();
    let manifest = r#"
id = "extractor_mod"
name = "Extractor Mod"
version = "0.5.0"
"#;
    let target = current_target_triple();
    let bin_path_in_zip = format!("binaries/{target}/plugin.dll");
    let bin_data = b"MOCK_COMPILED_PLUGIN_BYTES";
    let asset_data = b"{\"key\": \"value\"}";

    let rmod_path = create_test_rmod(
        tmp.path(),
        "extractor.rmod",
        Some(manifest),
        &[(&bin_path_in_zip, bin_data)],
        &[("assets/config.json", asset_data)],
    );

    let archive = RmodArchive::open(&rmod_path).unwrap();

    let out_dir = tmp.path().join("extracted");
    let extracted_bin = archive.extract_binary(target, &out_dir).unwrap();
    assert!(extracted_bin.exists());
    let read_bytes = std::fs::read(&extracted_bin).unwrap();
    assert_eq!(read_bytes, bin_data);

    let extracted_assets = archive.extract_assets(&out_dir).unwrap();
    assert!(extracted_assets.exists());
    let asset_file = extracted_assets.join("config.json");
    assert!(asset_file.exists());
    let read_asset_bytes = std::fs::read(&asset_file).unwrap();
    assert_eq!(read_asset_bytes, asset_data);
}

#[test]
fn test_archive_extract_binary_fallback() {
    let tmp = tempdir().unwrap();
    let manifest = r#"
id = "fallback_mod"
name = "Fallback Mod"
version = "0.1.0"
"#;
    let bin_data = b"FALLBACK_BIN";
    let rmod_path = create_test_rmod(
        tmp.path(),
        "fallback.rmod",
        Some(manifest),
        &[("binaries/fallback_plugin.dll", bin_data)],
        &[],
    );

    let archive = RmodArchive::open(&rmod_path).unwrap();
    let out_dir = tmp.path().join("fallback_out");
    let extracted_bin = archive.extract_binary("x86_64-unknown-linux-gnu", &out_dir).unwrap();
    assert!(extracted_bin.exists());
    assert_eq!(std::fs::read(&extracted_bin).unwrap(), bin_data);
}

#[test]
fn test_archive_binary_not_found() {
    let tmp = tempdir().unwrap();
    let manifest = r#"
id = "empty_mod"
name = "Empty Mod"
version = "1.0.0"
"#;
    let rmod_path = create_test_rmod(tmp.path(), "empty.rmod", Some(manifest), &[], &[]);
    let archive = RmodArchive::open(&rmod_path).unwrap();
    let out_dir = tmp.path().join("empty_out");

    let err = archive.extract_binary("unknown-target", &out_dir).unwrap_err();
    match err {
        ArchiveError::BinaryNotFound { target } => assert_eq!(target, "unknown-target"),
        other => panic!("Expected BinaryNotFound, got {other:?}"),
    }
}

#[test]
fn test_corrupted_and_missing_archives() {
    let tmp = tempdir().unwrap();

    // 1. Non-existent file
    let non_existent = tmp.path().join("does_not_exist.rmod");
    let err = RmodArchive::open(&non_existent).unwrap_err();
    assert!(matches!(err, ArchiveError::Io(_)));

    // 2. Corrupted file (garbage)
    let corrupted_path = tmp.path().join("corrupted.rmod");
    std::fs::write(&corrupted_path, b"not a zip file definitely").unwrap();
    let err = RmodArchive::open(&corrupted_path).unwrap_err();
    assert!(matches!(err, ArchiveError::Zip(_)));

    // 3. Archive missing rurge.toml
    let missing_manifest_path = create_test_rmod(tmp.path(), "no_manifest.rmod", None, &[], &[]);
    let archive = RmodArchive::open(&missing_manifest_path).unwrap();
    let err = archive.read_manifest().unwrap_err();
    assert!(matches!(err, ArchiveError::ManifestMissing));

    // 4. Archive with invalid TOML
    let invalid_toml = "id = 12345\nname = [invalid";
    let invalid_manifest_path = create_test_rmod(tmp.path(), "invalid_toml.rmod", Some(invalid_toml), &[], &[]);
    let archive = RmodArchive::open(&invalid_manifest_path).unwrap();
    let err = archive.read_manifest().unwrap_err();
    assert!(matches!(err, ArchiveError::ManifestInvalid(_)));
}

#[test]
fn test_mod_manager_discover_and_resolve() {
    let tmp = tempdir().unwrap();
    let mods_dir = tmp.path().join("mods");
    std::fs::create_dir_all(&mods_dir).unwrap();

    // Create mod A: depends on mod B >= 1.0.0
    let toml_a = r#"
id = "mod_a"
name = "Mod A"
version = "1.0.0"

[dependencies]
mod_b = ">=1.0.0"
"#;
    create_test_rmod(&mods_dir, "mod_a.rmod", Some(toml_a), &[], &[]);

    // Create mod B: no dependencies
    let toml_b = r#"
id = "mod_b"
name = "Mod B"
version = "1.2.0"
"#;
    create_test_rmod(&mods_dir, "mod_b.rmod", Some(toml_b), &[], &[]);

    // Create non-rmod file in the directory
    std::fs::write(mods_dir.join("readme.txt"), b"Ignored file").unwrap();

    let blamer = Arc::new(Blamer::new());
    let manager = ModManager::new(blamer);

    let archives = manager.discover_archives(&mods_dir).expect("Discovery failed");
    assert_eq!(archives.len(), 2);

    let resolved = manager.resolve_archives(&archives).expect("Resolution failed");
    assert_eq!(resolved.len(), 2);
    // mod_b must come before mod_a
    assert_eq!(resolved[0].id, "mod_b");
    assert_eq!(resolved[1].id, "mod_a");
}

#[test]
fn test_mod_manager_resolution_cycle_detection() {
    let tmp = tempdir().unwrap();
    let mods_dir = tmp.path().join("mods_cycle");
    std::fs::create_dir_all(&mods_dir).unwrap();

    let toml_x = r#"
id = "mod_x"
name = "Mod X"
version = "1.0.0"

[dependencies]
mod_y = ">=1.0.0"
"#;
    create_test_rmod(&mods_dir, "mod_x.rmod", Some(toml_x), &[], &[]);

    let toml_y = r#"
id = "mod_y"
name = "Mod Y"
version = "1.0.0"

[dependencies]
mod_x = ">=1.0.0"
"#;
    create_test_rmod(&mods_dir, "mod_y.rmod", Some(toml_y), &[], &[]);

    let blamer = Arc::new(Blamer::new());
    let manager = ModManager::new(blamer);

    let archives = manager.discover_archives(&mods_dir).unwrap();
    let err = manager.resolve_archives(&archives).unwrap_err();

    match err {
        LoaderError::Resolver(ResolverError::CircularDependency { cycle }) => {
            assert!(cycle.contains(&"mod_x".to_string()));
            assert!(cycle.contains(&"mod_y".to_string()));
        }
        other => panic!("Expected CircularDependency, got {other:?}"),
    }
}

#[test]
fn test_target_triple_helper() {
    let triple = current_target_triple();
    assert!(!triple.is_empty());
    #[cfg(target_os = "windows")]
    assert!(triple.contains("windows"));
    #[cfg(target_os = "linux")]
    assert!(triple.contains("linux"));
    #[cfg(target_os = "macos")]
    assert!(triple.contains("darwin") || triple.contains("apple"));
}

#[test]
fn test_dylib_loader_missing_file() {
    let non_existent = PathBuf::from("this_library_does_not_exist_12345.dll");
    let res = unsafe { load_plugin_from_dylib(&non_existent) };
    match res {
        Err(DylibError::Load(_)) => {}
        Err(other) => panic!("Expected DylibError::Load, got {other:?}"),
        Ok(_) => panic!("Expected failure loading non-existent dylib"),
    }
}
