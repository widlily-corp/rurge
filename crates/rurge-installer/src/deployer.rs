use std::fs;
use std::io;
use std::path::Path;

/// Sets up production-ready directory structure and default graphics configs.
pub fn deploy_default_configs(target_root: &Path) -> io::Result<()> {
    fs::create_dir_all(target_root)?;
    fs::create_dir_all(target_root.join("shaderpacks"))?;
    fs::create_dir_all(target_root.join("mods"))?;
    fs::create_dir_all(target_root.join("horizon-mods"))?;

    let options_path = target_root.join("options.txt");
    if !options_path.exists() {
        let default_options = "\
preferredGraphicsBackend:\"default\"
fullscreen:false
exclusiveFullscreen:false
renderDistance:16
simulationDistance:12
maxFps:144
guiScale:0
fov:70.0
";
        fs::write(&options_path, default_options)?;
    }

    let graphics_path = target_root.join("horizon-graphics.txt");
    if !graphics_path.exists() {
        let default_graphics = "\
preferredBackend:\"default\"
enableVsync:true
msaaSamples:1
";
        fs::write(&graphics_path, default_graphics)?;
    }

    Ok(())
}
