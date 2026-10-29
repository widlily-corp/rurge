//! `rurge-core` contains the core runtime infrastructure for the Rurge mod loader.
//!
//! Provides a prioritized type-safe event bus and content registries.

pub mod event_bus;
pub mod manifest;
pub mod registry;
pub mod resolver;

pub use event_bus::{CancellableEvent, EventBus, EventPriority, SubscriptionId};
pub use manifest::{ManifestError, ModEnvironment, ModManifest};
pub use registry::{Identifier, ParseIdentifierError, Registry, RegistryError};
pub use resolver::{DependencyResolver, ResolverError};
