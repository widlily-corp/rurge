use std::sync::Arc;
use rurge_blamer::{safe_invoke, Blamer};

/// Event fired during client game loop tick.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ClientTickEvent {
    pub tick_count: u64,
}

/// Event fired during server game loop tick.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ServerTickEvent {
    pub tick_count: u64,
}

type TickFn = Arc<dyn Fn(u64) + Send + Sync + 'static>;

#[derive(Clone)]
struct TickHandler {
    mod_id: String,
    handler: TickFn,
}

/// Collection of client and server tick hooks registered by mods.
#[derive(Default)]
pub struct TickHooks {
    client_handlers: Vec<TickHandler>,
    server_handlers: Vec<TickHandler>,
}

impl TickHooks {
    /// Creates a new empty `TickHooks` registry.
    pub fn new() -> Self {
        Self::default()
    }

    /// Registers a client tick hook callback attributed to `mod_id`.
    pub fn register_client_tick<F>(&mut self, mod_id: impl Into<String>, handler: F)
    where
        F: Fn(u64) + Send + Sync + 'static,
    {
        self.client_handlers.push(TickHandler {
            mod_id: mod_id.into(),
            handler: Arc::new(handler),
        });
    }

    /// Registers a server tick hook callback attributed to `mod_id`.
    pub fn register_server_tick<F>(&mut self, mod_id: impl Into<String>, handler: F)
    where
        F: Fn(u64) + Send + Sync + 'static,
    {
        self.server_handlers.push(TickHandler {
            mod_id: mod_id.into(),
            handler: Arc::new(handler),
        });
    }

    /// Executes all client tick hooks within safe panic boundaries.
    pub fn execute_client_ticks(&self, tick_count: u64, blamer: &Blamer) {
        for entry in &self.client_handlers {
            let handler = Arc::clone(&entry.handler);
            let _ = safe_invoke(
                &entry.mod_id,
                "client_tick",
                blamer,
                std::panic::AssertUnwindSafe(move || {
                    handler(tick_count);
                }),
            );
        }
    }

    /// Executes all server tick hooks within safe panic boundaries.
    pub fn execute_server_ticks(&self, tick_count: u64, blamer: &Blamer) {
        for entry in &self.server_handlers {
            let handler = Arc::clone(&entry.handler);
            let _ = safe_invoke(
                &entry.mod_id,
                "server_tick",
                blamer,
                std::panic::AssertUnwindSafe(move || {
                    handler(tick_count);
                }),
            );
        }
    }
}

/// Alias for `TickHooks`.
pub type GameLoopHooks = TickHooks;
