//! Package builder for creating `.rmod` ZIP archives from normalized content and assets.

use crate::assets::ExtractedAssets;
use crate::compatibility::CompatibilityReport;
use crate::error::Result;
use crate::ir::{ContentIR, RecipeDef};
use crate::manifest::ModMetadata;
use crate::schema::{ContentSchema, SchemaRecipe, SchemaRecipeResult};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fs::File;
use std::io::{Seek, Write};
use std::path::Path;
use zip::write::SimpleFileOptions;
use zip::ZipWriter;

/// Section `[mod]` of `manifest.toml`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ManifestModSection {
    pub id: String,
    pub name: String,
    pub version: String,
    pub description: String,
    pub authors: Vec<String>,
}

/// Section `[content]` of `manifest.toml`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ManifestContentSection {
    pub schema: String,
    pub data_driven: bool,
}

/// Specification-compliant `manifest.toml` for `.rmod` archives.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RmodManifest {
    #[serde(rename = "mod")]
    pub mod_section: ManifestModSection,
    pub content: ManifestContentSection,
}

impl RmodManifest {
    /// Constructs a `RmodManifest` from extracted mod metadata.
    pub fn from_metadata(meta: &ModMetadata) -> Self {
        Self {
            mod_section: ManifestModSection {
                id: meta.id.clone(),
                name: meta.name.clone(),
                version: meta.version.clone(),
                description: meta.description.clone(),
                authors: meta.authors.clone(),
            },
            content: ManifestContentSection {
                schema: "content.json".to_string(),
                data_driven: true,
            },
        }
    }

    /// Serializes manifest into pretty TOML string.
    pub fn to_toml_string(&self) -> Result<String> {
        toml::to_string_pretty(self).map_err(Into::into)
    }
}

/// Sanitizes an arbitrary version string into valid SemVer.
pub fn sanitize_semver(ver: &str) -> String {
    let trimmed = ver.trim().trim_start_matches('v');
    if semver::Version::parse(trimmed).is_ok() {
        return trimmed.to_string();
    }
    let parts: Vec<&str> = trimmed.split('.').collect();
    if parts.len() == 2
        && parts[0].chars().all(|c| c.is_ascii_digit())
        && parts[1].chars().all(|c| c.is_ascii_digit())
    {
        let padded = format!("{trimmed}.0");
        if semver::Version::parse(&padded).is_ok() {
            return padded;
        }
    } else if parts.len() == 1 && parts[0].chars().all(|c| c.is_ascii_digit()) {
        let padded = format!("{trimmed}.0.0");
        if semver::Version::parse(&padded).is_ok() {
            return padded;
        }
    }
    "1.0.0".to_string()
}

/// Generates `rurge.toml` manifest for `rurge-loader` runtime compatibility.
pub fn generate_rurge_toml(meta: &ModMetadata) -> String {
    let sanitized_version = sanitize_semver(&meta.version);
    let authors_toml = meta
        .authors
        .iter()
        .map(|a| format!("\"{}\"", a.replace('\"', "\\\"")))
        .collect::<Vec<_>>()
        .join(", ");
    let desc = meta.description.replace('\"', "\\\"");
    format!(
        "id = \"{}\"\nname = \"{}\"\nversion = \"{}\"\ndescription = \"{}\"\nauthors = [{}]\n",
        meta.id, meta.name, sanitized_version, desc, authors_toml
    )
}

fn write_zip_entry<W: Write + Seek>(
    zip: &mut ZipWriter<W>,
    written_entries: &mut HashSet<String>,
    options: SimpleFileOptions,
    path: &str,
    bytes: &[u8],
) -> Result<bool> {
    if written_entries.insert(path.to_string()) {
        zip.start_file(path, options)?;
        zip.write_all(bytes)?;
        Ok(true)
    } else {
        Ok(false)
    }
}

