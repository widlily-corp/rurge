use std::sync::{
    atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering},
    Arc,
};
use rurge_api::lifecycle::{InitEvent, PreInitEvent, ServerStoppingEvent};
use rurge_blamer::Blamer;
use rurge_core::EventPriority;
use rurge_runtime::{
    ClientTickEvent, NbtCompound, RenderPassContext, RenderPassPhase, RenderPipelineHooks,
    RurgeEngine, ServerTickEvent, WorldDataAttachment,
};
use serde::{Deserialize, Serialize};

#[test]
fn test_engine_lifecycle_and_tick_loop() {
    let temp_dir = tempfile::tempdir().expect("Failed to create tempdir");
    let mods_dir = temp_dir.path().join("mods");
    let cache_dir = temp_dir.path().join("cache");

    let mut engine = RurgeEngine::new(&mods_dir, &cache_dir);

    let pre_init_received = Arc::new(AtomicBool::new(false));
    let pre_init_clone = Arc::clone(&pre_init_received);
    engine.event_bus().subscribe(EventPriority::Normal, move |_e: &mut PreInitEvent| {
        pre_init_clone.store(true, Ordering::SeqCst);
    });

    let init_received = Arc::new(AtomicBool::new(false));
    let init_clone = Arc::clone(&init_received);
    engine.event_bus().subscribe(EventPriority::Normal, move |_e: &mut InitEvent| {
        init_clone.store(true, Ordering::SeqCst);
    });

    let client_tick_received = Arc::new(AtomicU64::new(0));
    let client_tick_clone = Arc::clone(&client_tick_received);
    engine.event_bus().subscribe(EventPriority::Normal, move |e: &mut ClientTickEvent| {
        client_tick_clone.store(e.tick_count, Ordering::SeqCst);
    });

    let server_tick_received = Arc::new(AtomicU64::new(0));
    let server_tick_clone = Arc::clone(&server_tick_received);
    engine.event_bus().subscribe(EventPriority::Normal, move |e: &mut ServerTickEvent| {
        server_tick_clone.store(e.tick_count, Ordering::SeqCst);
    });

    let server_stopping_received = Arc::new(AtomicBool::new(false));
    let server_stopping_clone = Arc::clone(&server_stopping_received);
    engine.event_bus().subscribe(EventPriority::Normal, move |_e: &mut ServerStoppingEvent| {
        server_stopping_clone.store(true, Ordering::SeqCst);
    });

    // Custom tick hook registered via engine
    let hook_client_tick_count = Arc::new(AtomicU64::new(0));
    let hook_client_clone = Arc::clone(&hook_client_tick_count);
    engine.register_client_tick("test_mod", move |tick| {
        hook_client_clone.store(tick, Ordering::SeqCst);
    });

    // Initialize engine
    engine.initialize().expect("Engine initialization should succeed");
    assert!(pre_init_received.load(Ordering::SeqCst), "PreInitEvent should have been fired");
    assert!(init_received.load(Ordering::SeqCst), "InitEvent should have been fired");

    // Client tick dispatch
    engine.client_tick(42);
    assert_eq!(client_tick_received.load(Ordering::SeqCst), 42);
    assert_eq!(hook_client_tick_count.load(Ordering::SeqCst), 42);

    engine.client_tick(43);
    assert_eq!(client_tick_received.load(Ordering::SeqCst), 43);
    assert_eq!(hook_client_tick_count.load(Ordering::SeqCst), 43);

    // Server tick dispatch
    engine.server_tick(100);
    assert_eq!(server_tick_received.load(Ordering::SeqCst), 100);

    // Shutdown
    engine.shutdown();
    assert!(server_stopping_received.load(Ordering::SeqCst), "ServerStoppingEvent should have been fired");
}

