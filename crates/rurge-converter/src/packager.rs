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
use std::io::{Read, Seek, Write};
use std::path::Path;
use zip::write::SimpleFileOptions;
use zip::ZipWriter;

/// Maximum allowable uncompressed size for any individual package entry (64 MB).
///
/// Guards against decompression bombs (zip bombs) and out-of-memory denial-of-service.
pub const MAX_UNCOMPRESSED_ENTRY_SIZE: usize = 64 * 1024 * 1024;

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

/// Generates `horizon-mod.toml` manifest for Horizon engine content mod compatibility.
pub fn generate_horizon_mod_toml(meta: &ModMetadata) -> String {
    let sanitized_version = sanitize_semver(&meta.version);
    let authors_toml = meta
        .authors
        .iter()
        .map(|a| format!("\"{}\"", a.replace('"', "\\\"")))
        .collect::<Vec<_>>()
        .join(", ");
    let desc = meta.description.replace('"', "\\\"");
    format!(
        "id = \"{}\"\nname = \"{}\"\nversion = \"{}\"\ndescription = \"{}\"\nauthors = [{}]\napi = \"0.2\"\nside = \"both\"\ncapabilities = [\"registries\"]\n",
        meta.id, meta.name, sanitized_version, desc, authors_toml
    )
}

/// Safely reads from an uncompressed ZIP entry stream up to `MAX_UNCOMPRESSED_ENTRY_SIZE` (64 MB).
///
/// Protects against zip bombs (decompression bombs) and memory exhaustion.
pub fn read_bounded_entry<R: Read>(reader: &mut R) -> Result<Vec<u8>> {
    read_bounded_stream(reader, MAX_UNCOMPRESSED_ENTRY_SIZE as u64)
}

/// Safely reads from an entry stream up to `limit` bytes.
///
/// Returns an error if the decompressed stream exceeds the limit.
pub fn read_bounded_stream<R: Read>(reader: &mut R, limit: u64) -> Result<Vec<u8>> {
    let mut take = Read::take(reader, limit + 1);
    let mut buf = Vec::new();
    take.read_to_end(&mut buf)?;
    if buf.len() as u64 > limit {
        return Err(crate::error::ConverterError::Other(format!(
            "Decompressed entry size exceeds maximum allowable limit of {limit} bytes (decompression bomb protection)"
        )));
    }
    Ok(buf)
}

