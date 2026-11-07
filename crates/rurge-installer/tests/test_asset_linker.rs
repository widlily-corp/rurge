use std::fs;
use tempfile::tempdir;

#[test]
fn test_sound_asset_linker_reconstructs_virtual_tree() {
    let dir = tempdir().unwrap();
    let assets = dir.path().join("assets");
    let indexes = assets.join("indexes");
    let objects = assets.join("objects");
    let target = dir.path().join("target_content");

    fs::create_dir_all(&indexes).unwrap();
    fs::create_dir_all(&target).unwrap();

    // Create a mock object file
    let hash = "e877b6a0abcdef1234567890abcdef1234567890";
    let obj_dir = objects.join(&hash[..2]);
    fs::create_dir_all(&obj_dir).unwrap();
    fs::write(obj_dir.join(hash), b"OGG_MOCK_DATA").unwrap();

    // Create mock 34.json mapping virtual sounds to hash
    let index_json = serde_json::json!({
        "objects": {
            "minecraft/sounds.json": {
                "hash": hash,
                "size": 13
            },
            "minecraft/sounds/step/stone1.ogg": {
                "hash": hash,
                "size": 13
            }
        }
    });
    fs::write(indexes.join("34.json"), serde_json::to_vec(&index_json).unwrap()).unwrap();

    let linked = rurge_installer::asset_linker::link_sound_assets(&assets, &[&target]).unwrap();
    assert_eq!(linked, 2);

    assert!(target.join("assets/minecraft/sounds.json").is_file());
    assert!(target.join("assets/minecraft/sounds/step/stone1.ogg").is_file());
    assert_eq!(
        fs::read(target.join("assets/minecraft/sounds.json")).unwrap(),
        b"OGG_MOCK_DATA"
    );
}

#[test]
fn test_sound_asset_linker_skips_missing_objects_gracefully() {
    let dir = tempdir().unwrap();
    let assets = dir.path().join("assets");
    let indexes = assets.join("indexes");
    let objects = assets.join("objects");
    let target = dir.path().join("target_content");

    fs::create_dir_all(&indexes).unwrap();
    fs::create_dir_all(&target).unwrap();

    // Create only one mock object file, leaving the second missing
    let hash_present = "11223344556677889900aabbccddeeff00112233";
    let hash_missing = "99887766554433221100ffeeddccbbaa99887766";

    let obj_dir = objects.join(&hash_present[..2]);
    fs::create_dir_all(&obj_dir).unwrap();
    fs::write(obj_dir.join(hash_present), b"VALID_PRESENT_DATA").unwrap();

    let index_json = serde_json::json!({
        "objects": {
            "minecraft/sounds.json": {
                "hash": hash_present,
                "size": 18
            },
            "minecraft/sounds/missing.ogg": {
                "hash": hash_missing,
                "size": 40
            }
        }
    });
    fs::write(indexes.join("34.json"), serde_json::to_vec(&index_json).unwrap()).unwrap();

    // Must not panic or return Err, skips missing object and links present one
    let linked = rurge_installer::asset_linker::link_sound_assets(&assets, &[&target]).unwrap();
    assert_eq!(linked, 1);
    assert!(target.join("assets/minecraft/sounds.json").is_file());
    assert!(!target.join("assets/minecraft/sounds/missing.ogg").exists());
}
