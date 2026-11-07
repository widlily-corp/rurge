//! Binary entrypoint for the Rurge Horizon native client.

use std::path::Path;
use std::sync::Arc;

use horizon_client::mod_api::Mods;
use rurge_horizon::hooks::RurgeHorizonHooks;
use rurge_horizon::jar_scanner::convert_dropped_jars_in;
use rurge_horizon::shaderpack_bundle::ensure_default_shaderpack_installed;
use rurge_runtime::engine::RurgeEngine;

fn main() {
    // 1. Ensure "mods", "horizon-mods", and "shaderpacks" folders exist and are unified.
    let mods_dir = Path::new("mods");
    let horizon_mods_dir = Path::new("horizon-mods");
    let shaderpacks_dir = Path::new("shaderpacks");

    ensure_mod_directories_linked(mods_dir, horizon_mods_dir);

    if !shaderpacks_dir.exists()
        && let Err(e) = std::fs::create_dir_all(shaderpacks_dir)
    {
        log::warn!("Failed to create shaderpacks directory: {e}");
    }

    // 2. Install bundled shaderpack with options and translations.
    match ensure_default_shaderpack_installed(shaderpacks_dir) {
        Ok(path) => log::info!("Ensured default shaderpack at '{}'", path.display()),
        Err(e) => log::warn!("Failed to install default shaderpack: {e}"),
    }

    // 3. Ensure default shaderpack and compatible graphics API (Auto/Vulkan) are configured
    // across both horizon-graphics.txt and options.txt (preventing OpenGL Naga depth blit crashes).
    ensure_compatible_graphics_config();
    let best_sp = if Path::new("shaderpacks/ComplementaryReimagined_r5.9.3.zip").exists() {
        "ComplementaryReimagined_r5.9.3.zip"
    } else {
        "test-classic-shader.zip"
    };
    horizon_shaders::set_live_shader_pack(best_sp);

    // 4. Auto-convert any dropped Minecraft Java .jar files in "mods" and "horizon-mods" to .rmod packages.
    let mut converted_jars = convert_dropped_jars_in(mods_dir);
    if horizon_mods_dir.exists() && horizon_mods_dir != mods_dir {
        converted_jars.extend(convert_dropped_jars_in(horizon_mods_dir));
    }
    if !converted_jars.is_empty() {
        log::info!("Auto-converted {} dropped JAR mod(s) to native .rmod", converted_jars.len());
    }

    // 4b. Ensure all .rmod packages have twin .hmod packages and are enabled in horizon-mods.txt
    sync_hmod_packages(mods_dir, horizon_mods_dir);
    ensure_mods_config(&[mods_dir, horizon_mods_dir]);

    // 5. Initialize RurgeEngine.
    let mut engine = RurgeEngine::new();

    // 6. Discover and load .rmod mods from "mods" folder using ModManager (via engine.initialize()).
    if let Err(e) = engine.initialize() {
        log::error!("Failed to initialize RurgeEngine: {e}");
    } else {
        // Also register any declarative content from horizon-mods if it is a separate directory
        if horizon_mods_dir.exists()
            && let Ok(archives) = engine.mod_manager().discover_archives(horizon_mods_dir)
        {
            for archive in &archives {
                if let Ok(Some(schema_str)) = archive.read_content_schema()
                    && let Ok(schema) = rurge_converter::ContentSchema::from_json_str(&schema_str)
                {
                    let _ = engine.content().register_schema(&schema.mod_id, &schema);
                }
            }
        }

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

/// Unifies the `mods/` and `horizon-mods/` directories so clicking "Open Mods Folder"
/// in Horizon's UI opens the Rurge mods directory where `.rmod` and `.jar` mods reside.
fn ensure_mod_directories_linked(mods_dir: &Path, horizon_mods_dir: &Path) {
    let _ = std::fs::create_dir_all(mods_dir);

    // On Windows, if horizon-mods does not exist, create an NTFS directory junction to mods/
    if !horizon_mods_dir.exists() {
        #[cfg(windows)]
        {
            let _ = std::process::Command::new("cmd")
                .args(["/C", "mklink", "/J", "horizon-mods", "mods"])
                .output();
        }
        #[cfg(not(windows))]
        {
            let _ = std::os::unix::fs::symlink("mods", "horizon-mods");
        }
    }

    if !horizon_mods_dir.exists() {
        let _ = std::fs::create_dir_all(horizon_mods_dir);
    }

    let readme = "=== Rurge Mod Loader ===\n\n\
Drop your native Rurge (.rmod) mod packages or legacy Minecraft Java (.jar) mods in this folder.\n\
Dropped .jar mods are automatically detected, converted to .rmod, and loaded on game startup.\n\
In-game Options > Mods lists all active native and converted mods with block/item counts.\n";

    let readme_path = mods_dir.join("README.txt");
    if !readme_path.exists() {
        let _ = std::fs::write(readme_path, readme);
    }
}

/// Activates `test-classic-shader.zip` by default in `horizon-graphics.txt` if none is configured,
/// and ensures a compatible modern backend (Auto/Vulkan/Dx12) is selected instead of legacy OpenGL
/// where Naga's GLSL backend fails on depth texture loads (`textureLoad` in `fs_blit`).
fn ensure_compatible_graphics_config() {
    let graphics_path = Path::new("horizon-graphics.txt");
    let content = std::fs::read_to_string(graphics_path).unwrap_or_default();
    let mut lines: Vec<String> = content.lines().map(|s| s.to_string()).collect();
    let mut modified = false;

    // 1. Ensure pickedShaderPack is set
        let best_shader = if Path::new("shaderpacks/ComplementaryReimagined_r5.9.3.zip").exists() {
        "ComplementaryReimagined_r5.9.3.zip"
    } else if Path::new("shaderpacks/ComplementaryReimagined_r5.8.1.zip").exists() {
        "ComplementaryReimagined_r5.8.1.zip"
    } else {
        "test-classic-shader.zip"
    };

    let has_picked = lines.iter().any(|l| {
        l.starts_with("pickedShaderPack:")
            && !l.trim_start_matches("pickedShaderPack:").trim().is_empty()
            && l.trim_start_matches("pickedShaderPack:").trim() != "test-classic-shader.zip"
    });
    if !has_picked {
        let mut found = false;
        for line in &mut lines {
            if line.starts_with("pickedShaderPack:") {
                *line = format!("pickedShaderPack:{best_shader}");
                found = true;
                break;
            }
        }
        if !found {
            lines.push(format!("pickedShaderPack:{best_shader}"));
        }
        modified = true;
    }

    // 2. Ensure graphicsApi in horizon-graphics.txt is auto/vulkan rather than opengl
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
        let _ = std::fs::write(graphics_path, lines.join("\n") + "\n");
    }

    // 3. Ensure options.txt does not force legacy OpenGL (which crashes wgpu on depth textureLoad)
    let options_path = Path::new("options.txt");
    if let Ok(opt_content) = std::fs::read_to_string(options_path) {
        let mut opt_lines: Vec<String> = opt_content.lines().map(|s| s.to_string()).collect();
        let mut opt_modified = false;
        for line in &mut opt_lines {
            if line.starts_with("preferredGraphicsBackend:") && line.contains("opengl") {
                *line = "preferredGraphicsBackend:\"default\"".to_string();
                opt_modified = true;
            }
            if line.starts_with("exclusiveFullscreen:") && line.contains("true") {
                *line = "exclusiveFullscreen:false".to_string();
                opt_modified = true;
            }
            if line.starts_with("fullscreen:") && line.contains("true") {
                *line = "fullscreen:false".to_string();
                opt_modified = true;
            }
        }
        if opt_modified {
            let _ = std::fs::write(options_path, opt_lines.join("\n") + "\n");
        }
    }
}


/// Ensures that every `.rmod` package has a twin `.hmod` package so Horizon engine mounts its assets and data.
fn sync_hmod_packages(mods_dir: &Path, horizon_mods_dir: &Path) {
    for dir in &[mods_dir, horizon_mods_dir] {
        if let Ok(entries) = std::fs::read_dir(dir) {
            for entry in entries.flatten() {
                let p = entry.path();
                if p.is_file() && p.extension().is_some_and(|e| e == "rmod") {
                    let hmod = p.with_extension("hmod");
                    if !hmod.exists() {
                        let _ = std::fs::copy(&p, &hmod);
                    }
                }
            }
        }
    }
}

/// Ensures that all detected mods are marked enabled in `horizon-mods.txt`.
fn ensure_mods_config(dirs: &[&Path]) {
    let config_path = Path::new("horizon-mods.txt");
    let content = std::fs::read_to_string(config_path).unwrap_or_default();
    let mut lines: Vec<String> = content.lines().map(|s| s.to_string()).collect();
    let mut modified = false;

    let mut detected_ids = Vec::new();
    for dir in dirs {
        if let Ok(entries) = std::fs::read_dir(dir) {
            for entry in entries.flatten() {
                let p = entry.path();
                let is_pkg = p.is_file()
                    && p.extension().is_some_and(|e| e == "rmod" || e == "hmod");
                if is_pkg {
                    if let Some(stem) = p.file_stem().and_then(|s| s.to_str()) {
                        let id = stem.split('-').next().unwrap_or(stem).to_string();
                        if !detected_ids.contains(&id) {
                            detected_ids.push(id);
                        }
                    }
                }
            }
        }
    }

    for id in detected_ids {
        let expected = format!("mod.{id}:true");
        let disabled_pattern = format!("mod.{id}:false");
        let native_pattern = format!("mod.rurge_mod_{id}:false");

        for line in &mut lines {
            if *line == disabled_pattern || *line == native_pattern {
                *line = expected.clone();
                modified = true;
            }
        }

        if !lines.iter().any(|l| l.trim() == expected) {
            lines.push(expected);
            modified = true;
        }
    }

    if modified {
        let _ = std::fs::write(config_path, lines.join("\n") + "\n");
    }
}
