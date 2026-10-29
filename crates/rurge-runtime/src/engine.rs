use std::path::PathBuf;
use std::sync::Arc;
use rurge_api::lifecycle::{InitEvent, PreInitEvent, ServerStoppingEvent};
use rurge_api::RurgeContext;
use rurge_blamer::{safe_invoke, Blamer, PanicError};
use rurge_core::EventBus;
use rurge_loader::{LoaderError, ModManager};

use crate::hooks::{ClientTickEvent, ServerTickEvent, TickHooks};
use crate::render_hooks::RenderPipelineHooks;

/// Errors that can occur during runtime engine initialization and execution.
#[derive(Debug, thiserror::Error)]
pub enum RuntimeError {
    /// Failure during mod discovery, dependency resolution, or loading.
    #[error("Loader error: {0}")]
    Loader(#[from] LoaderError),

    /// Panic captured within safe execution boundaries.
    #[error("Panic error: {0}")]
    Panic(#[from] PanicError),

    /// File system or I/O failure.
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    /// Attempted to initialize an already initialized engine instance.
    #[error("Engine already initialized")]
    AlreadyInitialized,
}

/// Central engine orchestrator linking Horizon's game loop to loaded native mods.
pub struct RurgeEngine {
    mods_dir: PathBuf,
    cache_dir: PathBuf,
    blamer: Arc<Blamer>,
    event_bus: Arc<EventBus>,
    mod_manager: ModManager,
    render_hooks: RenderPipelineHooks,
    tick_hooks: TickHooks,
    initialized: bool,
}

impl RurgeEngine {
    /// Creates a new uninitialized `RurgeEngine` pointing to the specified mods and cache directories.
    pub fn new(mods_dir: impl Into<PathBuf>, cache_dir: impl Into<PathBuf>) -> Self {
        let blamer = Arc::new(Blamer::new());
        let event_bus = Arc::new(EventBus::new());
        let mod_manager = ModManager::new(Arc::clone(&blamer));
        let render_hooks = RenderPipelineHooks::new();
        let tick_hooks = TickHooks::new();

        Self {
            mods_dir: mods_dir.into(),
            cache_dir: cache_dir.into(),
            blamer,
            event_bus,
            mod_manager,
            render_hooks,
            tick_hooks,
            initialized: false,
        }
    }

    /// Convenience bootstrap method initializing an engine in the given directory.
    pub fn bootstrap(mods_dir: impl Into<PathBuf>) -> Result<Self, RuntimeError> {
        let mods_dir = mods_dir.into();
        let cache_dir = mods_dir.join(".cache");
        let mut engine = Self::new(mods_dir, cache_dir);
        engine.initialize()?;
        Ok(engine)
    }

    /// Initializes the engine, discovers and loads mods, invokes plugin `on_initialize`,
    /// and fires lifecycle events (`PreInitEvent` and `InitEvent`).
    pub fn initialize(&mut self) -> Result<(), RuntimeError> {
        if self.initialized {
            return Err(RuntimeError::AlreadyInitialized);
        }

        // 1. Ensure directories exist and discover/resolve/load mods.
        if !self.mods_dir.exists() {
            std::fs::create_dir_all(&self.mods_dir)?;
        }
        if !self.cache_dir.exists() {
            std::fs::create_dir_all(&self.cache_dir)?;
        }

        self.mod_manager.load_mods(&self.mods_dir, &self.cache_dir)?;

        // 2. Create RurgeContext.
        let ctx = RurgeContext::new(
            env!("CARGO_PKG_VERSION"),
            "26.3",
            self.mods_dir.to_string_lossy().to_string(),
            self.cache_dir.to_string_lossy().to_string(),
        );

        // 3. Invoke `on_initialize` on all loaded plugins inside `safe_invoke`.
        for plugin_arc in self.mod_manager.loaded_plugins() {
            let mod_id = plugin_arc.manifest.id.clone();
            let plugin_clone = Arc::clone(plugin_arc);
            let mut ctx_clone = ctx.clone();

            let _ = safe_invoke(
                &mod_id,
                "on_initialize",
                &self.blamer,
                std::panic::AssertUnwindSafe(move || {
                    let mut plugin_lock = plugin_clone.plugin.lock();
                    plugin_lock.on_initialize(&mut ctx_clone);
                }),
            );
        }

        // 4. Dispatches PreInitEvent and InitEvent.
        let mut pre_init = PreInitEvent::new("rurge");
        self.event_bus.dispatch(&mut pre_init);
        for plugin in self.mod_manager.loaded_plugins() {
            let mut mod_pre_init = PreInitEvent::new(plugin.manifest.id.as_str());
            self.event_bus.dispatch(&mut mod_pre_init);
        }

        let mut init = InitEvent::new("rurge");
        self.event_bus.dispatch(&mut init);
        for plugin in self.mod_manager.loaded_plugins() {
            let mut mod_init = InitEvent::new(plugin.manifest.id.as_str());
            self.event_bus.dispatch(&mut mod_init);
        }

        self.initialized = true;
        Ok(())
    }

    /// Returns whether the engine has completed its initialization phase.
    pub fn is_initialized(&self) -> bool {
        self.initialized
    }

    /// Dispatches client tick event through `event_bus` and executes registered tick handlers.
    pub fn client_tick(&mut self, tick_count: u64) {
        let mut event = ClientTickEvent { tick_count };
        self.event_bus.dispatch(&mut event);
        self.tick_hooks.execute_client_ticks(tick_count, &self.blamer);
    }

    /// Dispatches server tick event through `event_bus` and executes registered server tick handlers.
    pub fn server_tick(&mut self, tick_count: u64) {
        let mut event = ServerTickEvent { tick_count };
        self.event_bus.dispatch(&mut event);
        self.tick_hooks.execute_server_ticks(tick_count, &self.blamer);
    }

    /// Shuts down the engine, firing `ServerStoppingEvent` and invoking `on_shutdown` on all plugins.
    pub fn shutdown(&mut self) {
        let mut stopping = ServerStoppingEvent;
        self.event_bus.dispatch(&mut stopping);
        self.mod_manager.shutdown_all();
        self.initialized = false;
    }

    /// Returns a reference to the shared event bus.
    pub fn event_bus(&self) -> &Arc<EventBus> {
        &self.event_bus
    }

    /// Returns a reference to the crash diagnostics blamer.
    pub fn blamer(&self) -> &Arc<Blamer> {
        &self.blamer
    }

    /// Returns a reference to the render pipeline hooks.
    pub fn render_hooks(&self) -> &RenderPipelineHooks {
        &self.render_hooks
    }

    /// Returns a mutable reference to the render pipeline hooks.
    pub fn render_hooks_mut(&mut self) -> &mut RenderPipelineHooks {
        &mut self.render_hooks
    }

    /// Returns a reference to the mod manager.
    pub fn mod_manager(&self) -> &ModManager {
        &self.mod_manager
    }

    /// Returns a mutable reference to the mod manager.
    pub fn mod_manager_mut(&mut self) -> &mut ModManager {
        &mut self.mod_manager
    }

    /// Returns a reference to the tick hooks registry.
    pub fn tick_hooks(&self) -> &TickHooks {
        &self.tick_hooks
    }

    /// Returns a mutable reference to the tick hooks registry.
    pub fn tick_hooks_mut(&mut self) -> &mut TickHooks {
        &mut self.tick_hooks
    }

    /// Convenience helper to register a client tick handler.
    pub fn register_client_tick<F>(&mut self, mod_id: impl Into<String>, handler: F)
    where
        F: Fn(u64) + Send + Sync + 'static,
    {
        self.tick_hooks.register_client_tick(mod_id, handler);
    }

    /// Convenience helper to register a server tick handler.
    pub fn register_server_tick<F>(&mut self, mod_id: impl Into<String>, handler: F)
    where
        F: Fn(u64) + Send + Sync + 'static,
    {
        self.tick_hooks.register_server_tick(mod_id, handler);
    }
}
