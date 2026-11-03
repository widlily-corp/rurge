use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use horizon_client::mod_api::Hooks;
use rurge_api::events::{ShaderpackLoadedEvent, ShaderpackUnloadedEvent};
use rurge_horizon::hooks::RurgeHorizonHooks;
use rurge_runtime::engine::RurgeEngine;

#[test]
fn test_rurge_horizon_hooks_creation_and_render_query() {
    let engine = Arc::new(RurgeEngine::new());
    let hooks = RurgeHorizonHooks::new(engine.clone());

    assert!(hooks.render().is_some(), "RenderHooks must be provided");
    assert!(!engine.has_world_drawn_hooks(), "Initially no world drawn hooks");
    assert!(!engine.has_hand_drawn_hooks(), "Initially no hand drawn hooks");

    let mut params = [[0.0f32; 4]; 8];
    hooks.shader_params(&mut params);
    assert_eq!(params, [[0.0; 4]; 8]);
}

#[test]
fn test_rurge_horizon_hooks_engine_dispatch() {
    let engine = Arc::new(RurgeEngine::new());
    let executed = Arc::new(Mutex::new(false));
    let executed_clone = executed.clone();

    engine.register_world_drawn_hook("test-mod", Box::new(move |_ctx| {
        let mut val = executed_clone.lock().unwrap();
        *val = true;
    }));

    assert!(engine.has_world_drawn_hooks(), "World drawn hooks now present");

    let hooks = RurgeHorizonHooks::new(engine.clone());
    let ctx = rurge_api::events::RenderPassContext {
        viewport_width: 1920,
        viewport_height: 1080,
        partial_tick: 0.0,
        camera_pos: [0.0, 0.0, 0.0],
        view_matrix: [1.0; 16],
    };

    let ok = hooks.engine.execute_world_drawn(&ctx);
    assert!(ok, "Execution must succeed when hooks do not panic");
    assert!(*executed.lock().unwrap(), "Hook must have executed through engine");
}

#[test]
fn test_rurge_horizon_hooks_hand_drawn_dispatch() {
    let engine = Arc::new(RurgeEngine::new());
    let executed = Arc::new(Mutex::new(false));
    let executed_clone = executed.clone();

    engine.register_hand_drawn_hook("test-mod", Box::new(move |_ctx| {
        let mut val = executed_clone.lock().unwrap();
        *val = true;
    }));

    assert!(engine.has_hand_drawn_hooks(), "Hand drawn hooks now present");

    let hooks = RurgeHorizonHooks::new(engine.clone());
    let ctx = rurge_api::events::RenderPassContext {
        viewport_width: 1920,
        viewport_height: 1080,
        partial_tick: 0.0,
        camera_pos: [0.0, 0.0, 0.0],
        view_matrix: [1.0; 16],
    };

    let ok = hooks.engine.execute_hand_drawn(&ctx);
    assert!(ok, "Execution must succeed when hand drawn hooks do not panic");
    assert!(*executed.lock().unwrap(), "Hand drawn hook must have executed through engine");
}

#[test]
fn test_rurge_horizon_hooks_panic_isolation() {
    let engine = Arc::new(RurgeEngine::new());
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

    // Safe execution boundary should catch panic and return false
    let ok = hooks.engine.execute_world_drawn(&ctx);
    assert!(!ok, "execute_world_drawn must report false when a mod hook panics");
    assert!(hook_ran.load(Ordering::SeqCst));
    assert_eq!(hooks.engine.blamer().panic_count_for_mod("faulty-mod"), 1);
}

#[test]
fn test_rurge_horizon_shader_uniforms_transfer() {
    let engine = Arc::new(RurgeEngine::new());
    engine.set_shader_uniform(0, [1.0, 2.0, 3.0, 4.0]);
    engine.set_shader_uniform(7, [5.5, 6.5, 7.5, 8.5]);

    let hooks = RurgeHorizonHooks::new(engine.clone());
    let mut params = [[0.0f32; 4]; 8];
    hooks.shader_params(&mut params);

    assert_eq!(params[0], [1.0, 2.0, 3.0, 4.0]);
    assert_eq!(params[1], [0.0, 0.0, 0.0, 0.0]);
    assert_eq!(params[7], [5.5, 6.5, 7.5, 8.5]);
}

#[test]
fn test_rurge_horizon_shader_transition_dispatch() {
    let engine = Arc::new(RurgeEngine::new());
    let event_bus = engine.event_bus();

    let loaded_count = Arc::new(AtomicUsize::new(0));
    let last_loaded_name = Arc::new(Mutex::new(String::new()));
    let loaded_count_clone = loaded_count.clone();
    let last_loaded_name_clone = last_loaded_name.clone();

    event_bus.register(0, move |event: &ShaderpackLoadedEvent| {
        loaded_count_clone.fetch_add(1, Ordering::SeqCst);
        let mut name = last_loaded_name_clone.lock().unwrap();
        *name = event.pack_name.as_str().to_string();
    });

    let unloaded_count = Arc::new(AtomicUsize::new(0));
    let unloaded_count_clone = unloaded_count.clone();

    event_bus.register(0, move |_event: &ShaderpackUnloadedEvent| {
        unloaded_count_clone.fetch_add(1, Ordering::SeqCst);
    });

    let hooks = RurgeHorizonHooks::new(engine.clone());

    // 1. Transition from None to Some("Solas.zip")
    hooks.check_and_dispatch_shader_transition(Some("Solas.zip".to_string()));
    assert_eq!(loaded_count.load(Ordering::SeqCst), 1);
    assert_eq!(*last_loaded_name.lock().unwrap(), "Solas.zip");
    assert_eq!(hooks.current_shader().as_deref(), Some("Solas.zip"));

    // 2. Call again with same shader - no duplicate event should fire
    hooks.check_and_dispatch_shader_transition(Some("Solas.zip".to_string()));
    assert_eq!(loaded_count.load(Ordering::SeqCst), 1);

    // 3. Transition to None (unloaded)
    hooks.check_and_dispatch_shader_transition(None);
    assert_eq!(unloaded_count.load(Ordering::SeqCst), 1);
    assert_eq!(hooks.current_shader(), None);

    // 4. Call again with None - no duplicate event
    hooks.check_and_dispatch_shader_transition(None);
    assert_eq!(unloaded_count.load(Ordering::SeqCst), 1);
}

