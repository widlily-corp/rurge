use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use parking_lot::RwLock;
use rurge_api::block_entity::{BlockEntity, BlockPos};
use rurge_api::events::RenderPassContext;
use rurge_api::lifecycle::{InitEvent, PreInitEvent, ServerStoppingEvent};
use rurge_api::RurgeContext;
use rurge_blamer::{safe_invoke, Blamer, PanicError};
use rurge_core::EventBus;
use rurge_loader::{LoaderError, ModManager};

use crate::block_entity::BlockEntityManager;
use crate::content_schema::ContentSchemaExecutor;
use crate::hooks::{ClientTickEvent, ServerTickEvent, TickHooks};
use crate::render::RenderPipelineHooks;
use crate::scripting::MechanicsExecutor;

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
    mod_manager: RwLock<ModManager>,
    content: Arc<ContentSchemaExecutor>,
    render_hooks: RenderPipelineHooks,
    tick_hooks: TickHooks,
    block_entities: Arc<BlockEntityManager>,
    mechanics: Arc<MechanicsExecutor>,
    initialized: AtomicBool,
    shader_uniforms: RwLock<[[f32; 4]; 8]>,
}

impl RurgeEngine {
    /// Creates a new default uninitialized `RurgeEngine` pointing to standard "mods" and ".cache" directories.
    pub fn new() -> Self {
        Self::with_dirs("mods", ".cache")
    }

    /// Creates a new uninitialized `RurgeEngine` pointing to the specified mods and cache directories.
    pub fn with_dirs(mods_dir: impl Into<PathBuf>, cache_dir: impl Into<PathBuf>) -> Self {
        let blamer = Arc::new(Blamer::new());
        let event_bus = Arc::new(EventBus::new());
        let mod_manager = RwLock::new(ModManager::new(Arc::clone(&blamer)));
        let content = Arc::new(ContentSchemaExecutor::new());
        let render_hooks = RenderPipelineHooks::new();
        let tick_hooks = TickHooks::new();
        let block_entities = Arc::new(BlockEntityManager::new());
        let mechanics = Arc::new(MechanicsExecutor::new());

        Self {
            mods_dir: mods_dir.into(),
            cache_dir: cache_dir.into(),
            blamer,
            event_bus,
            mod_manager,
            content,
            render_hooks,
            tick_hooks,
            block_entities,
            mechanics,
            initialized: AtomicBool::new(false),
            shader_uniforms: RwLock::new([[0.0f32; 4]; 8]),
        }
    }

    /// Convenience bootstrap method initializing an engine in the given directory.
    pub fn bootstrap(mods_dir: impl Into<PathBuf>) -> Result<Self, RuntimeError> {
        let mods_dir = mods_dir.into();
        let cache_dir = mods_dir.join(".cache");
        let mut engine = Self::with_dirs(mods_dir, cache_dir);
        engine.initialize()?;
        Ok(engine)
    }

