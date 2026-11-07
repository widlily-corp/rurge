use rurge_client::sound_linker::ensure_sound_assets_linked;
use std::fs;
use tempfile::tempdir;

#[test]
fn test_client_sound_linker_integration() {
    let dir = tempdir().unwrap();
    let base_dir = dir.path();
    let assets = base_dir.join("assets");
    let indexes = assets.join("indexes");
    let objects = assets.join("objects");

    fs::create_dir_all(&indexes).unwrap();
    fs::create_dir_all(&objects).unwrap();

    let hash = "a1b2c3d4e5f60718293a4b5c6d7e8f9012345678";
    let obj_dir = objects.join(&hash[..2]);
    fs::create_dir_all(&obj_dir).unwrap();
    fs::write(obj_dir.join(hash), b"AUDIO_DATA").unwrap();

    let index_json = serde_json::json!({
        "objects": {
            "minecraft/sounds.json": {
                "hash": hash,
                "size": 10
            }
        }
    });
    fs::write(indexes.join("34.json"), serde_json::to_vec(&index_json).unwrap()).unwrap();

    let linked = ensure_sound_assets_linked(base_dir).unwrap();
    assert_eq!(linked, 1);

    // Verify linked in base_dir/assets/...
    assert!(base_dir.join("assets/minecraft/sounds.json").is_file());
    // Verify linked in base_dir/.horizon-cache/mod-content/assets/...
    assert!(base_dir.join(".horizon-cache/mod-content/assets/minecraft/sounds.json").is_file());
}

#[test]
fn test_client_sound_linker_missing_dir_is_non_fatal() {
    let dir = tempdir().unwrap();
    // Assets directory doesn't exist
    let result = ensure_sound_assets_linked(dir.path());
    assert_eq!(result.unwrap(), 0);
}