#[test]
fn test_render_pipeline_hooks_execution_and_panic_suppression() {
    let mut hooks = RenderPipelineHooks::new();
    let blamer = Blamer::new();

    let pre_render_ran = Arc::new(AtomicBool::new(false));
    let pre_render_clone = Arc::clone(&pre_render_ran);
    hooks.register_pass(
        "visual_mod".to_string(),
        RenderPassPhase::PreRender,
        move |_ctx| {
            pre_render_clone.store(true, Ordering::SeqCst);
        },
    );

    // Deliberately panicking hook
    hooks.register_pass(
        "faulty_hud_mod".to_string(),
        RenderPassPhase::HudOverlay,
        |_ctx| {
            panic!("Intentional GPU pass crash");
        },
    );

    // Subsequent hook in same phase should still run (panic suppressed for the frame)
    let safe_hud_ran = Arc::new(AtomicBool::new(false));
    let safe_hud_clone = Arc::clone(&safe_hud_ran);
    hooks.register_pass(
        "safe_hud_mod".to_string(),
        RenderPassPhase::HudOverlay,
        move |_ctx| {
            safe_hud_clone.store(true, Ordering::SeqCst);
        },
    );

    let mut ctx = RenderPassContext::new(1, 1920, 1080);
    assert_eq!(ctx.frame_index, 1);
    assert_eq!(ctx.width, 1920);
    assert_eq!(ctx.height, 1080);
    assert!(ctx.custom_data.is_none());

    // Execute PreRender phase
    hooks.execute_phase(RenderPassPhase::PreRender, &blamer, &mut ctx);
    assert!(pre_render_ran.load(Ordering::SeqCst));
    assert_eq!(blamer.get_recent_panics().len(), 0);

    // Execute HudOverlay phase containing panic
    // This MUST NOT panic the calling thread!
    hooks.execute_phase(RenderPassPhase::HudOverlay, &blamer, &mut ctx);

    // Blamer must have recorded the crash for faulty_hud_mod
    let panics = blamer.get_recent_panics();
    assert_eq!(panics.len(), 1);
    assert_eq!(panics[0].mod_id, "faulty_hud_mod");
    assert_eq!(panics[0].hook_name, "render_pass");
    assert!(panics[0].panic_message.contains("Intentional GPU pass crash"));

    // Safe hook must have executed
    assert!(safe_hud_ran.load(Ordering::SeqCst));
}

#[test]
fn test_render_pass_context_custom_data() {
    let mut hooks = RenderPipelineHooks::new();
    let blamer = Blamer::new();

    let counter = Arc::new(AtomicU32::new(0));
    let counter_clone = Arc::clone(&counter);

    hooks.register_pass(
        "custom_mod".to_string(),
        RenderPassPhase::PostProcess,
        move |ctx| {
            if let Some(custom) = &mut ctx.custom_data {
                if let Some(val) = custom.downcast_mut::<u32>() {
                    *val += 10;
                    counter_clone.store(*val, Ordering::SeqCst);
                }
            }
        },
    );

    let mut custom_value: u32 = 5;
    let mut ctx = RenderPassContext::with_custom_data(10, 800, 600, &mut custom_value);

    hooks.execute_phase(RenderPassPhase::PostProcess, &blamer, &mut ctx);
    assert_eq!(counter.load(Ordering::SeqCst), 15);
}

#[derive(Debug, Serialize, Deserialize, PartialEq)]
struct CustomPlayerStats {
    mana: u32,
    max_mana: u32,
    spells: Vec<String>,
}

