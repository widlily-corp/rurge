//! `rurge-runtime` — Horizon Game Loop Hooks & WGPU Render Pass Injection.
//!
//! Provides the primary orchestrator binary (`RurgeEngine`), safe game loop tick
//! dispatch, crash-resilient graphics pipeline pass injection, NBT-isolated
//! mod persistence, and non-JVM mod mechanics sandbox (`MechanicsExecutor`).

pub mod block_entity;
pub mod content_schema;
pub mod engine;
pub mod hooks;
pub mod persistence;
pub mod render;
pub mod scripting;
pub mod wasm;
pub use render as render_hooks;

pub use block_entity::{
    api_to_runtime_nbt, json_to_nbt_tag, nbt_tag_to_json, runtime_to_api_nbt, BlockEntityEntry,
    BlockEntityFactory, BlockEntityManager, BLOCK_ENTITIES_NBT_KEY,
};
pub use content_schema::ContentSchemaExecutor;
pub use engine::{RurgeEngine, RuntimeError};
pub use hooks::{ClientTickEvent, GameLoopHooks, ServerTickEvent, TickHooks};
pub use persistence::{NbtCompound, PersistenceError, WorldDataAttachment};
pub use render::{RenderPassContext, RenderPassPhase, RenderPipelineHooks};
pub use rurge_api::block_entity::{BlockEntity, BlockEntityTickContext, BlockPos, NbtTag};
pub use scripting::{
    ActiveEffect, BlockState, ExecutionContext, ExecutionResult, MechanicsEvent, MechanicsExecutor,
    PlayerState,
};
