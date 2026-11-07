use rurge_client::runtime_bridge::RuntimeBridge;

#[test]
fn test_runtime_bridge_lifecycle_and_tick() {
    let temp_dir = tempfile::tempdir().expect("tempdir");
    let mods_dir = temp_dir.path().join("mods");

    let bridge = RuntimeBridge::bootstrap(&mods_dir).expect("Bootstrap failed");
    assert!(bridge.is_initialized());

    // Run 5 ticks - must execute without panics
    for tick in 1..=5 {
        bridge.on_simulation_tick(tick);
    }

    bridge.shutdown();
}
