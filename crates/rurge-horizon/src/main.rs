//! Binary entrypoint for the Rurge Horizon native client.

use std::path::Path;
use std::sync::Arc;

use horizon_client::mod_api::Mods;
use rurge_horizon::hooks::RurgeHorizonHooks;
use rurge_horizon::shaderpack_bundle::ensure_default_shaderpack_installed;
use rurge_runtime::engine::RurgeEngine;

fn main() {
    // 1. Ensure "mods" and "shaderpacks" folders exist.
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

    // 4b. Ensure default shaderpack is active in horizon-graphics.txt if none is picked.
    ensure_default_shaderpack_active();

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

    // 10. Cleanly shut down engine and mod plugins on client exit.
    engine.shutdown();
}

/// Activates `test-classic-shader.zip` by default in `horizon-graphics.txt` if none is configured,
/// ensuring the user immediately sees working classic shaders without menu navigation.
fn ensure_default_shaderpack_active() {
    let path = Path::new("horizon-graphics.txt");
    let content = std::fs::read_to_string(path).unwrap_or_default();
    let has_picked = content.lines().any(|l| {
        l.starts_with("pickedShaderPack:")
            && !l.trim_start_matches("pickedShaderPack:").trim().is_empty()
    });
    if !has_picked {
        let mut lines: Vec<String> = content.lines().map(|s| s.to_string()).collect();
        let mut found = false;
        for line in &mut lines {
            if line.starts_with("pickedShaderPack:") {
                *line = "pickedShaderPack:test-classic-shader.zip".to_string();
                found = true;
                break;
            }
        }
        if !found {
            lines.push("pickedShaderPack:test-classic-shader.zip".to_string());
        }
        let _ = std::fs::write(path, lines.join("\n") + "\n");
    }
}
