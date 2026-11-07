//! Core engine library for the standalone multi-core Rurge client.

pub mod governor;
pub mod hardware;
pub mod memory;
pub mod render_state;
pub mod sync;
pub mod thread_render;
pub mod thread_tick;

pub fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}
