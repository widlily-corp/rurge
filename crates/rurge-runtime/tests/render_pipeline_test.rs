use rurge_api::events::RenderPassContext;
use rurge_blamer::Blamer;
use rurge_runtime::render::RenderPipelineHooks;
use rurge_runtime::RurgeEngine;
use std::sync::{Arc, Mutex};

#[test]
fn test_render_pipeline_world_drawn_and_panic_isolation() {
    let blamer = Arc::new(Blamer::new());
    let hooks = RenderPipelineHooks::new();
    let executed = Arc::new(Mutex::new(false));
    let executed_clone = executed.clone();

    // Register a valid hook
    hooks.register_world_drawn("valid-mod", Box::new(move |_ctx| {
        let mut val = executed_clone.lock().unwrap();
        *val = true;
    }));

    // Register a panicking hook
    hooks.register_world_drawn("panicking-mod", Box::new(|_ctx| {
        panic!("simulated render hook panic!");
    }));

    let ctx = RenderPassContext {
        viewport_width: 800,
        viewport_height: 600,
        partial_tick: 1.0,
        camera_pos: [0.0, 0.0, 0.0],
        view_matrix: [0.0; 16],
    };

    // Execute with blamer isolation
    let ok = hooks.execute_world_drawn(&ctx, &blamer);
    assert!(!ok, "execute_world_drawn must return false when a hook panics");

    assert!(*executed.lock().unwrap(), "Valid hook must have executed");
    assert_eq!(blamer.recent_panics().len(), 1, "Panic must be trapped and blamed");
    assert_eq!(blamer.recent_panics()[0].mod_id, "panicking-mod");
}

#[test]
fn test_render_pipeline_hand_drawn_and_panic_isolation() {
    let blamer = Arc::new(Blamer::new());
    let hooks = RenderPipelineHooks::new();
    let executed = Arc::new(Mutex::new(false));
    let executed_clone = executed.clone();

    // Register a valid hook
    hooks.register_hand_drawn("valid-hand-mod", Box::new(move |_ctx| {
        let mut val = executed_clone.lock().unwrap();
        *val = true;
    }));

    // Register a panicking hook
    hooks.register_hand_drawn("panicking-hand-mod", Box::new(|_ctx| {
        panic!("simulated hand hook panic!");
    }));

    let ctx = RenderPassContext {
        viewport_width: 1920,
        viewport_height: 1080,
        partial_tick: 0.5,
        camera_pos: [10.0, 64.0, -10.0],
        view_matrix: [1.0; 16],
    };

    // Execute with blamer isolation
    let ok = hooks.execute_hand_drawn(&ctx, &blamer);
    assert!(!ok, "execute_hand_drawn must return false when a hook panics");

    assert!(*executed.lock().unwrap(), "Valid hook must have executed");
    assert_eq!(blamer.recent_panics().len(), 1, "Panic must be trapped and blamed");
    assert_eq!(blamer.recent_panics()[0].mod_id, "panicking-hand-mod");
    assert_eq!(blamer.recent_panics()[0].hook_name, "hand_drawn");
}

#[test]
fn test_engine_render_hooks_delegation() {
    let temp_dir = tempfile::tempdir().expect("tempdir");
    let engine = RurgeEngine::with_dirs(temp_dir.path().join("mods"), temp_dir.path().join("cache"));

    assert!(!engine.has_world_drawn_hooks());
    assert!(!engine.has_hand_drawn_hooks());

    let world_executed = Arc::new(Mutex::new(false));
    let world_clone = world_executed.clone();
    engine.register_world_drawn_hook("engine-world-mod", Box::new(move |_ctx| {
        *world_clone.lock().unwrap() = true;
    }));

    let hand_executed = Arc::new(Mutex::new(false));
    let hand_clone = hand_executed.clone();
    engine.register_hand_drawn_hook("engine-hand-mod", Box::new(move |_ctx| {
        *hand_clone.lock().unwrap() = true;
    }));

    assert!(engine.has_world_drawn_hooks());
    assert!(engine.has_hand_drawn_hooks());

    let ctx = RenderPassContext {
        viewport_width: 1280,
        viewport_height: 720,
        partial_tick: 0.0,
        camera_pos: [0.0; 3],
        view_matrix: [0.0; 16],
    };

    let world_ok = engine.execute_world_drawn(&ctx);
    let hand_ok = engine.execute_hand_drawn(&ctx);

    assert!(world_ok);
    assert!(hand_ok);
    assert!(*world_executed.lock().unwrap());
    assert!(*hand_executed.lock().unwrap());
}
