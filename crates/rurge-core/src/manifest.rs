use std::collections::HashMap;
use std::path::Path;
use serde::{Deserialize, Serialize};

/// Target environment for a mod.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum ModEnvironment {
    /// Mod runs on both client and dedicated server.
    #[default]
    #[serde(alias = "both", alias = "BOTH")]
    Both,
    /// Mod runs only on the client.
    #[serde(alias = "client", alias = "CLIENT")]
    Client,
    /// Mod runs only on dedicated servers.
    #[serde(alias = "server", alias = "SERVER")]
    Server,
}

/// Errors that can occur when parsing a mod manifest.
#[derive(Debug, thiserror::Error)]
pub enum ManifestError {
    /// File I/O error when reading manifest file.
    #[error("Failed to read manifest file: {0}")]
    Io(#[from] std::io::Error),

    /// Syntax error when deserializing TOML.
    #[error("Failed to parse manifest TOML: {0}")]
    Parse(#[from] toml::de::Error),

    /// Invalid mod ID format.
    #[error("Invalid mod ID '{id}': must be non-empty and contain only alphanumeric characters, underscores, or hyphens")]
    InvalidId { id: String },
}

/// Metadata and dependency specification for a Rurge mod (`rurge.toml`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModManifest {
    /// Unique identifier for the mod (alphanumeric + underscore/hyphen).
    pub id: String,

    /// Human-readable display name.
    pub name: String,

    /// SemVer version of the mod.
    pub version: semver::Version,

    /// Optional description of the mod.
    #[serde(default)]
    pub description: Option<String>,

    /// List of authors/contributors.
    #[serde(default)]
    pub authors: Vec<String>,

    /// Entrypoint dynamic library path or symbol name.
    #[serde(default)]
    pub entrypoint: Option<String>,

    /// Execution environment target.
    #[serde(default)]
    pub environment: ModEnvironment,

    /// Required dependencies and their SemVer version requirements.
    #[serde(default)]
    pub dependencies: HashMap<String, semver::VersionReq>,

    /// Incompatible mods and the conflicting SemVer version requirements.
    #[serde(default)]
    pub incompatibilities: HashMap<String, semver::VersionReq>,
}

impl ModManifest {
    /// Validates whether a mod ID is valid (non-empty, alphanumeric + `_` or `-`).
    pub fn is_valid_id(id: &str) -> bool {
        !id.is_empty() && id.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
    }

    /// Parses a `ModManifest` from a TOML string and validates its ID.
    pub fn from_toml_str(s: &str) -> Result<Self, ManifestError> {
        let manifest: Self = toml::from_str(s)?;
        if !Self::is_valid_id(&manifest.id) {
            return Err(ManifestError::InvalidId { id: manifest.id });
        }
        Ok(manifest)
    }

    /// Reads and parses a `ModManifest` from a file path.
    pub fn from_file<P: AsRef<Path>>(path: P) -> Result<Self, ManifestError> {
        let content = std::fs::read_to_string(path.as_ref())?;
        Self::from_toml_str(&content)
    }
}