    /// Initializes the engine, discovers and loads mods, invokes plugin `on_initialize`,
    /// and fires lifecycle events (`PreInitEvent` and `InitEvent`).
    pub fn initialize(&mut self) -> Result<(), RuntimeError> {
        if self.initialized.load(Ordering::SeqCst) {
            return Err(RuntimeError::AlreadyInitialized);
        }

        // 1. Ensure directories exist and discover/resolve/load mods.
        if !self.mods_dir.exists() {
            std::fs::create_dir_all(&self.mods_dir)?;
        }
        if !self.cache_dir.exists() {
            std::fs::create_dir_all(&self.cache_dir)?;
        }

        self.mod_manager.write().load_mods(&self.mods_dir, &self.cache_dir)?;

        // Discover and register declarative content.json from all .rmod archives in mods_dir
        if let Ok(archives) = self.mod_manager.read().discover_archives(&self.mods_dir) {
            for archive in &archives {
                if let Ok(Some(schema_str)) = archive.read_content_schema() {
                    match rurge_converter::ContentSchema::from_json_str(&schema_str) {
                        Ok(schema) => {
                            if let Err(e) = self.content.register_schema(&schema.mod_id, &schema) {
                                log::warn!(
                                    "Failed to register content schema for mod '{}' in '{}': {e}",
                                    schema.mod_id,
                                    archive.path().display()
                                );
                            } else {
                                log::info!(
                                    "Registered content schema for mod '{}': {} blocks, {} items, {} recipes",
                                    schema.mod_id,
                                    schema.blocks.len(),
                                    schema.items.len(),
                                    schema.recipes.len()
                                );
                            }
                        }
                        Err(e) => {
                            log::warn!(
                                "Failed to parse content.json in '{}': {e}",
                                archive.path().display()
                            );
                        }
                    }
                }
            }
        }

        // 2. Create RurgeContext.
        let ctx = RurgeContext::new(
            env!("CARGO_PKG_VERSION"),
            "26.3",
            self.mods_dir.to_string_lossy().to_string(),
            self.cache_dir.to_string_lossy().to_string(),
        );

        let loaded_plugins = self.mod_manager.read().loaded_plugins();

        // 3. Invoke `on_initialize` on all loaded plugins inside `safe_invoke`.
        for plugin_arc in &loaded_plugins {
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
        for plugin in &loaded_plugins {
            let mut mod_pre_init = PreInitEvent::new(plugin.manifest.id.as_str());
            self.event_bus.dispatch(&mut mod_pre_init);
        }

        let mut init = InitEvent::new("rurge");
        self.event_bus.dispatch(&mut init);
        for plugin in &loaded_plugins {
            let mut mod_init = InitEvent::new(plugin.manifest.id.as_str());
            self.event_bus.dispatch(&mut mod_init);
        }

        self.initialized.store(true, Ordering::SeqCst);
        Ok(())
    }

    /// Returns whether the engine has completed its initialization phase.
    pub fn is_initialized(&self) -> bool {
        self.initialized.load(Ordering::SeqCst)
    }

    /// Dispatches client tick event through `event_bus` and executes registered tick handlers.
    pub fn client_tick(&self, tick_count: u64) {
        let mut event = ClientTickEvent { tick_count };
        self.event_bus.dispatch(&mut event);
        self.tick_hooks.execute_client_ticks(tick_count, &self.blamer);
    }

    /// Dispatches server tick event through `event_bus`, executes 20 TPS block entity world ticks
    /// within safe panic boundaries, and executes registered server tick handlers.
    pub fn server_tick(&self, tick_count: u64) {
        let mut event = ServerTickEvent { tick_count };
        self.event_bus.dispatch(&mut event);
        self.block_entities.tick_all(tick_count, &self.blamer);
        self.tick_hooks.execute_server_ticks(tick_count, &self.blamer);
    }

    /// Shuts down the engine, firing `ServerStoppingEvent` and invoking `on_shutdown` on all plugins.
    pub fn shutdown(&self) {
        if !self.initialized.swap(false, Ordering::SeqCst) {
            return;
        }
        let mut stopping = ServerStoppingEvent;
        self.event_bus.dispatch(&mut stopping);
        self.mod_manager.write().shutdown_all();
    }

    /// Returns a reference to the shared event bus.
    pub fn event_bus(&self) -> &Arc<EventBus> {
        &self.event_bus
    }

    /// Returns a reference to the crash diagnostics blamer.
    pub fn blamer(&self) -> &Arc<Blamer> {
        &self.blamer
    }

    /// Returns a shared reference to the declarative content schema executor.
    pub fn content(&self) -> Arc<ContentSchemaExecutor> {
        Arc::clone(&self.content)
    }

    /// Returns a reference to the render pipeline hooks.
    pub fn render_hooks(&self) -> &RenderPipelineHooks {
        &self.render_hooks
    }

    /// Returns a mutable reference to the render pipeline hooks.
    pub fn render_hooks_mut(&mut self) -> &mut RenderPipelineHooks {
        &mut self.render_hooks
    }

    /// Returns a read guard to the mod manager.
    pub fn mod_manager(&self) -> parking_lot::RwLockReadGuard<'_, ModManager> {
        self.mod_manager.read()
    }

