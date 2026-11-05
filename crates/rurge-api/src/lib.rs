//! `rurge-api` defines the stable ABI plugin contract for the Rurge native mod loader.
//!
//! Dynamic library mods link against this crate using `abi_stable`.

#![allow(non_local_definitions)]

pub mod block_entity;
pub mod context;
pub mod events;
pub mod metadata;
pub mod plugin;

pub use abi_stable;
pub use block_entity::{BlockEntity, BlockEntityTickContext, BlockPos, NbtCompound, NbtTag};
pub use context::RurgeContext;
pub use events::*;
pub use metadata::ModMetadata;
pub use plugin::{RurgePlugin, RurgePlugin_TO};
