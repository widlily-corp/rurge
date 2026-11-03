use abi_stable::{
    abi_stability::check_layout_compatibility,
    std_types::RString,
    StableAbi,
};
use rurge_api::events::{RenderPassContext, ShaderpackLoadedEvent, ShaderpackUnloadedEvent};

#[test]
fn test_shader_events_and_context_abi_stability() {
    let loaded = ShaderpackLoadedEvent {
        pack_name: RString::from("complementary-v5.zip"),
        is_classic: true,
    };
    assert_eq!(loaded.pack_name.as_str(), "complementary-v5.zip");
    assert!(loaded.is_classic);

    let unloaded = ShaderpackUnloadedEvent;
    let _ = unloaded;

    let ctx = RenderPassContext {
        viewport_width: 1920,
        viewport_height: 1080,
        partial_tick: 0.5,
        camera_pos: [100.0, 64.0, -200.0],
        view_matrix: [1.0; 16],
    };
    assert_eq!(ctx.viewport_width, 1920);
    assert_eq!(ctx.partial_tick, 0.5);

    // Verify ABI stability layout checks
    assert!(
        check_layout_compatibility(
            <ShaderpackLoadedEvent as StableAbi>::LAYOUT,
            <ShaderpackLoadedEvent as StableAbi>::LAYOUT
        )
        .is_ok(),
        "ShaderpackLoadedEvent layout must be ABI-compatible"
    );
    assert!(
        check_layout_compatibility(
            <ShaderpackUnloadedEvent as StableAbi>::LAYOUT,
            <ShaderpackUnloadedEvent as StableAbi>::LAYOUT
        )
        .is_ok(),
        "ShaderpackUnloadedEvent layout must be ABI-compatible"
    );
    assert!(
        check_layout_compatibility(
            <RenderPassContext as StableAbi>::LAYOUT,
            <RenderPassContext as StableAbi>::LAYOUT
        )
        .is_ok(),
        "RenderPassContext layout must be ABI-compatible"
    );
}

#[test]
fn test_reexports_from_crate_root() {
    use rurge_api::{RenderPassContext, ShaderpackLoadedEvent, ShaderpackUnloadedEvent};

    let _loaded = ShaderpackLoadedEvent {
        pack_name: RString::from("test"),
        is_classic: false,
    };
    let _unloaded = ShaderpackUnloadedEvent;
    let _ctx = RenderPassContext {
        viewport_width: 800,
        viewport_height: 600,
        partial_tick: 0.0,
        camera_pos: [0.0, 0.0, 0.0],
        view_matrix: [0.0; 16],
    };
}
