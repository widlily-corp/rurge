//! Binary entrypoint for the Rurge Horizon native client.

use std::path::Path;
use std::sync::Arc;

use horizon_client::mod_api::Mods;
use rurge_horizon::hooks::RurgeHorizonHooks;
use rurge_horizon::jar_scanner::convert_dropped_jars_in;
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

    // 4b. Ensure default shaderpack and compatible graphics API (Auto/Vulkan) are configured.
    ensure_compatible_graphics_config();

    // 4c. Auto-convert any dropped Minecraft Java .jar files in "mods" to .rmod packages.
    let converted_jars = convert_dropped_jars_in(mods_dir);
    if !converted_jars.is_empty() {
        log::info!("Auto-converted {} dropped JAR mod(s) to native .rmod", converted_jars.len());
    }

    // 5. Initialize RurgeEngine.
    let mut engine = RurgeEngine::new();

    // 6. Discover and load .rmod mods from "mods" folder using ModManager (via engine.initialize()).
    if let Err(e) = engine.initialize() {
        log::error!("Failed to initialize RurgeEngine: {e}");
    } else {
        let loaded_count = engine.mod_manager().loaded_plugins().len();
        let content_count = engine.content().block_count() + engine.content().item_count();
        log::info!("Discovered and loaded {loaded_count} .rmod plugin(s), {content_count} custom content item(s)");
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
/// and ensures a compatible modern backend (Auto/Vulkan/Dx12) is selected instead of legacy OpenGL
/// where Naga's GLSL backend fails on depth texture loads (`textureLoad` in `fs_blit`).
fn ensure_compatible_graphics_config() {
    let path = Path::new("horizon-graphics.txt");
    let content = std::fs::read_to_string(path).unwrap_or_default();
    let mut lines: Vec<String> = content.lines().map(|s| s.to_string()).collect();
    let mut modified = false;

    // 1. Ensure pickedShaderPack is set
    let has_picked = lines.iter().any(|l| {
        l.starts_with("pickedShaderPack:")
            && !l.trim_start_matches("pickedShaderPack:").trim().is_empty()
    });
    if !has_picked {
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
        modified = true;
    }

    // 2. Ensure graphicsApi is auto/vulkan rather than opengl (which breaks shader blit in wgpu)
    let mut api_found = false;
    for line in &mut lines {
        if line.starts_with("graphicsApi:") {
            api_found = true;
            if line.trim() == "graphicsApi:opengl" {
                *line = "graphicsApi:auto".to_string();
                modified = true;
            }
            break;
        }
    }
    if !api_found {
        lines.push("graphicsApi:auto".to_string());
        modified = true;
    }

    if modified {
        let _ = std::fs::write(path, lines.join("\n") + "\n");
    }
}
