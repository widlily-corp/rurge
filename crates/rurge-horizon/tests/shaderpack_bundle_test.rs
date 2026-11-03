fn pack_has_stage(pack: &horizon_shaders::Pack, stage: &str) -> bool {
    [".fsh", ".vsh", ".csh", ".gsh"]
        .iter()
        .any(|ext| pack.exists(&format!("{stage}{ext}")))
}

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
    assert!(pack.exists("final.fsh"));
    assert!(pack.exists("final.vsh"));
    assert!(pack.exists("composite.fsh"));
    assert!(pack.exists("composite.vsh"));
    assert!(pack.exists("gbuffers_terrain.fsh"));
    assert!(pack.exists("gbuffers_terrain.vsh"));
    assert!(pack.exists("shaders.properties"));

    assert!(pack_has_stage(&pack, "final"));
    assert!(pack_has_stage(&pack, "composite"));
    assert!(pack_has_stage(&pack, "gbuffers_terrain"));

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
    assert!(pack_has_stage(&in_memory_pack, "final"));
    assert!(pack_has_stage(&in_memory_pack, "composite"));
    assert!(pack_has_stage(&in_memory_pack, "gbuffers_terrain"));
    assert!(in_memory_pack.exists("shaders.properties"));
}

#[test]
fn test_load_post_on_default_shaderpack() {
    let bytes = rurge_horizon::shaderpack_bundle::build_shaderpack_zip_bytes()
        .expect("build_shaderpack_zip_bytes must succeed");
    let pack = horizon_shaders::Pack::from_zip_bytes("test-classic-shader.zip", bytes)
        .expect("in-memory zip bytes must be valid pack");

    let macros = horizon_shaders::Macros::standard();
    let properties = horizon_shaders::Properties::default();
    let mut inputs = horizon_shaders::GameInputs::default();
    inputs.terrain = Some(horizon_shaders::GbuffersInputs::default());

    let post = horizon_shaders::load_post(&pack, "", &properties, &macros, &inputs);
    println!("Post notes: {:?}", post.notes);
    println!("Post programs: {}", post.programs.len());
    println!("Post terrain: is_some={}", post.terrain.is_some());
    for p in &post.programs {
        println!("Program {}: is_some={}", p.name, p.program.is_some());
    }
    assert!(post.notes.is_empty(), "Notes: {:?}", post.notes);
    assert!(post.terrain.is_some(), "terrain program must be present");
}
