use rurge_runtime::{RurgeEngine, RuntimeError};
use std::path::Path;
use std::sync::Arc;

/// Bridges the `rurge-runtime` mod execution engine with the multi-core `rurge-client`.
pub struct RuntimeBridge {
    engine: Arc<RurgeEngine>,
}

impl RuntimeBridge {
    /// Bootstraps the engine by discovering and loading all mods from `mods_dir`.
    pub fn bootstrap(mods_dir: impl AsRef<Path>) -> Result<Self, RuntimeError> {
        let engine = RurgeEngine::bootstrap(mods_dir.as_ref())?;
        Ok(Self {
            engine: Arc::new(engine),
        })
    }

    pub fn is_initialized(&self) -> bool {
        self.engine.is_initialized()
    }

    /// Dispatches server and client tick cycles through the mod engine.
    ///
    /// Executes block entities, scripting mechanics, and mod tick events with panic isolation.
    pub fn on_simulation_tick(&self, tick_count: u64) {
        self.engine.server_tick(tick_count);
        self.engine.client_tick(tick_count);
    }

    pub fn engine(&self) -> &Arc<RurgeEngine> {
        &self.engine
    }

    pub fn shutdown(&self) {
        self.engine.shutdown();
    }
}
