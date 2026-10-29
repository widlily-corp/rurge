use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use abi_stable::std_types::RString;
use rurge_api::{export_rurge_plugin, ModMetadata, RurgeContext, RurgePlugin};

/// Monotonically increasing client tick count recorded by the HUD mod.
pub static TICK_COUNT: AtomicU64 = AtomicU64::new(0);

/// Flag indicating whether the mod hook should intentionally panic (for diagnostics/blamer verification).
pub static INTENTIONAL_PANIC: AtomicBool = AtomicBool::new(false);

/// Flag indicating whether `on_initialize` was invoked.
pub static INITIALIZED: AtomicBool = AtomicBool::new(false);

/// Flag indicating whether `on_shutdown` was invoked.
pub static SHUTDOWN: AtomicBool = AtomicBool::new(false);

/// Three-dimensional player position coordinates for HUD display.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PlayerCoords {
    pub x: f64,
    pub y: f64,
    pub z: f64,
}

impl Default for PlayerCoords {
    fn default() -> Self {
        Self {
            x: 0.0,
            y: 64.0,
            z: 0.0,
        }
    }
}

/// Simple HUD Mod implementing the `RurgePlugin` interface.
///
/// Tracks player coordinates, simulates HUD overlay rendering, and demonstrates
/// Rurge's panic boundary containment.
#[derive(Default)]
pub struct SimpleHudMod {
    initialized: bool,
}

impl SimpleHudMod {
    /// Creates a new `SimpleHudMod` instance.
    pub fn new() -> Self {
        Self { initialized: false }
    }

    /// Simulates a single client tick.
    ///
    /// Increments internal tick counter. If `INTENTIONAL_PANIC` is set,
    /// panics immediately to verify blame attribution without crashing the engine.
    pub fn on_tick(tick: u64) {
        if INTENTIONAL_PANIC.load(Ordering::SeqCst) {
            panic!("Intentional panic in simple_hud at tick {tick}!");
        }
        TICK_COUNT.store(tick, Ordering::SeqCst);
    }

    /// Formats coordinates and current tick for HUD overlay text.
    pub fn render_coordinates(coords: PlayerCoords) -> String {
        format!(
            "XYZ: {:.2} / {:.2} / {:.2} | Tick: {}",
            coords.x,
            coords.y,
            coords.z,
            TICK_COUNT.load(Ordering::SeqCst)
        )
    }

    /// Configures the intentional panic flag.
    pub fn set_panic(should_panic: bool) {
        INTENTIONAL_PANIC.store(should_panic, Ordering::SeqCst);
    }

    /// Returns the last recorded tick count.
    pub fn get_ticks() -> u64 {
        TICK_COUNT.load(Ordering::SeqCst)
    }

    /// Returns whether the plugin has been initialized.
    pub fn is_initialized() -> bool {
        INITIALIZED.load(Ordering::SeqCst)
    }

    /// Returns whether the plugin has received shutdown.
    pub fn is_shutdown() -> bool {
        SHUTDOWN.load(Ordering::SeqCst)
    }
}

impl RurgePlugin for SimpleHudMod {
    fn metadata(&self) -> ModMetadata {
        ModMetadata::new(
            "simple_hud",
            "Simple HUD Mod",
            "0.1.0",
            "HUD overlay and coordinates tracking mod for Horizon",
            vec![RString::from("Rurge Team")],
        )
    }

    fn on_initialize(&mut self, ctx: &mut RurgeContext) {
        self.initialized = true;
        INITIALIZED.store(true, Ordering::SeqCst);
        println!(
            "[simple_hud] Mod initialized successfully (engine: {}, game: {})",
            ctx.engine_version, ctx.game_version
        );
    }

    fn on_shutdown(&mut self) {
        self.initialized = false;
        SHUTDOWN.store(true, Ordering::SeqCst);
        println!("[simple_hud] Mod shutdown successfully");
    }
}

// Export the plugin C ABI factory symbol for dynamic loading by `rurge-loader`
export_rurge_plugin!(SimpleHudMod);

// Optional C-ABI symbol hooks for direct dylib interop
#[unsafe(no_mangle)]
pub extern "C" fn simple_hud_tick(tick: u64) {
    SimpleHudMod::on_tick(tick);
}

#[unsafe(no_mangle)]
pub extern "C" fn simple_hud_set_panic(should_panic: bool) {
    SimpleHudMod::set_panic(should_panic);
}

#[unsafe(no_mangle)]
pub extern "C" fn simple_hud_get_ticks() -> u64 {
    SimpleHudMod::get_ticks()
}
