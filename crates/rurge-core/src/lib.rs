//! `rurge-core` contains the core runtime infrastructure for the Rurge mod loader.
//!
//! Provides a prioritized type-safe event bus and content registries.

pub mod event_bus;
pub mod registry;

pub use event_bus::{CancellableEvent, EventBus, EventPriority, SubscriptionId};
pub use registry::{Identifier, ParseIdentifierError, Registry, RegistryError};
