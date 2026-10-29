//! `rurge-runtime` — Horizon Game Loop Hooks & WGPU Render Pass Injection.
//!
//! Provides the primary orchestrator binary (`RurgeEngine`), safe game loop tick
//! dispatch, crash-resilient graphics pipeline pass injection, and NBT-isolated
//! mod persistence.

pub mod engine;
pub mod hooks;
pub mod persistence;
pub mod render_hooks;

pub use engine::{RurgeEngine, RuntimeError};
pub use hooks::{ClientTickEvent, GameLoopHooks, ServerTickEvent, TickHooks};
pub use persistence::{NbtCompound, PersistenceError, WorldDataAttachment};
pub use render_hooks::{RenderPassContext, RenderPassPhase, RenderPipelineHooks};