fn write_zip_entry<W: Write + Seek>(
    zip: &mut ZipWriter<W>,
    written_entries: &mut HashSet<String>,
    options: SimpleFileOptions,
    path: &str,
    bytes: &[u8],
) -> Result<bool> {
    if bytes.len() > MAX_UNCOMPRESSED_ENTRY_SIZE {
        return Err(crate::error::ConverterError::Other(format!(
            "Entry '{path}' size ({} bytes) exceeds maximum allowable limit of 64MB (decompression bomb protection)",
            bytes.len()
        )));
    }
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

    // 2b. Write horizon-mod.toml (for Horizon engine content mod compatibility)
    let horizon_toml = generate_horizon_mod_toml(&ir.metadata);
    write_zip_entry(
        &mut zip,
        &mut written_entries,
        options,
        "horizon-mod.toml",
        horizon_toml.as_bytes(),
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

    // 5. Write blockstate JSON files in deterministic alphabetical order
    let mut blockstate_keys: Vec<_> = assets.blockstates.keys().collect();
    blockstate_keys.sort();
    for key in blockstate_keys {
        let bytes = &assets.blockstates[key];
        let zip_path = if let Some((ns, subpath)) = key.split_once(':') {
            format!("assets/{ns}/blockstates/{subpath}.json")
        } else {
            format!("assets/{default_ns}/blockstates/{key}.json")
        };
        write_zip_entry(&mut zip, &mut written_entries, options, &zip_path, bytes)?;
    }

    // 6. Write model JSON files in deterministic alphabetical order
    let mut model_keys: Vec<_> = assets.models.keys().collect();
    model_keys.sort();
    for key in model_keys {
        let bytes = &assets.models[key];
        let zip_path = if let Some((ns, subpath)) = key.split_once(':') {
            format!("assets/{ns}/models/{subpath}.json")
        } else {
            format!("assets/{default_ns}/models/{key}.json")
        };
        write_zip_entry(&mut zip, &mut written_entries, options, &zip_path, bytes)?;
    }

    // 7. Write texture PNG files in deterministic alphabetical order
    let mut texture_keys: Vec<_> = assets.textures.keys().collect();
    texture_keys.sort();
    for key in texture_keys {
        let bytes = &assets.textures[key];
        let zip_path = if let Some((ns, subpath)) = key.split_once(':') {
            format!("assets/{ns}/textures/{subpath}.png")
        } else {
            format!("assets/{default_ns}/textures/{key}.png")
        };
        write_zip_entry(&mut zip, &mut written_entries, options, &zip_path, bytes)?;
    }

    // 8. Write loot table JSON files in deterministic alphabetical order
    let mut loot_keys: Vec<_> = assets.loot_tables.keys().collect();
    loot_keys.sort();
    for key in loot_keys {
        let bytes = &assets.loot_tables[key];
        let zip_path = if let Some((ns, subpath)) = key.split_once(':') {
            format!("data/{ns}/loot_tables/{subpath}.json")
        } else {
            format!("data/{default_ns}/loot_tables/{key}.json")
        };
        write_zip_entry(&mut zip, &mut written_entries, options, &zip_path, bytes)?;
    }

    // 9. Write recipe files in deterministic order (first raw, then synthetic from IR)
    let mut raw_recipe_keys: Vec<_> = assets.raw_recipes.keys().collect();
    raw_recipe_keys.sort();
    for key in raw_recipe_keys {
        let bytes = &assets.raw_recipes[key];
        let zip_path = if let Some((ns, subpath)) = key.split_once(':') {
            format!("data/{ns}/recipes/{subpath}.json")
        } else {
            format!("data/{default_ns}/recipes/{key}.json")
        };
        write_zip_entry(&mut zip, &mut written_entries, options, &zip_path, bytes)?;
    }

    let mut sorted_recipes = ir.recipes.clone();
    sorted_recipes.sort_by(|a, b| a.id().cmp(b.id()));
    for recipe in &sorted_recipes {
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

    // 10. Write language files in deterministic order
    let mut lang_paths: Vec<_> = assets.raw_lang_files.keys().collect();
    lang_paths.sort();
    for path in lang_paths {
        let bytes = &assets.raw_lang_files[path];
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

        // 11. Write Horizon content declarations (blocks, items, creative tabs)
    for block in &ir.blocks {
        let (ns, subpath) = if let Some((ns, sub)) = block.id.split_once(':') {
            (ns, sub)
        } else {
            (default_ns.as_str(), block.id.as_str())
        };
        let zip_path = format!("data/{ns}/horizon/block/{subpath}.json");
        let block_json = r#"{"template":"minecraft:stone","item":true}"#;
        write_zip_entry(&mut zip, &mut written_entries, options, &zip_path, block_json.as_bytes())?;
    }

    for item in &ir.items {
        let (ns, subpath) = if let Some((ns, sub)) = item.id.split_once(':') {
            (ns, sub)
        } else {
            (default_ns.as_str(), item.id.as_str())
        };
        let zip_path = format!("data/{ns}/horizon/item/{subpath}.json");
        let stack_size = item.max_stack_size.clamp(1, 99);
        let item_json = format!(r#"{{"template":"minecraft:stick","max_stack_size":{stack_size}}}"#);
        write_zip_entry(&mut zip, &mut written_entries, options, &zip_path, item_json.as_bytes())?;
    }

    let mut tab_items = Vec::new();
    for b in &ir.blocks {
        let full_id = if b.id.contains(':') {
            b.id.clone()
        } else {
            format!("{default_ns}:{}", b.id)
        };
        tab_items.push(full_id);
    }
    for i in &ir.items {
        let full_id = if i.id.contains(':') {
            i.id.clone()
        } else {
            format!("{default_ns}:{}", i.id)
        };
        if !tab_items.contains(&full_id) {
            tab_items.push(full_id);
        }
    }
    let icon_id = tab_items.first().cloned().unwrap_or_else(|| format!("{default_ns}:item"));
    let tab_json = serde_json::json!({
        "title": ir.metadata.name,
        "icon": icon_id,
        "items": tab_items,
    });
    if let Ok(tab_bytes) = serde_json::to_vec_pretty(&tab_json) {
        let tab_path = format!("data/{default_ns}/horizon/creative_tab/{default_ns}.json");
        write_zip_entry(&mut zip, &mut written_entries, options, &tab_path, &tab_bytes)?;
    }

    zip.finish()?;
    // 12. Also ensure a twin .hmod package is created if saving to a .rmod path
    if output_path.extension().and_then(|e| e.to_str()) == Some("rmod") {
        let hmod_path = output_path.with_extension("hmod");
        if let Err(e) = std::fs::copy(output_path, &hmod_path) {
            log::debug!("Could not auto-create twin .hmod package: {e}");
        }
    }
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
