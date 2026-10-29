use std::fs;
use std::path::PathBuf;
use clap::Args;
use rurge_core::manifest::ModManifest;

use crate::templates;
pub use crate::templates::TemplateFlavor;
use crate::CargoRurgeError;

/// Arguments for `cargo rurge new`.
#[derive(Args, Debug, Clone)]
pub struct NewArgs {
    /// Name / ID of the new mod (alphanumeric, hyphens, and underscores)
    pub name: String,

    /// Template flavor to use
    #[arg(short, long, value_enum, default_value_t = TemplateFlavor::Hud)]
    pub template: TemplateFlavor,

    /// Target directory to create the mod in (defaults to `./<name>`)
    #[arg(short, long)]
    pub path: Option<PathBuf>,

    /// Force creation even if the target directory already exists and is not empty
    #[arg(short, long)]
    pub force: bool,
}

/// Executes the mod project scaffolding command.
pub fn execute(args: NewArgs) -> Result<PathBuf, CargoRurgeError> {
    if !ModManifest::is_valid_id(&args.name) {
        return Err(CargoRurgeError::InvalidModId(args.name));
    }

    let target_dir = args.path.unwrap_or_else(|| PathBuf::from(&args.name));

    if target_dir.exists() {
        let is_empty = fs::read_dir(&target_dir)?
            .next()
            .is_none();
        if !is_empty && !args.force {
            return Err(CargoRurgeError::DirectoryNotEmpty(target_dir));
        }
    }

    // Create directory tree
    fs::create_dir_all(&target_dir)?;
    fs::create_dir_all(target_dir.join("src"))?;
    fs::create_dir_all(target_dir.join("assets"))?;
    fs::create_dir_all(target_dir.join("data"))?;

    // Create .gitkeep in assets and data
    let _ = fs::write(target_dir.join("assets").join(".gitkeep"), b"");
    let _ = fs::write(target_dir.join("data").join(".gitkeep"), b"");

    // Write Cargo.toml
    let cargo_toml = templates::generate_cargo_toml(&args.name);
    fs::write(target_dir.join("Cargo.toml"), cargo_toml)?;

    // Write rurge.toml manifest
    let rurge_toml = templates::generate_rurge_toml(&args.name, args.template);
    fs::write(target_dir.join("rurge.toml"), rurge_toml)?;

    // Write src/lib.rs
    let lib_rs = templates::generate_lib_rs(&args.name, args.template);
    fs::write(target_dir.join("src").join("lib.rs"), lib_rs)?;

    println!("Scaffolded Rurge mod '{}' at {}", args.name, target_dir.display());
    Ok(target_dir)
}
