use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use horizon_client::mod_api::Hooks;
use rurge_horizon::hooks::RurgeHorizonHooks;
use rurge_runtime::engine::RurgeEngine;

#[test]
fn test_rurge_horizon_hooks_creation_and_render_query() {
    let engine = Arc::new(RurgeEngine::new("mods", ".cache"));
    let hooks = RurgeHorizonHooks::new(engine.clone());

    assert!(hooks.render().is_some(), "RenderHooks must be provided");

    let mut params = [[0.0f32; 4]; 8];
    hooks.shader_params(&mut params);
}

#[test]
fn test_rurge_horizon_hooks_engine_dispatch() {
    let engine = Arc::new(RurgeEngine::new("mods", ".cache"));
    let executed = Arc::new(Mutex::new(false));
    let executed_clone = executed.clone();

    engine.register_world_drawn_hook("test-mod", Box::new(move |_ctx| {
        let mut val = executed_clone.lock().unwrap();
        *val = true;
    }));

    let hooks = RurgeHorizonHooks::new(engine.clone());
    let ctx = rurge_api::events::RenderPassContext {
        viewport_width: 1920,
        viewport_height: 1080,
        partial_tick: 0.0,
        camera_pos: [0.0, 0.0, 0.0],
        view_matrix: [1.0; 16],
    };

    hooks.engine.execute_world_drawn(&ctx);
    assert!(*executed.lock().unwrap(), "Hook must have executed through engine");
}

#[test]
fn test_rurge_horizon_hooks_hand_drawn_dispatch() {
    let engine = Arc::new(RurgeEngine::new("mods", ".cache"));
    let executed = Arc::new(Mutex::new(false));
    let executed_clone = executed.clone();

    engine.register_hand_drawn_hook("test-mod", Box::new(move |_ctx| {
        let mut val = executed_clone.lock().unwrap();
        *val = true;
    }));

    let hooks = RurgeHorizonHooks::new(engine.clone());
    let ctx = rurge_api::events::RenderPassContext {
        viewport_width: 1920,
        viewport_height: 1080,
        partial_tick: 0.0,
        camera_pos: [0.0, 0.0, 0.0],
        view_matrix: [1.0; 16],
    };

    hooks.engine.execute_hand_drawn(&ctx);
    assert!(*executed.lock().unwrap(), "Hand drawn hook must have executed through engine");
}

#[test]
fn test_rurge_horizon_hooks_panic_isolation() {
    let engine = Arc::new(RurgeEngine::new("mods", ".cache"));
    let hook_ran = Arc::new(AtomicBool::new(false));
    let hook_ran_clone = hook_ran.clone();

    engine.register_world_drawn_hook("faulty-mod", Box::new(move |_ctx| {
        hook_ran_clone.store(true, Ordering::SeqCst);
        panic!("Simulated crash in mod world_drawn hook");
    }));

    let hooks = RurgeHorizonHooks::new(engine.clone());
    let ctx = rurge_api::events::RenderPassContext {
        viewport_width: 1920,
        viewport_height: 1080,
        partial_tick: 0.0,
        camera_pos: [0.0, 0.0, 0.0],
        view_matrix: [1.0; 16],
    };

    // Safe execution boundary should catch panic and not crash test runner
    hooks.engine.execute_world_drawn(&ctx);
    assert!(hook_ran.load(Ordering::SeqCst));
    assert_eq!(hooks.engine.blamer().panic_count_for_mod("faulty-mod"), 1);
}
