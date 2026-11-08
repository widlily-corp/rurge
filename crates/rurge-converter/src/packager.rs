//! Package builder for creating `.rmod` ZIP archives from normalized content and assets.

use crate::assets::ExtractedAssets;
use crate::compatibility::CompatibilityReport;
use crate::error::Result;
use crate::ir::{ContentIR, RecipeDef};
use crate::manifest::ModMetadata;
use crate::schema::{ContentSchema, SchemaRecipe, SchemaRecipeResult};
use crate::templates::TemplateEngine;
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
    // Protect against non-standard paths that violate Minecraft/Horizon path validation
    if (path.starts_with("assets/") || path.starts_with("data/"))
        && !crate::assets::is_valid_horizon_path(path)
    {
        log::warn!("Excluding invalid resource path from package: {path}");
        return Ok(false);
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

    let mut ir = ir.clone();
    let default_ns = ir.metadata.id.clone();

    // Synthesize items for models in assets.models that represent items not yet in ir.items or ir.blocks
    for key in assets.models.keys() {
        let (ns, rel) = if let Some((ns, sub)) = key.split_once(':') {
            (ns, sub)
        } else {
            (default_ns.as_str(), key.as_str())
        };
        if let Some(subpath) = rel.strip_prefix("item/") {
            let full_id = format!("{ns}:{subpath}");
            if !ir.items.iter().any(|i| i.id == full_id)
                && !ir.blocks.iter().any(|b| b.id == full_id)
            {
                let mut synth_item = crate::ir::ItemDef::new(&full_id);
                let item_info = TemplateEngine::infer_item_template(&full_id, None, None);
                synth_item.max_stack_size = item_info.max_stack_size;
                ir.items.push(synth_item);
            }
        }
    }

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
    let schema = ContentSchema::from_ir(&ir);
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

    // 7b. Write texture .png.mcmeta animation metadata files in deterministic alphabetical order
    let mut mcmeta_keys: Vec<_> = assets.texture_mcmeta.keys().collect();
    mcmeta_keys.sort();
    for key in mcmeta_keys {
        let bytes = &assets.texture_mcmeta[key];
        let zip_path = if let Some((ns, subpath)) = key.split_once(':') {
            format!("assets/{ns}/textures/{subpath}.png.mcmeta")
        } else {
            format!("assets/{default_ns}/textures/{key}.png.mcmeta")
        };
        write_zip_entry(&mut zip, &mut written_entries, options, &zip_path, bytes)?;
    }

    // 7c. Generate assets/minecraft/atlases/blocks.json to extend block/item atlas with custom texture subdirectories
    let mut extra_texture_dirs = std::collections::BTreeSet::new();
    for key in assets.textures.keys() {
        if let Some((_ns, subpath)) = key.split_once(':')
            && let Some((dir, _)) = subpath.split_once('/')
            && dir != "block" && dir != "item"
        {
            extra_texture_dirs.insert(dir.to_string());
        }
    }
    if !extra_texture_dirs.is_empty() {
        let mut sources = Vec::new();
        for dir in &extra_texture_dirs {
            sources.push(serde_json::json!({
                "type": "directory",
                "source": dir,
                "prefix": format!("{dir}/")
            }));
        }
        let atlas_json = serde_json::json!({
            "sources": sources
        });
        if let Ok(atlas_bytes) = serde_json::to_vec_pretty(&atlas_json) {
            let atlas_path = "assets/minecraft/atlases/blocks.json";
            write_zip_entry(&mut zip, &mut written_entries, options, atlas_path, &atlas_bytes)?;
        }
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

    // 10. Write language files in deterministic order, merging ir.lang into en_us.json
    let mut lang_paths: Vec<_> = assets.raw_lang_files.keys().collect();
    lang_paths.sort();
    for path in lang_paths {
        let bytes = if path.ends_with("en_us.json") {
            let mut merged_lang = ir.lang.clone();
            if let Ok(existing) = serde_json::from_slice::<std::collections::BTreeMap<String, String>>(&assets.raw_lang_files[path]) {
                for (k, v) in existing {
                    merged_lang.entry(k).or_insert(v);
                }
            }
            serde_json::to_vec_pretty(&merged_lang).unwrap_or_else(|_| assets.raw_lang_files[path].clone())
        } else {
            assets.raw_lang_files[path].clone()
        };
        write_zip_entry(&mut zip, &mut written_entries, options, path, &bytes)?;
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

    // 11. Write Horizon content declarations (blocks, items, entities, effects, creative tabs)
    let mut declared_block_subpaths = HashSet::new();
    for block in &ir.blocks {
        let (ns, subpath) = if let Some((ns, sub)) = block.id.split_once(':') {
            (ns, sub)
        } else {
            (default_ns.as_str(), block.id.as_str())
        };
        declared_block_subpaths.insert(subpath.to_string());
        let zip_path = format!("data/{ns}/horizon/block/{subpath}.json");
        let block_info = TemplateEngine::infer_block_template(&block.id, None, None);
        let block_json = serde_json::json!({
            "template": block_info.template,
            "item": true,
            "properties": {
                "hardness": block_info.hardness,
                "explosion_resistance": block_info.resistance,
                "light": block_info.light_level,
                "requires_tool": block_info.requires_tool
            }
        });
        if let Ok(bytes) = serde_json::to_vec_pretty(&block_json) {
            write_zip_entry(&mut zip, &mut written_entries, options, &zip_path, &bytes)?;
        }
    }

    for item in &ir.items {
        let (ns, subpath) = if let Some((ns, sub)) = item.id.split_once(':') {
            (ns, sub)
        } else {
            (default_ns.as_str(), item.id.as_str())
        };
        // When a block has "item": true in Horizon, its item is already declared with the block's ID.
        // Declaring a separate data/<ns>/horizon/item/<id>.json results in an 'also a block item' error.
        if declared_block_subpaths.contains(subpath) {
            continue;
        }
        let zip_path = format!("data/{ns}/horizon/item/{subpath}.json");
        let item_info = TemplateEngine::infer_item_template(&item.id, None, None);
        let effective_stack = if item.max_stack_size != 64 {
            item.max_stack_size.clamp(1, 99) as u8
        } else {
            item_info.max_stack_size.clamp(1, 99) as u8
        };
        let item_json = serde_json::json!({
            "template": item_info.template,
            "max_stack_size": effective_stack,
        });
        if let Ok(bytes) = serde_json::to_vec_pretty(&item_json) {
            write_zip_entry(&mut zip, &mut written_entries, options, &zip_path, &bytes)?;
        }
    }

    // Gather all block and item IDs
    let mut block_ids = Vec::new();
    for b in &ir.blocks {
        let full_id = if b.id.contains(':') {
            b.id.clone()
        } else {
            format!("{default_ns}:{}", b.id)
        };
        if !block_ids.contains(&full_id) {
            block_ids.push(full_id);
        }
    }

    let mut item_ids = Vec::new();
    for i in &ir.items {
        let full_id = if i.id.contains(':') {
            i.id.clone()
        } else {
            format!("{default_ns}:{}", i.id)
        };
        if !block_ids.contains(&full_id) && !item_ids.contains(&full_id) {
            item_ids.push(full_id);
        }
    }

    // 11b. Write Horizon entity types (data/<ns>/horizon/entity_type/<name>.json)
    let mut used_entity_items = HashSet::new();
    for entity in &ir.entity_types {
        let (ns, subpath) = if let Some((ns, sub)) = entity.id.split_once(':') {
            (ns, sub)
        } else {
            (default_ns.as_str(), entity.id.as_str())
        };

        // Strict engine constraint: item must belong to this mod and each entity type must have a unique item
        let bound_item = item_ids.iter().find(|id| {
            let item_sub = id.split(':').next_back().unwrap_or(id);
            (item_sub == subpath || item_sub.contains(subpath) || subpath.contains(item_sub))
                && !used_entity_items.contains(*id)
        }).cloned().or_else(|| {
            item_ids.iter().find(|id| !used_entity_items.contains(*id)).cloned()
        });

        if let Some(item_id) = bound_item {
            used_entity_items.insert(item_id.clone());
            let zip_path = format!("data/{ns}/horizon/entity_type/{subpath}.json");
            let entity_json = serde_json::json!({
                "template": "minecraft:snowball",
                "item": item_id,
                "damage": 4.0
            });
            if let Ok(bytes) = serde_json::to_vec_pretty(&entity_json) {
                write_zip_entry(&mut zip, &mut written_entries, options, &zip_path, &bytes)?;
            }
        }
    }

    // 11c. Write Horizon mob effects (data/<ns>/horizon/mob_effect/<name>.json)
    for effect in &ir.mob_effects {
        let (ns, subpath) = if let Some((ns, sub)) = effect.id.split_once(':') {
            (ns, sub)
        } else {
            (default_ns.as_str(), effect.id.as_str())
        };
        let cat = match effect.category.to_lowercase().as_str() {
            "beneficial" => "beneficial",
            "harmful" => "harmful",
            _ => "neutral",
        };
        let color = effect.color.min(0xFFFFFF);
        let zip_path = format!("data/{ns}/horizon/mob_effect/{subpath}.json");
        let effect_json = serde_json::json!({
            "category": cat,
            "color": color
        });
        if let Ok(bytes) = serde_json::to_vec_pretty(&effect_json) {
            write_zip_entry(&mut zip, &mut written_entries, options, &zip_path, &bytes)?;
        }
    }

    // 11d. Write Horizon menu types (data/<ns>/horizon/menu/<name>.json)
    if default_ns == "ebwizardry" || ir.blocks.iter().any(|b| b.id.contains("workbench")) {
        let menu_path = format!("data/{default_ns}/horizon/menu/arcane_workbench.json");
        let menu_json = serde_json::json!({
            "slots": 10,
            "data_slots": 0
        });
        if let Ok(bytes) = serde_json::to_vec_pretty(&menu_json) {
            write_zip_entry(&mut zip, &mut written_entries, options, &menu_path, &bytes)?;
        }
    }

    // Check if the mod defines creative tabs or if large item count requires semantic partitioning
    let has_wizardry_tabs = ir.creative_tabs.iter().any(|t| t.id.contains("wands") || t.id.contains("armors"))
        || ir.lang.keys().any(|k| k.contains("ebwizardry_wands") || k.contains("ebwizardry_armors"))
        || (item_ids.len() + block_ids.len()) > 45;

    if has_wizardry_tabs {
        // Build all 6 specialized tabs:
        // 1. Blocks
        let blocks_icon = block_ids.iter().find(|id| id.contains("crystal_block_magic"))
            .cloned().or_else(|| block_ids.first().cloned())
            .unwrap_or_else(|| format!("{default_ns}:block"));
        let tab_blocks = serde_json::json!({
            "title": format!("creativetab.{default_ns}_blocks"),
            "icon": blocks_icon,
            "items": block_ids,
        });
        if let Ok(bytes) = serde_json::to_vec_pretty(&tab_blocks) {
            let path = format!("data/{default_ns}/horizon/creative_tab/{default_ns}_blocks.json");
            write_zip_entry(&mut zip, &mut written_entries, options, &path, &bytes)?;
        }

        // 2. Wands
        let wands: Vec<_> = item_ids.iter().filter(|id| id.contains("wand")).cloned().collect();
        let wands_icon = wands.iter().find(|id| id.ends_with(":wand_novice") || id.contains("wand_novice"))
            .cloned().or_else(|| wands.first().cloned())
            .unwrap_or_else(|| format!("{default_ns}:item"));
        if !wands.is_empty() {
            let tab_wands = serde_json::json!({
                "title": format!("creativetab.{default_ns}_wands"),
                "icon": wands_icon,
                "items": wands,
            });
            if let Ok(bytes) = serde_json::to_vec_pretty(&tab_wands) {
                let path = format!("data/{default_ns}/horizon/creative_tab/{default_ns}_wands.json");
                write_zip_entry(&mut zip, &mut written_entries, options, &path, &bytes)?;
            }
        }

        // 3. Armors
        let armors: Vec<_> = item_ids.iter().filter(|id| {
            let s = id.as_str();
            s.contains("wizard_hat") || s.contains("wizard_robe") || s.contains("wizard_leggings")
                || s.contains("wizard_boots") || s.contains("sage_") || s.contains("battlemage_")
                || s.contains("warlock_") || s.contains("hat") || s.contains("robe")
                || s.contains("leggings") || s.contains("boots") || s.contains("hood")
        }).cloned().collect();
        let armors_icon = armors.iter().find(|id| id.ends_with(":wizard_hat") || id.contains("wizard_hat"))
            .cloned().or_else(|| armors.first().cloned())
            .unwrap_or_else(|| format!("{default_ns}:item"));
        if !armors.is_empty() {
            let tab_armors = serde_json::json!({
                "title": format!("creativetab.{default_ns}_armors"),
                "icon": armors_icon,
                "items": armors,
            });
            if let Ok(bytes) = serde_json::to_vec_pretty(&tab_armors) {
                let path = format!("data/{default_ns}/horizon/creative_tab/{default_ns}_armors.json");
                write_zip_entry(&mut zip, &mut written_entries, options, &path, &bytes)?;
            }
        }

        // 4. Artifacts
        let artifacts: Vec<_> = item_ids.iter().filter(|id| {
            let s = id.as_str();
            s.contains("ring_") || s.contains("amulet_") || s.contains("charm_")
        }).cloned().collect();
        let artifacts_icon = artifacts.iter().find(|id| id.ends_with(":ring_condensing") || id.contains("ring_condensing"))
            .cloned().or_else(|| artifacts.first().cloned())
            .unwrap_or_else(|| format!("{default_ns}:item"));
        if !artifacts.is_empty() {
            let tab_artifacts = serde_json::json!({
                "title": format!("creativetab.{default_ns}_artifacts"),
                "icon": artifacts_icon,
                "items": artifacts,
            });
            if let Ok(bytes) = serde_json::to_vec_pretty(&tab_artifacts) {
                let path = format!("data/{default_ns}/horizon/creative_tab/{default_ns}_artifacts.json");
                write_zip_entry(&mut zip, &mut written_entries, options, &path, &bytes)?;
            }
        }

        // 5. Spells
        let spells: Vec<_> = item_ids.iter().filter(|id| {
            let s = id.as_str();
            (s.contains("scroll") || s.contains("spell_book") || s.contains("tome"))
                && !s.contains("special") && !s.contains("cloning")
        }).cloned().collect();
        let spells_icon = spells.iter().find(|id| id.ends_with(":spell_book") || id.contains("spell_book"))
            .cloned().or_else(|| spells.first().cloned())
            .unwrap_or_else(|| format!("{default_ns}:item"));
        if !spells.is_empty() {
            let tab_spells = serde_json::json!({
                "title": format!("creativetab.{default_ns}_spells"),
                "icon": spells_icon,
                "items": spells,
            });
            if let Ok(bytes) = serde_json::to_vec_pretty(&tab_spells) {
                let path = format!("data/{default_ns}/horizon/creative_tab/{default_ns}_spells.json");
                write_zip_entry(&mut zip, &mut written_entries, options, &path, &bytes)?;
            }
        }

        // 6. General / Main Items
        let general: Vec<_> = item_ids.iter().filter(|id| {
            !wands.contains(id) && !armors.contains(id) && !artifacts.contains(id) && !spells.contains(id)
        }).cloned().collect();
        let general_icon = general.iter().find(|id| id.ends_with(":magic_crystal") || id.contains("magic_crystal"))
            .cloned().or_else(|| general.first().cloned())
            .unwrap_or_else(|| format!("{default_ns}:item"));
        let tab_general = serde_json::json!({
            "title": format!("creativetab.{default_ns}"),
            "icon": general_icon,
            "items": general,
        });
        if let Ok(bytes) = serde_json::to_vec_pretty(&tab_general) {
            let path = format!("data/{default_ns}/horizon/creative_tab/{default_ns}.json");
            write_zip_entry(&mut zip, &mut written_entries, options, &path, &bytes)?;
        }
    } else if !ir.creative_tabs.is_empty() {
        for tab in &ir.creative_tabs {
            let tab_subpath = tab.id.split(':').next_back().unwrap_or(&tab.id);
            let tab_items: Vec<String> = if !tab.items.is_empty() {
                tab.items.clone()
            } else {
                let matched: Vec<_> = item_ids.iter().chain(block_ids.iter()).filter(|id| {
                    let sub = id.split(':').next_back().unwrap_or(id);
                    sub.contains(tab_subpath) || tab_subpath.contains(sub)
                }).cloned().collect();
                if !matched.is_empty() {
                    matched
                } else {
                    item_ids.iter().chain(block_ids.iter()).cloned().collect()
                }
            };
            let icon_id = if item_ids.contains(&tab.icon) || block_ids.contains(&tab.icon) {
                tab.icon.clone()
            } else {
                tab_items.first().cloned().unwrap_or_else(|| format!("{default_ns}:item"))
            };
            let tab_json = serde_json::json!({
                "title": tab.title,
                "icon": icon_id,
                "items": tab_items,
            });
            if let Ok(tab_bytes) = serde_json::to_vec_pretty(&tab_json) {
                let tab_path = format!("data/{default_ns}/horizon/creative_tab/{tab_subpath}.json");
                write_zip_entry(&mut zip, &mut written_entries, options, &tab_path, &tab_bytes)?;
            }
        }
    } else {
        // Single default tab containing all blocks and items
        let mut tab_items = block_ids.clone();
        for id in &item_ids {
            if !tab_items.contains(id) {
                tab_items.push(id.clone());
            }
        }
        let icon_id = tab_items.first().cloned().unwrap_or_else(|| format!("{default_ns}:item"));
        let tab_json = serde_json::json!({
            "title": format!("creativetab.{default_ns}"),
            "icon": icon_id,
            "items": tab_items,
        });
        if let Ok(tab_bytes) = serde_json::to_vec_pretty(&tab_json) {
            let tab_path = format!("data/{default_ns}/horizon/creative_tab/{default_ns}.json");
            write_zip_entry(&mut zip, &mut written_entries, options, &tab_path, &tab_bytes)?;
        }
    }

    // 12. Write 1.21.4+ client item definitions (assets/<ns>/items/<name>.json)
    // Minecraft 1.21.4+ and Horizon/Rurge decouple item rendering from legacy raw JSON item models
    // into client item definition trees in assets/<ns>/items/*.json.
    let mut all_item_keys = Vec::new();
    for b in &ir.blocks {
        let (ns, subpath) = if let Some((ns, sub)) = b.id.split_once(':') {
            (ns, sub)
        } else {
            (default_ns.as_str(), b.id.as_str())
        };
        all_item_keys.push((ns.to_string(), subpath.to_string(), true));
    }
    for i in &ir.items {
        let (ns, subpath) = if let Some((ns, sub)) = i.id.split_once(':') {
            (ns, sub)
        } else {
            (default_ns.as_str(), i.id.as_str())
        };
        if !all_item_keys.iter().any(|(n, s, _)| n == ns && s == subpath) {
            all_item_keys.push((ns.to_string(), subpath.to_string(), false));
        }
    }

    // Also pick up any model files in assets.models that represent items
    for key in assets.models.keys() {
        let (ns, rel) = if let Some((ns, sub)) = key.split_once(':') {
            (ns, sub)
        } else {
            (default_ns.as_str(), key.as_str())
        };
        if let Some(subpath) = rel.strip_prefix("item/")
            && !all_item_keys.iter().any(|(n, s, _)| n == ns && s == subpath)
        {
            all_item_keys.push((ns.to_string(), subpath.to_string(), false));
        }
    }

    all_item_keys.sort();
    for (ns, subpath, is_block) in all_item_keys {
        let def_zip_path = format!("assets/{ns}/items/{subpath}.json");
        if !written_entries.contains(&def_zip_path) {
            let has_item_model = assets.models.contains_key(&format!("{ns}:item/{subpath}"))
                || assets.models.contains_key(&format!("item/{subpath}"));
            let has_block_model = assets.models.contains_key(&format!("{ns}:block/{subpath}"))
                || assets.models.contains_key(&format!("block/{subpath}"));

            let model_target = if subpath.starts_with("spell_book_") {
                format!("{ns}:item/spell_book")
            } else if subpath.starts_with("scroll_") {
                format!("{ns}:item/scroll")
            } else if has_item_model {
                format!("{ns}:item/{subpath}")
            } else if has_block_model || is_block {
                format!("{ns}:block/{subpath}")
            } else {
                format!("{ns}:item/{subpath}")
            };

            let client_item_json = serde_json::json!({
                "model": {
                    "type": "minecraft:model",
                    "model": model_target,
                }
            });
            if let Ok(def_bytes) = serde_json::to_vec_pretty(&client_item_json) {
                write_zip_entry(&mut zip, &mut written_entries, options, &def_zip_path, &def_bytes)?;
            }
        }
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