/// Builds a complete `.rmod` zip package from content IR, normalized assets, and compatibility report.
pub fn build_rmod_package(
    ir: &ContentIR,
    assets: &ExtractedAssets,
    report: &CompatibilityReport,
    output_path: &Path,
) -> Result<()> {
    if let Some(parent) = output_path.parent()
        && !parent.as_os_str().is_empty()
    {
        std::fs::create_dir_all(parent)?;
    }

    let file = File::create(output_path)?;
    let mut zip = ZipWriter::new(file);
    let options = SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);

    let mut written_entries = HashSet::new();

    // 1. Write manifest.toml
    let manifest = RmodManifest::from_metadata(&ir.metadata);
    let manifest_toml = manifest.to_toml_string()?;
    write_zip_entry(
        &mut zip,
        &mut written_entries,
        options,
        "manifest.toml",
        manifest_toml.as_bytes(),
    )?;

    // 2. Write rurge.toml (for rurge-loader compatibility)
    let rurge_toml = generate_rurge_toml(&ir.metadata);
    write_zip_entry(
        &mut zip,
        &mut written_entries,
        options,
        "rurge.toml",
        rurge_toml.as_bytes(),
    )?;

    // 3. Write content.json
    let schema = ContentSchema::from_ir(ir);
    let content_json = schema.to_json_pretty()?;
    write_zip_entry(
        &mut zip,
        &mut written_entries,
        options,
        "content.json",
        content_json.as_bytes(),
    )?;

    // 4. Write compatibility-report.json
    let report_json = report.to_json_pretty()?;
    write_zip_entry(
        &mut zip,
        &mut written_entries,
        options,
        "compatibility-report.json",
        report_json.as_bytes(),
    )?;

    let default_ns = &ir.metadata.id;

    // 5. Write blockstate JSON files
    for (key, bytes) in &assets.blockstates {
        let zip_path = if let Some((ns, subpath)) = key.split_once(':') {
            format!("assets/{ns}/blockstates/{subpath}.json")
        } else {
            format!("assets/{default_ns}/blockstates/{key}.json")
        };
        write_zip_entry(&mut zip, &mut written_entries, options, &zip_path, bytes)?;
    }

    // 6. Write model JSON files
    for (key, bytes) in &assets.models {
        let zip_path = if let Some((ns, subpath)) = key.split_once(':') {
            format!("assets/{ns}/models/{subpath}.json")
        } else {
            format!("assets/{default_ns}/models/{key}.json")
        };
        write_zip_entry(&mut zip, &mut written_entries, options, &zip_path, bytes)?;
    }

    // 7. Write texture PNG files
    for (key, bytes) in &assets.textures {
        let zip_path = if let Some((ns, subpath)) = key.split_once(':') {
            format!("assets/{ns}/textures/{subpath}.png")
        } else {
            format!("assets/{default_ns}/textures/{key}.png")
        };
        write_zip_entry(&mut zip, &mut written_entries, options, &zip_path, bytes)?;
    }

    // 8. Write loot table JSON files
    for (key, bytes) in &assets.loot_tables {
        let zip_path = if let Some((ns, subpath)) = key.split_once(':') {
            format!("data/{ns}/loot_tables/{subpath}.json")
        } else {
            format!("data/{default_ns}/loot_tables/{key}.json")
        };
        write_zip_entry(&mut zip, &mut written_entries, options, &zip_path, bytes)?;
    }

    // 9. Write recipe files (first raw, then synthetic from IR)
    for (key, bytes) in &assets.raw_recipes {
        let zip_path = if let Some((ns, subpath)) = key.split_once(':') {
            format!("data/{ns}/recipes/{subpath}.json")
        } else {
            format!("data/{default_ns}/recipes/{key}.json")
        };
        write_zip_entry(&mut zip, &mut written_entries, options, &zip_path, bytes)?;
    }

    for recipe in &ir.recipes {
        let key = recipe.id();
        let zip_path = if let Some((ns, subpath)) = key.split_once(':') {
            format!("data/{ns}/recipes/{subpath}.json")
        } else {
            format!("data/{default_ns}/recipes/{key}.json")
        };
        if !written_entries.contains(&zip_path) {
            let bytes = serialize_recipe_def(recipe)?;
            write_zip_entry(&mut zip, &mut written_entries, options, &zip_path, &bytes)?;
        }
    }

    // 10. Write language files
    for (path, bytes) in &assets.raw_lang_files {
        write_zip_entry(&mut zip, &mut written_entries, options, path, bytes)?;
    }

    let lang_fallback_path = format!("assets/{default_ns}/lang/en_us.json");
    if !written_entries.contains(&lang_fallback_path) && !ir.lang.is_empty() {
        let lang_bytes = serde_json::to_vec_pretty(&ir.lang)?;
        write_zip_entry(
            &mut zip,
            &mut written_entries,
            options,
            &lang_fallback_path,
            &lang_bytes,
        )?;
    }

    zip.finish()?;
    Ok(())
}

fn serialize_recipe_def(recipe: &RecipeDef) -> Result<Vec<u8>> {
    let schema_recipe = match recipe {
        RecipeDef::Shaped {
            pattern,
            key,
            result_item,
            result_count,
            ..
        } => SchemaRecipe::Shaped {
            id: None,
            pattern: pattern.clone(),
            key: key.clone(),
            result: SchemaRecipeResult {
                item: result_item.clone(),
                count: *result_count,
            },
        },
        RecipeDef::Shapeless {
            ingredients,
            result_item,
            result_count,
            ..
        } => SchemaRecipe::Shapeless {
            id: None,
            ingredients: ingredients.clone(),
            result: SchemaRecipeResult {
                item: result_item.clone(),
                count: *result_count,
            },
        },
        RecipeDef::Smelting {
            ingredient,
            result_item,
            experience,
            cooking_time,
            ..
        } => SchemaRecipe::Smelting {
            id: None,
            ingredient: ingredient.clone(),
            result: SchemaRecipeResult {
                item: result_item.clone(),
                count: 1,
            },
            experience: *experience,
            cooking_time: *cooking_time,
        },
    };
    serde_json::to_vec_pretty(&schema_recipe).map_err(Into::into)
}
