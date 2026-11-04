//! Mod manifest parsers for Fabric, Quilt, Modern Forge, NeoForge, and Legacy Forge.

use crate::error::{ConverterError, Result};
use serde::{Deserialize, Serialize};
use std::io::{Read, Seek};

/// Detected mod loader ecosystem for the archive.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ModLoader {
    Fabric,
    Quilt,
    Forge,
    NeoForge,
    LegacyForge,
    Unknown,
}

/// Unified metadata extracted from loader manifest files.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModMetadata {
    pub id: String,
    pub name: String,
    pub version: String,
    pub authors: Vec<String>,
    pub description: String,
    pub loader: ModLoader,
}

/// Parses a Fabric (`fabric.mod.json`) or Quilt manifest string.
pub fn parse_fabric_manifest(json_str: &str) -> Result<ModMetadata> {
    let val: serde_json::Value = serde_json::from_str(json_str)?;

    let id = val
        .get("id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ConverterError::Other("Fabric manifest missing 'id'".into()))?
        .to_string();

    let name = val
        .get("name")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .unwrap_or_else(|| id.clone());

    let version = val
        .get("version")
        .and_then(|v| v.as_str())
        .unwrap_or("1.0.0")
        .to_string();

    let description = val
        .get("description")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .trim()
        .to_string();

    let mut authors = Vec::new();
    if let Some(authors_val) = val.get("authors") {
        match authors_val {
            serde_json::Value::Array(arr) => {
                for item in arr {
                    match item {
                        serde_json::Value::String(s) => {
                            let trimmed = s.trim().to_string();
                            if !trimmed.is_empty() {
                                authors.push(trimmed);
                            }
                        }
                        serde_json::Value::Object(map) => {
                            if let Some(name_val) = map.get("name").and_then(|v| v.as_str()) {
                                let trimmed = name_val.trim().to_string();
                                if !trimmed.is_empty() {
                                authors.push(trimmed);
                                }
                            }
                        }
                        _ => {}
                    }
                }
            }
            serde_json::Value::String(s) => {
                let trimmed = s.trim().to_string();
                if !trimmed.is_empty() {
                    authors.push(trimmed);
                }
            }
            _ => {}
        }
    }

    let loader = if val.get("quilt_loader").is_some() {
        ModLoader::Quilt
    } else {
        ModLoader::Fabric
    };

    Ok(ModMetadata {
        id,
        name,
        version,
        authors,
        description,
        loader,
    })
}

/// Parses a Modern Forge or NeoForge TOML manifest string (`mods.toml`).
pub fn parse_forge_manifest(toml_str: &str) -> Result<ModMetadata> {
    let val: toml::Value = toml::from_str(toml_str)?;

    let mod_loader_str = val
        .get("modLoader")
        .and_then(|v| v.as_str())
        .unwrap_or("javafml")
        .to_lowercase();

    let is_neoforge = mod_loader_str.contains("neoforge");

    let mods_table = val
        .get("mods")
        .and_then(|v| v.as_array())
        .and_then(|arr| arr.first())
        .ok_or_else(|| ConverterError::Other("Forge mods.toml missing [[mods]] entry".into()))?;

    let id = mods_table
        .get("modId")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ConverterError::Other("Forge mods.toml missing 'modId'".into()))?
        .to_string();

    let name = mods_table
        .get("displayName")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .unwrap_or_else(|| id.clone());

    let raw_version = mods_table
        .get("version")
        .and_then(|v| v.as_str())
        .unwrap_or("1.0.0");

    let version = if raw_version.starts_with("${") {
        "1.0.0".to_string()
    } else {
        raw_version.to_string()
    };

    let description = mods_table
        .get("description")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .trim()
        .to_string();

    let mut authors = Vec::new();
    if let Some(authors_val) = mods_table.get("authors") {
        match authors_val {
            toml::Value::String(s) => {
                for part in s.split(',') {
                    let trimmed = part.trim().to_string();
                    if !trimmed.is_empty() {
                        authors.push(trimmed);
                    }
                }
            }
            toml::Value::Array(arr) => {
                for item in arr {
                    if let toml::Value::String(s) = item {
                        let trimmed = s.trim().to_string();
                        if !trimmed.is_empty() {
                            authors.push(trimmed);
                        }
                    }
                }
            }
            _ => {}
        }
    }

    let loader = if is_neoforge {
        ModLoader::NeoForge
    } else {
        ModLoader::Forge
    };

    Ok(ModMetadata {
        id,
        name,
        version,
        authors,
        description,
        loader,
    })
}

