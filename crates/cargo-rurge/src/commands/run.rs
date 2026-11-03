use std::process::Command;
use clap::Args;

use crate::CargoRurgeError;

/// Arguments for `cargo rurge run`.
#[derive(Args, Debug, Clone, Default, PartialEq, Eq)]
pub struct RunArgs {
    /// Build and run in release mode
    #[arg(long, short)]
    pub release: bool,

    /// Additional arguments forwarded to the client
    #[arg(last = true)]
    pub args: Vec<String>,
}

/// Executes `cargo run -p rurge-horizon` forwarding `--release` if specified and any trailing args.
pub fn execute(args: RunArgs) -> Result<(), CargoRurgeError> {
    let mut cmd = Command::new("cargo");
    cmd.arg("run").arg("-p").arg("rurge-horizon");

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
