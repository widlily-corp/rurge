use rurge_installer::deployer::deploy_default_configs;
use rurge_installer::unpacker::unpack_assets_from_jar_bytes;
use std::fs;
use std::io::Write;
use zip::write::{SimpleFileOptions, ZipWriter};

#[test]
fn test_unpacker_extracts_only_assets() {
    let temp_dir = tempfile::tempdir().expect("Failed to create tempdir");
    let target_assets = temp_dir.path().join("assets");

    // Construct a mock jar in memory with some non-assets and some assets
    let mut zip_buf = Vec::new();
    {
        let mut writer = ZipWriter::new(std::io::Cursor::new(&mut zip_buf));
        let options = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);

        // Valid asset
        writer.start_file("assets/minecraft/textures/block/stone.png", options).unwrap();
        writer.write_all(b"fake_stone_texture").unwrap();

        // Another valid asset
        writer.start_file("assets/minecraft/lang/en_us.json", options).unwrap();
        writer.write_all(b"{\"menu\": \"Menu\"}").unwrap();

        // Non-asset class file that should be skipped
        writer.start_file("net/minecraft/client/main/Main.class", options).unwrap();
        writer.write_all(b"\xCA\xFE\xBA\xBE").unwrap();

        writer.finish().unwrap();
    }

    let extracted = unpack_assets_from_jar_bytes(&zip_buf, &target_assets).expect("Unpack failed");
    assert_eq!(extracted, 2);

    let stone_path = target_assets.join("minecraft/textures/block/stone.png");
    assert!(stone_path.exists());
    assert_eq!(fs::read(&stone_path).unwrap(), b"fake_stone_texture");

    let class_path = temp_dir.path().join("net/minecraft/client/main/Main.class");
    assert!(!class_path.exists());
}

#[test]
fn test_deployer_creates_compatible_options_and_dirs() {
    let temp_dir = tempfile::tempdir().expect("Failed to create tempdir");
    deploy_default_configs(temp_dir.path()).expect("Deploy configs failed");

    let options_path = temp_dir.path().join("options.txt");
    assert!(options_path.exists());

    let content = fs::read_to_string(&options_path).unwrap();
    assert!(content.contains("preferredGraphicsBackend:\"default\""));
    assert!(content.contains("fullscreen:false"));

    let shaderpacks_dir = temp_dir.path().join("shaderpacks");
    assert!(shaderpacks_dir.is_dir());

    let mods_dir = temp_dir.path().join("mods");
    assert!(mods_dir.is_dir());
}
