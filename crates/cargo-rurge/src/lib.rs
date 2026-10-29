use std::path::PathBuf;
use clap::{Parser, Subcommand};

pub mod commands;
pub mod templates;

/// Unified errors for `cargo-rurge` CLI and library operations.
#[derive(Debug, thiserror::Error)]
pub enum CargoRurgeError {
    /// Standard file I/O errors.
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    /// ZIP archive creation or reading errors.
    #[error("ZIP error: {0}")]
    Zip(#[from] zip::result::ZipError),

    /// Archive verification or loader errors.
    #[error("Loader error: {0}")]
    Loader(#[from] rurge_loader::ArchiveError),

    /// Manifest parsing or validation errors.
    #[error("Manifest error: {0}")]
    Manifest(#[from] rurge_core::ManifestError),

    /// Invalid mod identifier format.
    #[error("Invalid mod ID '{0}': ID must be non-empty and contain only alphanumeric, underscores, or hyphens")]
    InvalidModId(String),

    /// Directory exists and is not empty when scaffolding without `--force`.
    #[error("Target directory '{0}' already exists and is not empty. Use --force to overwrite.")]
    DirectoryNotEmpty(PathBuf),

    /// Manifest `rurge.toml` was not found.
    #[error("Manifest file 'rurge.toml' not found at '{0}'")]
    ManifestNotFound(PathBuf),

    /// Target dynamic library could not be found.
    #[error("Compiled binary for target triple '{target}' not found (searched: {searched:?})")]
    BinaryNotFound {
        target: String,
        searched: Vec<PathBuf>,
    },

    /// Cargo build subprocess failure.
    #[error("Cargo build failed with exit code: {0:?}")]
    BuildFailed(Option<i32>),

    /// Custom error message.
    #[error("{0}")]
    Custom(String),
}

/// Developer CLI tool for Rurge native mods (scaffolding, building, packaging).
#[derive(Parser, Debug)]
#[command(
    name = "cargo-rurge",
    bin_name = "cargo-rurge",
    version,
    about = "Developer CLI tool for Rurge native mods"
)]
pub enum Cli {
    /// Cargo subcommand wrapper when invoked as `cargo rurge <cmd>`
    #[command(subcommand)]
    Rurge(Commands),

    /// Scaffold a new Rurge mod project
    New(commands::new::NewArgs),

    /// Build the mod crate using cargo
    Build(commands::build::BuildArgs),

    /// Pack the mod into a .rmod ZIP package
    Pack(commands::pack::PackArgs),
}

/// Commands available under `cargo-rurge` or `cargo rurge`.
#[derive(Subcommand, Debug, Clone)]
pub enum Commands {
    /// Scaffold a new Rurge mod project
    New(commands::new::NewArgs),

    /// Build the mod crate using cargo
    Build(commands::build::BuildArgs),

    /// Pack the mod into a .rmod ZIP package
    Pack(commands::pack::PackArgs),
}

impl Cli {
    /// Dispatches and executes the parsed CLI command.
    pub fn execute(self) -> Result<(), CargoRurgeError> {
        match self {
            Cli::Rurge(cmd) => cmd.execute(),
            Cli::New(args) => {
                commands::new::execute(args)?;
                Ok(())
            }
            Cli::Build(args) => {
                commands::build::execute(args)?;
                Ok(())
            }
            Cli::Pack(args) => {
                commands::pack::execute(args)?;
                Ok(())
            }
        }
    }
}

impl Commands {
    /// Executes the subcommand.
    pub fn execute(self) -> Result<(), CargoRurgeError> {
        match self {
            Commands::New(args) => {
                commands::new::execute(args)?;
                Ok(())
            }
            Commands::Build(args) => {
                commands::build::execute(args)?;
                Ok(())
            }
            Commands::Pack(args) => {
                commands::pack::execute(args)?;
                Ok(())
            }
        }
    }
}
