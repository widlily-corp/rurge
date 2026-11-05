use std::any::Any;
use std::sync::{Arc, RwLock};
use abi_stable::{
    std_types::RString,
    StableAbi,
};
use serde::{Deserialize, Serialize};

static CENTRAL_EVENT_BUS: RwLock<Option<Arc<dyn Any + Send + Sync>>> = RwLock::new(None);

/// Context passed to a mod during initialization and lifecycle phases.
///
/// Wraps engine state, versions, and paths across the C ABI boundary.
#[repr(C)]
#[derive(StableAbi, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RurgeContext {
    /// Rurge mod loader version (e.g. "0.1.0").
    pub engine_version: RString,
    /// Horizon game version (e.g. "1.20.4" or "26.3").
    pub game_version: RString,
    /// Absolute or relative path to the mods directory.
    pub mod_dir: RString,
    /// Absolute or relative path to the configuration directory.
    pub config_dir: RString,
}

impl RurgeContext {
    /// Creates a new `RurgeContext`.
    pub fn new(
        engine_version: impl Into<RString>,
        game_version: impl Into<RString>,
        mod_dir: impl Into<RString>,
        config_dir: impl Into<RString>,
    ) -> Self {
        Self {
            engine_version: engine_version.into(),
            game_version: game_version.into(),
            mod_dir: mod_dir.into(),
            config_dir: config_dir.into(),
        }
    }

    /// Sets the central shared event bus instance.
    pub fn set_event_bus<T: 'static + Send + Sync>(bus: Arc<T>) {
        if let Ok(mut lock) = CENTRAL_EVENT_BUS.write() {
            *lock = Some(bus);
        }
    }

    /// Clears the central shared event bus instance.
    pub fn clear_event_bus() {
        if let Ok(mut lock) = CENTRAL_EVENT_BUS.write() {
            *lock = None;
        }
    }

    /// Returns the central shared event bus instance downcasted to `Arc<T>`.
    pub fn event_bus<T: 'static + Send + Sync>() -> Option<Arc<T>> {
        if let Ok(lock) = CENTRAL_EVENT_BUS.read() {
            lock.as_ref().and_then(|b| b.clone().downcast::<T>().ok())
        } else {
            None
        }
    }

    /// Retrieves the central shared event bus from an instance of `RurgeContext`.
    pub fn get_event_bus<T: 'static + Send + Sync>(&self) -> Option<Arc<T>> {
        Self::event_bus()
    }
}
