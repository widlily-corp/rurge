use std::path::PathBuf;
use std::process::Command;
use clap::Args;

use crate::CargoRurgeError;

/// Arguments for `cargo rurge build`.
#[derive(Args, Debug, Clone, Default)]
pub struct BuildArgs {
    /// Build artifacts in release mode, with optimizations
    #[arg(short, long)]
    pub release: bool,

    /// Path to Cargo.toml manifest file or crate directory
    #[arg(long)]
    pub manifest_path: Option<PathBuf>,

    /// Target triple to build for
    #[arg(long)]
    pub target: Option<String>,
}

/// Executes cargo build for the mod crate.
pub fn execute(args: BuildArgs) -> Result<(), CargoRurgeError> {
    let mut cmd = Command::new("cargo");
    cmd.arg("build");

    if args.release {
        cmd.arg("--release");
    }

    if let Some(ref path) = args.manifest_path {
        let manifest_file = if path.is_dir() {
            path.join("Cargo.toml")
        } else {
            path.clone()
        };
        cmd.arg("--manifest-path").arg(manifest_file);
    }

    if let Some(ref target) = args.target {
        cmd.arg("--target").arg(target);
    }

    let status = cmd.status()?;
    if !status.success() {
        return Err(CargoRurgeError::BuildFailed(status.code()));
    }

    println!("Mod build succeeded.");
    Ok(())
}
