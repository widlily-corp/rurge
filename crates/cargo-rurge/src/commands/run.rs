use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use clap::Args;
use rurge_core::manifest::ModManifest;

use crate::commands::build::{self, BuildArgs};
use crate::commands::pack::{self, PackArgs};
use crate::CargoRurgeError;

/// Arguments for `cargo rurge run`.
#[derive(Args, Debug, Clone, Default, PartialEq, Eq)]
pub struct RunArgs {
    /// Build and run in release mode
    #[arg(long, short)]
    pub release: bool,

    /// Path to mod project root or manifest (defaults to current directory)
    #[arg(long, short)]
    pub path: Option<PathBuf>,

    /// Destination mods directory where mod packages are deployed (defaults to "mods")
    #[arg(long)]
    pub mods_dir: Option<PathBuf>,

    /// Target triple to build for
    #[arg(long)]
    pub target: Option<String>,

    /// Skip building the mod before launching
    #[arg(long)]
    pub no_build: bool,

    /// Dry run mode: compiles and links into mods/ without spawning the game client process
    #[arg(long)]
    pub dry_run: bool,

    /// Additional arguments forwarded to the client
    #[arg(last = true)]
    pub args: Vec<String>,
}

/// Resolves the cargo package name to launch for running the game client.
pub fn resolve_client_package(_args: &RunArgs) -> &'static str {
    "rurge-client"
}

/// Links `src` file to `dst` using symbolic link, hard link, or copy as fallback.
pub fn link_file(src: &Path, dst: &Path) -> Result<(), std::io::Error> {
    if dst.exists() {
        let _ = fs::remove_file(dst);
    }
    if let Some(parent) = dst.parent() {
        fs::create_dir_all(parent)?;
    }

    #[cfg(windows)]
    let symlink_res = std::os::windows::fs::symlink_file(src, dst);
    #[cfg(unix)]
    let symlink_res = std::os::unix::fs::symlink(src, dst);

    if symlink_res.is_ok() {
        return Ok(());
    }

    if fs::hard_link(src, dst).is_ok() {
        return Ok(());
    }

    fs::copy(src, dst)?;
    Ok(())
}

/// Prepares, compiles, and deploys the mod into the destination `mods/` directory.
/// Returns the path to the deployed `.rmod` archive in `mods_dir`.
pub fn prepare_and_link_mod(
    project_dir: &Path,
    mods_dir: &Path,
    release: bool,
    target: Option<&str>,
    no_build: bool,
) -> Result<PathBuf, CargoRurgeError> {
    let manifest_path = project_dir.join("rurge.toml");
    if !manifest_path.is_file() {
        return Err(CargoRurgeError::ManifestNotFound(manifest_path));
    }

    let manifest = ModManifest::from_file(&manifest_path)?;

    // 1. Build the mod crate using cargo build unless --no-build is set
    if !no_build {
        build::execute(BuildArgs {
            release,
            manifest_path: Some(project_dir.to_path_buf()),
            target: target.map(|s| s.to_string()),
        })?;
    }

    // 2. Package mod into a standard .rmod archive inside target/
    let target_dir = project_dir.join("target");
    let rmod_path = pack::execute(PackArgs {
        path: Some(project_dir.to_path_buf()),
        out: Some(target_dir),
        release,
        target: target.map(|s| s.to_string()),
        binary: None,
    })?;

    // 3. Ensure mods/ directory exists and link the .rmod package into it
    fs::create_dir_all(mods_dir)?;
    let dest_rmod_name = format!("{}-{}.rmod", manifest.id, manifest.version);
    let dest_rmod_path = mods_dir.join(dest_rmod_name);
    link_file(&rmod_path, &dest_rmod_path)?;

    println!(
        "Linked mod package '{}' into '{}'",
        rmod_path.display(),
        dest_rmod_path.display()
    );

    Ok(dest_rmod_path)
}

/// Executes `cargo rurge run`.
///
/// If invoked inside a mod directory (with `rurge.toml`), compiles the mod,
/// packs and links the `.rmod` package into `mods/`, and launches `rurge-client`.
pub fn execute(args: RunArgs) -> Result<(), CargoRurgeError> {
    let project_dir = match args.path {
        Some(ref p) if p.is_file() => p.parent().unwrap_or(Path::new(".")).to_path_buf(),
        Some(ref p) => p.clone(),
        None => std::env::current_dir()?,
    };

    let mods_dir = args.mods_dir.clone().unwrap_or_else(|| PathBuf::from("mods"));
    let has_manifest = project_dir.join("rurge.toml").is_file();

    if has_manifest {
        prepare_and_link_mod(
            &project_dir,
            &mods_dir,
            args.release,
            args.target.as_deref(),
            args.no_build,
        )?;
    } else if args.path.is_some() {
        return Err(CargoRurgeError::ManifestNotFound(
            project_dir.join("rurge.toml"),
        ));
    }

    if args.dry_run {
        println!("Dev run dry-run completed successfully; skipping game client process.");
        return Ok(());
    }

    let mut cmd = Command::new("cargo");
    cmd.arg("run").arg("-p").arg(resolve_client_package(&args));

    if args.release {
        cmd.arg("--release");
    }

    if !args.args.is_empty() {
        cmd.arg("--");
        cmd.args(&args.args);
    }

    let status = cmd.status()?;
    if !status.success() {
        return Err(CargoRurgeError::RunFailed(status.code()));
    }

    Ok(())
}
