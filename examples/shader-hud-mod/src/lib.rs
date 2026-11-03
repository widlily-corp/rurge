use std::sync::RwLock;
use abi_stable::std_types::{ROption, RString};
use log::info;
use rurge_api::events::{RenderPassContext, ShaderpackLoadedEvent, ShaderpackUnloadedEvent};
use rurge_api::{export_rurge_plugin, ModMetadata, RurgeContext, RurgePlugin};

/// Active shaderpack name stored across mod lifecycle.
static ACTIVE_SHADER: RwLock<Option<String>> = RwLock::new(None);

/// Returns the currently active shaderpack name, or None if vanilla/none is active.
pub fn active_shader() -> Option<String> {
    ACTIVE_SHADER.read().ok().and_then(|lock| lock.clone())
}

/// Sets the active shaderpack name in thread-safe static state.
pub fn set_active_shader(name: Option<String>) {
    if let Ok(mut lock) = ACTIVE_SHADER.write() {
        *lock = name;
    }
}

/// Shader-aware HUD mod implementing the `RurgePlugin` stable ABI contract.
#[derive(Default)]
pub struct ShaderHudMod {
    initialized: bool,
}

impl ShaderHudMod {
    /// Creates a new `ShaderHudMod` instance.
    pub fn new() -> Self {
        Self { initialized: false }
    }

    /// Returns the currently active shaderpack name.
    pub fn active_shader() -> Option<String> {
        active_shader()
    }

    /// Event listener for `ShaderpackLoadedEvent`.
    pub fn on_shaderpack_loaded(event: &ShaderpackLoadedEvent) {
        let pack_name = event.pack_name.as_str().to_string();
        info!("Shaderpack active: {pack_name}");
        set_active_shader(Some(pack_name));
    }

    /// Event listener for `ShaderpackUnloadedEvent`.
    pub fn on_shaderpack_unloaded(_event: &ShaderpackUnloadedEvent) {
        info!("Shaderpack unloaded");
        set_active_shader(None);
    }

    /// Lifecycle hook called on initialization to set up shaderpack event observers.
    pub fn on_load() {
        info!("[shader-hud-mod] Setting up shaderpack lifecycle observers");
    }

    /// Renders or formats HUD overlay text given the render pass context.
    pub fn on_world_drawn(ctx: &RenderPassContext) -> String {
        let shader = active_shader().unwrap_or_else(|| "Vanilla (No Shaders)".to_string());
        format!(
            "Active Shader: {} | Viewport: {}x{} | Cam: [{:.1}, {:.1}, {:.1}]",
            shader, ctx.viewport_width, ctx.viewport_height,
            ctx.camera_pos[0], ctx.camera_pos[1], ctx.camera_pos[2]
        )
    }

    /// Returns whether this mod has been initialized.
    pub fn is_initialized(&self) -> bool {
        self.initialized
    }
}

impl RurgePlugin for ShaderHudMod {
    fn metadata(&self) -> ModMetadata {
        ModMetadata::new(
            "shader-hud-mod",
            "Shader Aware HUD Mod",
            "0.1.0",
            "An example Rurge mod demonstrating shaderpack lifecycle observation and render hooks",
            vec![RString::from("Rurge Team")],
        )
    }

    fn on_initialize(&mut self, ctx: &mut RurgeContext) {
        self.initialized = true;
        Self::on_load();
        info!(
            "[shader-hud-mod] Initialized under Rurge {} (game: {})",
            ctx.engine_version, ctx.game_version
        );
    }

    fn on_shutdown(&mut self) {
        self.initialized = false;
        set_active_shader(None);
        info!("[shader-hud-mod] Shutdown successfully");
    }
}

// Export the plugin C ABI factory symbol for dynamic loading by `rurge-loader`
export_rurge_plugin!(ShaderHudMod);

#[unsafe(no_mangle)]
pub extern "C" fn shader_hud_active_shader() -> ROption<RString> {
    active_shader().map(RString::from).into()
}
