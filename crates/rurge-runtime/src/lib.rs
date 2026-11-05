//! `rurge-runtime` — Horizon Game Loop Hooks & WGPU Render Pass Injection.
//!
//! Provides the primary orchestrator binary (`RurgeEngine`), safe game loop tick
//! dispatch, crash-resilient graphics pipeline pass injection, NBT-isolated
//! mod persistence, and non-JVM mod mechanics sandbox (`MechanicsExecutor`).

pub mod content_schema;
pub mod engine;
pub mod hooks;
pub mod persistence;
pub mod render;
pub mod scripting;
pub use render as render_hooks;

pub use content_schema::ContentSchemaExecutor;
pub use engine::{RurgeEngine, RuntimeError};
pub use hooks::{ClientTickEvent, GameLoopHooks, ServerTickEvent, TickHooks};
pub use persistence::{NbtCompound, PersistenceError, WorldDataAttachment};
pub use render::{RenderPassContext, RenderPassPhase, RenderPipelineHooks};
pub use scripting::{
    ActiveEffect, BlockState, ExecutionContext, ExecutionResult, MechanicsEvent, MechanicsExecutor,
    PlayerState,
};
