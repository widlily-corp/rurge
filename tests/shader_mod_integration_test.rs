use rurge_runtime::engine::RurgeEngine;
use rurge_api::events::{ShaderpackLoadedEvent, ShaderpackUnloadedEvent, RenderPassContext};
use abi_stable::std_types::RString;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

#[test]
fn test_shader_aware_mod_event_dispatch() {
    let engine = RurgeEngine::new();
    let event_bus = engine.event_bus();

    let loaded_flag = Arc::new(AtomicBool::new(false));
    let loaded_flag_clone = loaded_flag.clone();

    event_bus.register(0, move |event: &ShaderpackLoadedEvent| {
        assert_eq!(event.pack_name.as_str(), "test-classic-shader.zip");
        assert!(event.is_classic);
        loaded_flag_clone.store(true, Ordering::Relaxed);
    });

    let unloaded_flag = Arc::new(AtomicBool::new(false));
    let unloaded_flag_clone = unloaded_flag.clone();

    event_bus.register(0, move |_event: &ShaderpackUnloadedEvent| {
        unloaded_flag_clone.store(true, Ordering::Relaxed);
    });

    // Dispatch loaded
    event_bus.dispatch(&ShaderpackLoadedEvent {
        pack_name: RString::from("test-classic-shader.zip"),
        is_classic: true,
    });
    assert!(loaded_flag.load(Ordering::Relaxed), "ShaderpackLoadedEvent must be handled");

    // Dispatch unloaded
    event_bus.dispatch(&ShaderpackUnloadedEvent);
    assert!(unloaded_flag.load(Ordering::Relaxed), "ShaderpackUnloadedEvent must be handled");

    // Verify render hook integration
    let render_executed = Arc::new(AtomicBool::new(false));
    let render_clone = render_executed.clone();
    engine.register_world_drawn_hook("shader-hud-mod", Box::new(move |ctx| {
        assert_eq!(ctx.viewport_width, 1920);
        render_clone.store(true, Ordering::Relaxed);
    }));

    let ctx = RenderPassContext {
        viewport_width: 1920,
        viewport_height: 1080,
        partial_tick: 0.0,
        camera_pos: [0.0, 64.0, 0.0],
        view_matrix: [1.0; 16],
    };
    engine.execute_world_drawn(&ctx);
    assert!(render_executed.load(Ordering::Relaxed));
}

#[test]
fn test_shader_hud_mod_event_handling_and_active_shader() {
    use shader_hud_mod::{active_shader, ShaderHudMod};
    use rurge_api::RurgePlugin;

    let plugin = ShaderHudMod::new();
    let meta = plugin.metadata();
    assert_eq!(meta.id.as_str(), "shader-hud-mod");
    assert_eq!(meta.name.as_str(), "Shader Aware HUD Mod");

    assert_eq!(active_shader(), None);

    // Simulate event handler calls as wired in mod on_load
    ShaderHudMod::on_shaderpack_loaded(&ShaderpackLoadedEvent {
        pack_name: RString::from("test-classic-shader.zip"),
        is_classic: true,
    });
    assert_eq!(active_shader().as_deref(), Some("test-classic-shader.zip"));

    // Verify HUD overlay text formatting
    let ctx = RenderPassContext {
        viewport_width: 1920,
        viewport_height: 1080,
        partial_tick: 0.0,
        camera_pos: [0.0, 64.0, 0.0],
        view_matrix: [1.0; 16],
    };
    let hud_text = ShaderHudMod::on_world_drawn(&ctx);
    assert!(hud_text.contains("test-classic-shader.zip"));
    assert!(hud_text.contains("1920x1080"));

    // Unload shader
    ShaderHudMod::on_shaderpack_unloaded(&ShaderpackUnloadedEvent);
    assert_eq!(active_shader(), None);
}
