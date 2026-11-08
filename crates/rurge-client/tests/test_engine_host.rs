use rurge_client::engine_host::EngineConfig;
use tempfile::tempdir;

#[test]
fn test_engine_host_preflight_validates_environment() {
    let dir = tempdir().unwrap();
    let config = EngineConfig {
        game_dir: dir.path().to_path_buf(),
        mods_dir: dir.path().join("mods"),
        shaderpacks_dir: dir.path().join("shaderpacks"),
        target_fps: 144,
    };
    let preflight = config.preflight();
    assert!(preflight.is_ok());
    assert!(config.mods_dir.is_dir());
    assert!(config.shaderpacks_dir.is_dir());
}
