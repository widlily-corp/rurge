use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use rurge_api::lifecycle::{InitEvent, PreInitEvent, ServerStoppingEvent};
use rurge_core::manifest::ModEnvironment;
use rurge_core::EventPriority;
use rurge_loader::RmodArchive;
use rurge_runtime::{
    ClientTickEvent, RenderPassContext, RenderPassPhase, RurgeEngine,
};
use simple_hud_mod::{PlayerCoords, SimpleHudMod};

/// Helper to ensure `simple-hud-mod` dynamic library is built before packaging.
fn ensure_simple_hud_mod_built() {
    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".to_string());
    let status = Command::new(cargo)
        .args(["build", "-p", "simple-hud-mod"])
        .status()
        .expect("Failed to execute cargo build for simple-hud-mod");
    assert!(status.success(), "Building simple-hud-mod cdylib must succeed");
}

#[test]
fn test_end_to_end_simple_hud_mod_lifecycle() {
    // 0. Ensure the cdylib binary is up to date
    ensure_simple_hud_mod_built();

    let temp_dir = tempfile::tempdir().expect("Failed to create temporary test directory");
    let mods_dir = temp_dir.path().join("mods");
    let cache_dir = temp_dir.path().join("cache");
    std::fs::create_dir_all(&mods_dir).expect("Failed to create mods directory");
    std::fs::create_dir_all(&cache_dir).expect("Failed to create cache directory");

    let workspace_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let example_dir = workspace_dir.join("examples").join("simple-hud-mod");

    // 1. Package simple-hud-mod into a real .rmod ZIP archive using cargo-rurge
    let pack_args = cargo_rurge::commands::pack::PackArgs {
        path: Some(example_dir),
        out: Some(mods_dir.clone()),
        release: false,
        target: None,
        binary: None,
    };

    let rmod_path = cargo_rurge::commands::pack::execute(pack_args)
        .expect("cargo-rurge pack should package simple-hud-mod into .rmod");

    assert!(rmod_path.exists(), "Packaged .rmod archive must exist on disk");
    assert_eq!(
        rmod_path.file_name().unwrap().to_str().unwrap(),
        "simple_hud-0.1.0.rmod",
        "Archive file name must match convention {{id}}-{{version}}.rmod"
    );

    // Verify .rmod contents using RmodArchive
    let archive = RmodArchive::open(&rmod_path).expect("RmodArchive::open must succeed");
    let manifest = archive.read_manifest().expect("Must read valid manifest from archive");
    assert_eq!(manifest.id, "simple_hud");
    assert_eq!(manifest.name, "Simple HUD Mod");
    assert_eq!(manifest.version.to_string(), "0.1.0");
    assert_eq!(manifest.environment, ModEnvironment::Client);

    // 2. Initialize RurgeEngine pointing to the mods directory
    let mut engine = RurgeEngine::new(&mods_dir, &cache_dir);
    assert!(!engine.is_initialized(), "Engine must initially be uninitialized");

    // Track lifecycle events dispatched on the EventBus
    let pre_init_mod_ids = Arc::new(Mutex::new(Vec::new()));
    let pre_init_clone = Arc::clone(&pre_init_mod_ids);
    engine.event_bus().subscribe(EventPriority::Normal, move |e: &mut PreInitEvent| {
        pre_init_clone.lock().unwrap().push(e.mod_id.to_string());
    });

    let init_mod_ids = Arc::new(Mutex::new(Vec::new()));
    let init_clone = Arc::clone(&init_mod_ids);
    engine.event_bus().subscribe(EventPriority::Normal, move |e: &mut InitEvent| {
        init_clone.lock().unwrap().push(e.mod_id.to_string());
    });

    let client_tick_counts = Arc::new(AtomicU64::new(0));
    let client_tick_clone = Arc::clone(&client_tick_counts);
    engine.event_bus().subscribe(EventPriority::Normal, move |e: &mut ClientTickEvent| {
        client_tick_clone.store(e.tick_count, Ordering::SeqCst);
    });

    let server_stopping_fired = Arc::new(AtomicBool::new(false));
    let server_stopping_clone = Arc::clone(&server_stopping_fired);
    engine.event_bus().subscribe(EventPriority::Normal, move |_e: &mut ServerStoppingEvent| {
        server_stopping_clone.store(true, Ordering::SeqCst);
    });

    // 3. Register tick hook attributed to simple_hud
    let hook_tick_counter = Arc::new(AtomicU64::new(0));
    let hook_tick_clone = Arc::clone(&hook_tick_counter);
    let should_panic_flag = Arc::new(AtomicBool::new(false));
    let panic_flag_clone = Arc::clone(&should_panic_flag);

    engine.register_client_tick("simple_hud", move |tick| {
        if panic_flag_clone.load(Ordering::SeqCst) {
            panic!("Intentional panic in simple_hud at tick {tick}!");
        }
        hook_tick_clone.store(tick, Ordering::SeqCst);
    });

    // Register render pipeline HUD overlay pass
    let hud_pass_ran = Arc::new(AtomicBool::new(false));
    let hud_pass_clone = Arc::clone(&hud_pass_ran);
    engine.render_hooks_mut().register_pass(
        "simple_hud".to_string(),
        RenderPassPhase::HudOverlay,
        move |_ctx| {
            hud_pass_clone.store(true, Ordering::SeqCst);
        },
    );

    // 4. Initialize engine: discovery, DAG resolution, dylib extraction, and dynamic loading
    engine.initialize().expect("Engine initialization must succeed");
    assert!(engine.is_initialized(), "Engine must be in initialized state");

    // Verify mod discovery and dynamic loading
    let loaded = engine.mod_manager().loaded_plugins();
    assert_eq!(loaded.len(), 1, "Exactly one plugin should be loaded");
    assert_eq!(loaded[0].manifest.id, "simple_hud");
    assert_eq!(loaded[0].metadata.id.as_str(), "simple_hud");
    assert_eq!(loaded[0].metadata.name.as_str(), "Simple HUD Mod");
    assert_eq!(loaded[0].metadata.version.as_str(), "0.1.0");

    // Verify dylib extraction to cache directory
    let extracted_mod_dir = cache_dir.join("simple_hud");
    assert!(extracted_mod_dir.is_dir(), "Extracted mod directory must exist in cache");
    let entries: Vec<_> = std::fs::read_dir(&extracted_mod_dir)
        .unwrap()
        .filter_map(|e| e.ok())
        .collect();
    assert!(!entries.is_empty(), "Cache dir must contain extracted dylib file");

    // Verify lifecycle events dispatched for simple_hud
    assert!(
        pre_init_mod_ids.lock().unwrap().contains(&"simple_hud".to_string()),
        "PreInitEvent must be fired for simple_hud"
    );
    assert!(
        init_mod_ids.lock().unwrap().contains(&"simple_hud".to_string()),
        "InitEvent must be fired for simple_hud"
    );

    // Verify calling through the dynamically loaded library directly
    unsafe {
        let tick_fn: libloading::Symbol<unsafe extern "C" fn(u64)> = loaded[0]
            ._library
            .get(b"simple_hud_tick\0")
            .expect("simple_hud_tick symbol must be exported by cdylib");
        let get_ticks_fn: libloading::Symbol<unsafe extern "C" fn() -> u64> = loaded[0]
            ._library
            .get(b"simple_hud_get_ticks\0")
            .expect("simple_hud_get_ticks symbol must be exported by cdylib");

        tick_fn(100);
        assert_eq!(get_ticks_fn(), 100);
    }

    // 5. Verify client tick execution
    engine.client_tick(1);
    assert_eq!(client_tick_counts.load(Ordering::SeqCst), 1);
    assert_eq!(hook_tick_counter.load(Ordering::SeqCst), 1);

    engine.client_tick(2);
    assert_eq!(client_tick_counts.load(Ordering::SeqCst), 2);
    assert_eq!(hook_tick_counter.load(Ordering::SeqCst), 2);

    engine.client_tick(3);
    assert_eq!(client_tick_counts.load(Ordering::SeqCst), 3);
    assert_eq!(hook_tick_counter.load(Ordering::SeqCst), 3);

    // Verify coordinate HUD overlay formatting helper
    let coords = PlayerCoords { x: 100.5, y: 64.0, z: -200.25 };
    let hud_text = SimpleHudMod::render_coordinates(coords);
    assert!(hud_text.contains("XYZ: 100.50 / 64.00 / -200.25"));

    // Verify render pass execution
    let mut render_ctx = RenderPassContext::new(1, 1920, 1080);
    engine.render_hooks().execute_phase(
        RenderPassPhase::HudOverlay,
        engine.blamer(),
        &mut render_ctx,
    );
    assert!(hud_pass_ran.load(Ordering::SeqCst), "HUD overlay render pass must execute");

    // 6. Verify panic containment and blame attribution without process exit
    assert_eq!(engine.blamer().get_recent_panics().len(), 0);

    // Trigger intentional panic in mod hook on tick 4
    should_panic_flag.store(true, Ordering::SeqCst);
    engine.client_tick(4);

    // Blamer must catch the panic and attribute it to "simple_hud" without terminating the process
    let panics = engine.blamer().get_recent_panics();
    assert_eq!(panics.len(), 1, "Exactly one panic must be recorded");
    assert_eq!(panics[0].mod_id, "simple_hud");
    assert_eq!(panics[0].hook_name, "client_tick");
    assert!(
        panics[0].panic_message.contains("Intentional panic in simple_hud at tick 4"),
        "Panic message must match expected text, got: {}",
        panics[0].panic_message
    );
    assert_eq!(engine.blamer().panic_count_for_mod("simple_hud"), 1);

    // 7. Verify engine continues running healthy and can process subsequent ticks normally
    should_panic_flag.store(false, Ordering::SeqCst);

    engine.client_tick(5);
    assert_eq!(client_tick_counts.load(Ordering::SeqCst), 5);
    assert_eq!(hook_tick_counter.load(Ordering::SeqCst), 5);

    engine.client_tick(6);
    assert_eq!(client_tick_counts.load(Ordering::SeqCst), 6);
    assert_eq!(hook_tick_counter.load(Ordering::SeqCst), 6);

    // Panic count should not have increased
    assert_eq!(engine.blamer().panic_count_for_mod("simple_hud"), 1);

    // 8. Clean engine shutdown
    engine.shutdown();
    assert!(!engine.is_initialized(), "Engine must no longer be initialized");
    assert!(server_stopping_fired.load(Ordering::SeqCst), "ServerStoppingEvent must be fired");
}