    /// Returns a mutable reference to the mod manager.
    pub fn mod_manager_mut(&mut self) -> &mut ModManager {
        self.mod_manager.get_mut()
    }

    /// Sets the custom shader uniform vector at the specified index (`0..8`).
    pub fn set_shader_uniform(&self, slot: usize, value: [f32; 4]) {
        if slot < 8 {
            let mut uniforms = self.shader_uniforms.write();
            uniforms[slot] = value;
        }
    }

    /// Returns the current custom shader uniform vectors (`8` vectors of `[f32; 4]`).
    pub fn get_shader_uniforms(&self) -> [[f32; 4]; 8] {
        *self.shader_uniforms.read()
    }

    /// Returns a reference to the tick hooks registry.
    pub fn tick_hooks(&self) -> &TickHooks {
        &self.tick_hooks
    }

    /// Returns a mutable reference to the tick hooks registry.
    pub fn tick_hooks_mut(&mut self) -> &mut TickHooks {
        &mut self.tick_hooks
    }

    /// Returns a reference to the block entity manager.
    pub fn block_entities(&self) -> &Arc<BlockEntityManager> {
        &self.block_entities
    }

    /// Returns a reference to the mechanics scripting executor.
    pub fn mechanics(&self) -> &Arc<MechanicsExecutor> {
        &self.mechanics
    }

    /// Convenience helper to register a block entity at the given coordinates.
    pub fn register_block_entity<T: BlockEntity + 'static>(
        &self,
        mod_id: impl Into<String>,
        pos: BlockPos,
        entity: T,
    ) {
        self.block_entities.register(mod_id, pos, entity);
    }

    /// Explicitly executes a 20 TPS world tick cycle for all registered block entities.
    pub fn tick_world(&self, tick_count: u64) {
        self.block_entities.tick_all(tick_count, &self.blamer);
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

    /// Convenience helper to register a world-drawn render hook.
    pub fn register_world_drawn_hook(
        &self,
        mod_id: impl Into<String>,
        hook: Box<dyn Fn(&RenderPassContext) + Send + Sync>,
    ) {
        self.render_hooks.register_world_drawn(mod_id, hook);
    }

    /// Convenience helper to register a hand-drawn render hook.
    pub fn register_hand_drawn_hook(
        &self,
        mod_id: impl Into<String>,
        hook: Box<dyn Fn(&RenderPassContext) + Send + Sync>,
    ) {
        self.render_hooks.register_hand_drawn(mod_id, hook);
    }

    /// Returns whether any world-drawn render hooks are currently registered.
    pub fn has_world_drawn_hooks(&self) -> bool {
        self.render_hooks.has_world_drawn_hooks()
    }

    /// Returns whether any hand-drawn render hooks are currently registered.
    pub fn has_hand_drawn_hooks(&self) -> bool {
        self.render_hooks.has_hand_drawn_hooks()
    }

    /// Executes all registered world-drawn hooks within safe panic boundaries.
    /// Returns `true` if all hooks executed successfully, or `false` if any hook panicked.
    pub fn execute_world_drawn(&self, ctx: &RenderPassContext) -> bool {
        self.render_hooks.execute_world_drawn(ctx, &self.blamer)
    }

    /// Executes all registered hand-drawn hooks within safe panic boundaries.
    /// Returns `true` if all hooks executed successfully, or `false` if any hook panicked.
    pub fn execute_hand_drawn(&self, ctx: &RenderPassContext) -> bool {
        self.render_hooks.execute_hand_drawn(ctx, &self.blamer)
    }
}

impl Default for RurgeEngine {
    fn default() -> Self {
        Self::new()
    }
}
