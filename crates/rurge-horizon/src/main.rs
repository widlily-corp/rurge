//! Binary entrypoint for the Rurge Horizon native client.

use std::path::Path;
use std::sync::Arc;

use horizon_client::mod_api::Mods;
use rurge_horizon::hooks::RurgeHorizonHooks;
use rurge_horizon::shaderpack_bundle::ensure_default_shaderpack_installed;
use rurge_runtime::engine::RurgeEngine;

fn main() {
    // 1. Initialize env_logger.
    env_logger::init();

    // 2. Log banner.
    log::info!("Starting Rurge Horizon Native Client...");

    // 3. Ensure "mods" and "shaderpacks" folders exist.
    let mods_dir = Path::new("mods");
    let shaderpacks_dir = Path::new("shaderpacks");

    if !mods_dir.exists()
        && let Err(e) = std::fs::create_dir_all(mods_dir)
    {
        log::warn!("Failed to create mods directory: {e}");
    }

    if !shaderpacks_dir.exists()
        && let Err(e) = std::fs::create_dir_all(shaderpacks_dir)
    {
        log::warn!("Failed to create shaderpacks directory: {e}");
    }

    // 4. Install bundled shaderpack using ensure_default_shaderpack_installed.
    match ensure_default_shaderpack_installed(shaderpacks_dir) {
        Ok(path) => log::info!("Ensured default shaderpack at '{}'", path.display()),
        Err(e) => log::warn!("Failed to install default shaderpack: {e}"),
    }

    // 5. Initialize RurgeEngine.
    let mut engine = RurgeEngine::new();

    // 6. Discover and load .rmod mods from "mods" folder using ModManager (via engine.initialize()).
    if let Err(e) = engine.initialize() {
        log::error!("Failed to initialize RurgeEngine: {e}");
    } else {
        let loaded_count = engine.mod_manager().loaded_plugins().len();
        log::info!("Discovered and loaded {loaded_count} .rmod mod(s)");
    }

    // 7. Construct RurgeHorizonHooks wrapping Arc<RurgeEngine>.
    let engine = Arc::new(engine);
    let hooks = RurgeHorizonHooks::new(Arc::clone(&engine));

    // 8. Package hooks into Mods trait collection.
    let mods: Mods = Arc::new([Box::new(hooks)]);

    // 9. Launch client via horizon_client::entry::main_with.
    horizon_client::entry::main_with(mods);
}