/// Parses a Legacy Forge JSON manifest (`mcmod.info`).
pub fn parse_legacy_manifest(json_str: &str) -> Result<ModMetadata> {
    let val: serde_json::Value = serde_json::from_str(json_str)?;

    let mod_obj = if let Some(arr) = val.as_array() {
        arr.first()
    } else if let Some(list) = val.get("modList").and_then(|v| v.as_array()) {
        list.first()
    } else {
        None
    }
    .ok_or_else(|| ConverterError::Other("Legacy mcmod.info contains no mod entries".into()))?;

    let id = mod_obj
        .get("modid")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ConverterError::Other("Legacy mcmod.info missing 'modid'".into()))?
        .to_string();

    let name = mod_obj
        .get("name")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .unwrap_or_else(|| id.clone());

    let version = mod_obj
        .get("version")
        .and_then(|v| v.as_str())
        .unwrap_or("1.0.0")
        .to_string();

    let description = mod_obj
        .get("description")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .trim()
        .to_string();

    let mut authors = Vec::new();
    if let Some(author_list) = mod_obj.get("authorList").and_then(|v| v.as_array()) {
        for a in author_list {
            if let Some(s) = a.as_str() {
                let trimmed = s.trim().to_string();
                if !trimmed.is_empty() {
                    authors.push(trimmed);
                }
            }
        }
    }

    Ok(ModMetadata {
        id,
        name,
        version,
        authors,
        description,
        loader: ModLoader::LegacyForge,
    })
}

/// Inspects a ZIP archive reader and automatically extracts the appropriate mod manifest.
pub fn parse_manifest_from_zip<R: Read + Seek>(
    archive: &mut zip::ZipArchive<R>,
) -> Result<ModMetadata> {
    // 1. Quilt JSON
    if let Ok(mut file) = archive.by_name("quilt.mod.json") {
        let mut content = String::new();
        file.read_to_string(&mut content)?;
        let mut meta = parse_fabric_manifest(&content)?;
        meta.loader = ModLoader::Quilt;
        return Ok(meta);
    }

    // 2. Fabric JSON
    if let Ok(mut file) = archive.by_name("fabric.mod.json") {
        let mut content = String::new();
        file.read_to_string(&mut content)?;
        return parse_fabric_manifest(&content);
    }

    // 3. NeoForge TOML
    if let Ok(mut file) = archive.by_name("META-INF/neoforge.mods.toml") {
        let mut content = String::new();
        file.read_to_string(&mut content)?;
        let mut meta = parse_forge_manifest(&content)?;
        meta.loader = ModLoader::NeoForge;
        return Ok(meta);
    }

    // 4. Modern Forge TOML
    if let Ok(mut file) = archive.by_name("META-INF/mods.toml") {
        let mut content = String::new();
        file.read_to_string(&mut content)?;
        return parse_forge_manifest(&content);
    }

    // 5. Legacy Forge JSON
    if let Ok(mut file) = archive.by_name("mcmod.info") {
        let mut content = String::new();
        file.read_to_string(&mut content)?;
        return parse_legacy_manifest(&content);
    }

    Err(ConverterError::UnrecognizedModFormat(
        "No recognized mod manifest found (fabric.mod.json, mods.toml, or mcmod.info)".into(),
    ))
}
