//! Core engine library for the standalone multi-core Rurge client.

pub mod branding;
pub mod engine_host;
pub mod governor;
pub mod hardware;
pub mod hooks;
pub mod jar_scanner;
pub mod memory;
pub mod mesh_pipeline;
pub mod render_state;
pub mod runtime_bridge;
pub mod shaderpack_bundle;
pub mod simulation;
pub mod sound_linker;
pub mod sync;
pub mod thread_render;
pub mod thread_tick;

pub fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}
