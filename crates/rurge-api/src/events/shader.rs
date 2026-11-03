use abi_stable::{
    std_types::RString,
    StableAbi,
};

/// Event fired when a shaderpack has been loaded and initialized.
#[repr(C)]
#[derive(Clone, Debug, PartialEq, StableAbi)]
pub struct ShaderpackLoadedEvent {
    pub pack_name: RString,
    pub is_classic: bool,
}

impl ShaderpackLoadedEvent {
    pub fn new(pack_name: impl Into<RString>, is_classic: bool) -> Self {
        Self {
            pack_name: pack_name.into(),
            is_classic,
        }
    }
}

/// Event fired when the active shaderpack has been unloaded.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, StableAbi)]
pub struct ShaderpackUnloadedEvent;

/// Frame render pass context passed to plugin render callbacks and shader pipelines across FFI.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, StableAbi)]
pub struct RenderPassContext {
    pub viewport_width: u32,
    pub viewport_height: u32,
    pub partial_tick: f32,
    pub camera_pos: [f64; 3],
    pub view_matrix: [f32; 16],
}

impl RenderPassContext {
    pub fn new(
        viewport_width: u32,
        viewport_height: u32,
        partial_tick: f32,
        camera_pos: [f64; 3],
        view_matrix: [f32; 16],
    ) -> Self {
        Self {
            viewport_width,
            viewport_height,
            partial_tick,
            camera_pos,
            view_matrix,
        }
    }
}