#[test]
fn test_rurge_horizon_active_shaderpack_query_from_file() {
    let temp_dir = tempfile::tempdir().expect("tempdir");
    let graphics_file = temp_dir.path().join("horizon-graphics.txt");

    std::fs::write(&graphics_file, "preset:custom\npickedShaderPack:Complementary.zip\nfxaa:true\n")
        .expect("write graphics config");

    let pack = RurgeHorizonHooks::query_active_shaderpack_in(temp_dir.path());
    assert_eq!(pack.as_deref(), Some("Complementary.zip"));

    // Overwrite with empty
    std::fs::write(&graphics_file, "preset:custom\npickedShaderPack:\nfxaa:true\n")
        .expect("write empty shaderpack");

    let pack_empty = RurgeHorizonHooks::query_active_shaderpack_in(temp_dir.path());
    assert_eq!(pack_empty, None);

    // Non-existent dir
    let non_existent = temp_dir.path().join("sub");
    assert_eq!(RurgeHorizonHooks::query_active_shaderpack_in(&non_existent), None);
}

#[test]
fn test_arc_engine_shutdown() {
    let engine = Arc::new(RurgeEngine::new());
    assert!(!engine.is_initialized());

    // Shutdown can be invoked directly on Arc<RurgeEngine> without panic
    engine.shutdown();
    assert!(!engine.is_initialized());
}

#[test]
fn test_rurge_horizon_mods_and_settings_population() {
    let engine = Arc::new(RurgeEngine::new());
    let hooks = RurgeHorizonHooks::new(engine);

    let mut mods = Vec::new();
    hooks.populate_native_mods(&mut mods);

    assert_eq!(mods.len(), 2, "Expected core loader and shaders entries");
    assert_eq!(mods[0].id, "rurge_core");
    assert!(mods[0].title.contains("Rurge Mod Loader"));
    assert_eq!(mods[1].id, "rurge_shaders");
    assert!(mods[1].title.contains("Shaders"));

    let mut core_buttons = Vec::new();
    hooks.populate_settings("rurge_core", &mut core_buttons);
    assert!(core_buttons.iter().any(|b| b.contains("Open Mods Folder")));
    assert!(core_buttons.iter().any(|b| b.contains("Open Shaderpacks Folder")));

    let mut shader_buttons = Vec::new();
    hooks.populate_settings("rurge_shaders", &mut shader_buttons);
    assert!(shader_buttons.iter().any(|b| b.contains("Active:")));
    assert!(shader_buttons.iter().any(|b| b.contains("Open Shaderpacks Folder")));
}

#[test]
fn test_rurge_horizon_command_handling() {
    let engine = Arc::new(RurgeEngine::new());
    let hooks = RurgeHorizonHooks::new(engine);

    // /rurge command
    let res = hooks.handle_command_str("rurge");
    assert!(res.is_some());
    let (msg, flow) = res.unwrap();
    assert!(msg.contains("Rurge"));
    assert!(matches!(flow, horizon_client::mod_api::Flow::Cancel));

    // /rurge help command
    let res_help = hooks.handle_command_str("rurge help");
    assert!(res_help.is_some());

    // Unknown command
    let res_unknown = hooks.handle_command_str("other_cmd");
    assert!(res_unknown.is_none());
}

#[test]
fn test_rurge_horizon_toggle_default_shaderpack_in() {
    let temp_dir = tempfile::tempdir().expect("tempdir");
    let engine = Arc::new(RurgeEngine::new());
    let hooks = RurgeHorizonHooks::new(engine.clone());

    // Initial state: no graphics file
    assert_eq!(RurgeHorizonHooks::query_active_shaderpack_in(temp_dir.path()), None);

    // Toggle 1: should activate test-classic-shader.zip
    hooks.toggle_default_shaderpack_in(temp_dir.path());
    assert_eq!(
        RurgeHorizonHooks::query_active_shaderpack_in(temp_dir.path()).as_deref(),
        Some("test-classic-shader.zip")
    );
    assert_eq!(hooks.current_shader().as_deref(), Some("test-classic-shader.zip"));

    // Toggle 2: should deactivate
    hooks.toggle_default_shaderpack_in(temp_dir.path());
    assert_eq!(RurgeHorizonHooks::query_active_shaderpack_in(temp_dir.path()), None);
    assert_eq!(hooks.current_shader(), None);
}