#[test]
fn test_world_data_attachment_roundtrip_and_vanilla_isolation() {
    let mut attachment = WorldDataAttachment::new();

    let stats = CustomPlayerStats {
        mana: 85,
        max_mana: 100,
        spells: vec!["fireball".to_string(), "teleport".to_string()],
    };

    attachment
        .set("magic_mod", "player_stats", &stats)
        .expect("Setting stats should succeed");
    attachment
        .set("magic_mod", "cast_count", &42u64)
        .expect("Setting cast count should succeed");
    attachment
        .set("minimap_mod", "zoom_level", &1.5f32)
        .expect("Setting zoom level should succeed");

    // Reading before saving
    let retrieved_stats: Option<CustomPlayerStats> = attachment
        .get("magic_mod", "player_stats")
        .expect("Get stats should succeed");
    assert_eq!(retrieved_stats, Some(stats));

    // Vanilla chunk save simulation
    let mut root = NbtCompound::new();
    root.insert("DataVersion", serde_json::json!(3465));
    root.insert("Level", serde_json::json!({
        "xPos": 12,
        "zPos": -3,
        "Status": "full"
    }));

    // Save attachment to root
    attachment
        .save_to_nbt(&mut root)
        .expect("Save to NBT should succeed");

    // Verify vanilla isolation: "DataVersion" and "Level" are untouched
    assert_eq!(root.get("DataVersion"), Some(&serde_json::json!(3465)));
    assert_eq!(
        root.get("Level"),
        Some(&serde_json::json!({
            "xPos": 12,
            "zPos": -3,
            "Status": "full"
        }))
    );

    // Verify mod data is namespaced under "rurge:mods"
    assert!(root.get("rurge:mods").is_some());

    // Roundtrip: load into a completely fresh attachment
    let mut loaded_attachment = WorldDataAttachment::new();
    loaded_attachment
        .load_from_nbt(&root)
        .expect("Load from NBT should succeed");

    let restored_stats: Option<CustomPlayerStats> = loaded_attachment
        .get("magic_mod", "player_stats")
        .expect("Get restored stats should succeed");
    assert_eq!(restored_stats, retrieved_stats);

    let restored_cast_count: Option<u64> = loaded_attachment
        .get("magic_mod", "cast_count")
        .expect("Get cast count should succeed");
    assert_eq!(restored_cast_count, Some(42));

    let restored_zoom: Option<f32> = loaded_attachment
        .get("minimap_mod", "zoom_level")
        .expect("Get zoom level should succeed");
    assert_eq!(restored_zoom, Some(1.5f32));

    // Query non-existent key / mod
    let non_existent: Option<String> = loaded_attachment
        .get("magic_mod", "unknown_key")
        .expect("Query non-existent key");
    assert_eq!(non_existent, None);

    let unknown_mod: Option<u32> = loaded_attachment
        .get("unknown_mod", "foo")
        .expect("Query unknown mod");
    assert_eq!(unknown_mod, None);
}

#[test]
fn test_world_data_attachment_empty_nbt_load() {
    let mut attachment = WorldDataAttachment::new();
    let vanilla_root = NbtCompound::new();

    // Loading from vanilla NBT with no "rurge:mods" should succeed gracefully without error
    let res = attachment.load_from_nbt(&vanilla_root);
    assert!(res.is_ok());

    let val: Option<i32> = attachment.get("any_mod", "key").unwrap();
    assert_eq!(val, None);
}

#[test]
fn test_engine_tick_hook_panic_safety() {
    let temp_dir = tempfile::tempdir().expect("Failed to create tempdir");
    let mut engine = RurgeEngine::new(temp_dir.path().join("mods"), temp_dir.path().join("cache"));

    // Register a panicking client tick handler
    engine.register_client_tick("faulty_tick_mod", |_tick| {
        panic!("Crash in tick hook");
    });

    // Register a valid tick handler
    let valid_tick_called = Arc::new(AtomicBool::new(false));
    let valid_tick_clone = Arc::clone(&valid_tick_called);
    engine.register_client_tick("valid_tick_mod", move |_tick| {
        valid_tick_clone.store(true, Ordering::SeqCst);
    });

    // client_tick should NOT panic
    engine.client_tick(1);

    // Panic should be blamed to "faulty_tick_mod"
    assert_eq!(engine.blamer().panic_count_for_mod("faulty_tick_mod"), 1);
    assert!(valid_tick_called.load(Ordering::SeqCst));
}
