#[test]
fn test_shaderpack_bundle_installation_and_validation() {
    let tmp = tempfile::tempdir().unwrap();
    let shaderpacks_dir = tmp.path().join("shaderpacks");
    std::fs::create_dir_all(&shaderpacks_dir).unwrap();

    let pack_path =
        rurge_horizon::shaderpack_bundle::ensure_default_shaderpack_installed(&shaderpacks_dir)
            .expect("Must install default shaderpack");

    assert!(pack_path.exists());
    assert_eq!(pack_path.file_name().unwrap(), "test-classic-shader.zip");

    // Verify horizon-shaders recognizes and parses this pack
    let pack = horizon_shaders::Pack::open(&pack_path)
        .expect("Pack must be recognized by horizon-shaders");
    assert!(pack.has_stage("final"));
    assert!(pack.has_stage("composite"));
    assert!(pack.has_stage("gbuffers_terrain"));
    assert!(pack.exists("shaders.properties"));

    // Verify idempotency: calling ensure_default_shaderpack_installed again returns existing path
    let second_path =
        rurge_horizon::shaderpack_bundle::ensure_default_shaderpack_installed(&shaderpacks_dir)
            .expect("Must succeed on subsequent invocation");
    assert_eq!(pack_path, second_path);

    // Verify build_shaderpack_zip_bytes creates a valid pack directly from memory
    let bytes = rurge_horizon::shaderpack_bundle::build_shaderpack_zip_bytes()
        .expect("build_shaderpack_zip_bytes must succeed");
    let in_memory_pack = horizon_shaders::Pack::from_zip_bytes("in_memory.zip", bytes)
        .expect("in-memory zip bytes must be valid pack");
    assert!(in_memory_pack.has_stage("final"));
    assert!(in_memory_pack.has_stage("composite"));
    assert!(in_memory_pack.has_stage("gbuffers_terrain"));
}
