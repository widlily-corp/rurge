//! Asset extraction, normalization, and recipe parsing from mod archives.

use crate::error::{ConverterError, Result};
use crate::ir::{ContentIR, RecipeDef};
use log::warn;
use serde_json::Value;
use std::collections::HashMap;
use std::io::{Read, Seek};

/// Maximum total bytes allowed when extracting assets from an archive to prevent zip bombs (512 MB).
const MAX_TOTAL_EXTRACT_BYTES: u64 = 512 * 1024 * 1024;

/// Maximum bytes allowed for a single extracted file entry (64 MB).
const MAX_SINGLE_FILE_BYTES: u64 = 64 * 1024 * 1024;

/// Collection of raw and parsed asset buffers extracted from a mod archive.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ExtractedAssets {
    /// Normalized blockstate definitions: identifier (e.g. `modid:block_name`) -> JSON bytes.
    pub blockstates: HashMap<String, Vec<u8>>,
    /// Normalized model definitions: identifier (e.g. `modid:block/block_name`) -> JSON bytes.
    pub models: HashMap<String, Vec<u8>>,
    /// Normalized texture images: identifier (e.g. `modid:block/texture_name`) -> PNG bytes.
    pub textures: HashMap<String, Vec<u8>>,
    /// Localized translations from JSON and legacy .lang files.
    pub lang: HashMap<String, String>,
    /// Parsed standard Minecraft recipes.
    pub recipes: Vec<RecipeDef>,
    /// Raw recipe JSON file buffers: identifier (e.g. `modid:recipe_name`) -> JSON bytes.
    pub raw_recipes: HashMap<String, Vec<u8>>,
    /// Raw language JSON file buffers: path (e.g. `assets/modid/lang/en_us.json`) -> JSON bytes.
    pub raw_lang_files: HashMap<String, Vec<u8>>,
    /// Raw loot table JSON definitions.
    pub loot_tables: HashMap<String, Vec<u8>>,
}

impl ExtractedAssets {
    /// Creates a new empty `ExtractedAssets` collection.
    pub fn new() -> Self {
        Self::default()
    }

    /// Merges extracted assets, recipes, and lang into the provided `ContentIR`.
    /// Also infers missing model references on blocks and items when matching models exist.
    pub fn merge_into(&self, ir: &mut ContentIR) {
        let mod_id = ir.metadata.id.clone();

        // 1. Merge recipes
        for recipe in &self.recipes {
            ir.add_recipe(recipe.clone());
        }

        // 2. Merge language keys
        for (k, v) in &self.lang {
            ir.lang.insert(k.clone(), v.clone());
        }

        // 3. Link block models if unset
        for block in &mut ir.blocks {
            if block.model.is_none() {
                let candidate_model = format!("{mod_id}:block/{}", block.id);
                if self.models.contains_key(&candidate_model) {
                    block.model = Some(candidate_model);
                } else {
                    let direct_model = format!("{mod_id}:{}", block.id);
                    if self.models.contains_key(&direct_model) {
                        block.model = Some(direct_model);
                    }
                }
            }
        }

        // 4. Link item models if unset
        for item in &mut ir.items {
            if item.model.is_none() {
                let candidate_model = format!("{mod_id}:item/{}", item.id);
                if self.models.contains_key(&candidate_model) {
                    item.model = Some(candidate_model);
                } else {
                    let direct_model = format!("{mod_id}:{}", item.id);
                    if self.models.contains_key(&direct_model) {
                        item.model = Some(direct_model);
                    }
                }
            }
        }
    }
}

/// Parses a recipe JSON string according to standard Minecraft recipe schemas.
///
/// Supports:
/// - `minecraft:crafting_shaped` / `crafting_shaped`
/// - `minecraft:crafting_shapeless` / `crafting_shapeless`
/// - `minecraft:smelting` / `smelting` / `minecraft:blasting` / `blasting`
pub fn parse_recipe_json(recipe_id: &str, json_str: &str) -> Result<Option<RecipeDef>> {
    let root: Value = match serde_json::from_str(json_str) {
        Ok(v) => v,
        Err(e) => {
            warn!("Failed to parse recipe JSON for '{recipe_id}': {e}");
            return Ok(None);
        }
    };

    let recipe_type = root
        .get("type")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .trim();

    let normalized_type = recipe_type.strip_prefix("minecraft:").unwrap_or(recipe_type);

    match normalized_type {
        "crafting_shaped" | "forge:shaped_ore" => {
            let pattern = match root.get("pattern").and_then(|v| v.as_array()) {
                Some(arr) => arr
                    .iter()
                    .filter_map(|s| s.as_str().map(|str_val| str_val.to_string()))
                    .collect::<Vec<String>>(),
                None => return Ok(None),
            };

            let mut key_map = HashMap::new();
            if let Some(key_obj) = root.get("key").and_then(|v| v.as_object()) {
                for (k, val) in key_obj {
                    if let Some(item_id) = extract_ingredient_id(val) {
                        key_map.insert(k.clone(), item_id);
                    }
                }
            }

            let (result_item, result_count) = match root.get("result") {
                Some(res_val) => extract_result_item_and_count(res_val)?,
                None => return Ok(None),
            };

            Ok(Some(RecipeDef::Shaped {
                id: recipe_id.to_string(),
                pattern,
                key: key_map,
                result_item,
                result_count,
            }))
        }
        "crafting_shapeless" | "forge:shapeless_ore" => {
            let ingredients = match root.get("ingredients").and_then(|v| v.as_array()) {
                Some(arr) => arr
                    .iter()
                    .filter_map(extract_ingredient_id)
                    .collect::<Vec<String>>(),
                None => return Ok(None),
            };

            let (result_item, result_count) = match root.get("result") {
                Some(res_val) => extract_result_item_and_count(res_val)?,
                None => return Ok(None),
            };

            Ok(Some(RecipeDef::Shapeless {
                id: recipe_id.to_string(),
                ingredients,
                result_item,
                result_count,
            }))
        }
        "smelting" | "blasting" => {
            let ingredient = match root.get("ingredient") {
                Some(ing_val) => extract_ingredient_id(ing_val).unwrap_or_default(),
                None => return Ok(None),
            };

            let (result_item, _count) = match root.get("result") {
                Some(res_val) => extract_result_item_and_count(res_val)?,
                None => return Ok(None),
            };

            let experience = root
                .get("experience")
                .and_then(|v| v.as_f64())
                .map(|f| f as f32)
                .unwrap_or(0.0);

            let cooking_time = root
                .get("cookingtime")
                .or_else(|| root.get("cooking_time"))
                .and_then(|v| v.as_u64())
                .map(|u| u as u32)
                .unwrap_or(200);

            Ok(Some(RecipeDef::Smelting {
                id: recipe_id.to_string(),
                ingredient,
                result_item,
                experience,
                cooking_time,
            }))
        }
        _ => Ok(None),
    }
}

/// Helper extracting an item or tag identifier from a recipe ingredient node.
fn extract_ingredient_id(val: &Value) -> Option<String> {
    match val {
        Value::String(s) => Some(s.clone()),
        Value::Object(map) => {
            if let Some(item) = map.get("item").and_then(|v| v.as_str()) {
                Some(item.to_string())
            } else if let Some(tag) = map.get("tag").and_then(|v| v.as_str()) {
                Some(format!("#{tag}"))
            } else {
                map.get("id")
                    .and_then(|v| v.as_str())
                    .map(|id| id.to_string())
            }
        }
        Value::Array(arr) => arr.first().and_then(extract_ingredient_id),
        _ => None,
    }
}

/// Helper extracting (item_id, count) from a recipe result node.
fn extract_result_item_and_count(val: &Value) -> Result<(String, u32)> {
    match val {
        Value::String(s) => Ok((s.clone(), 1)),
        Value::Object(map) => {
            let item = map
                .get("item")
                .or_else(|| map.get("id"))
                .or_else(|| map.get("result"))
                .and_then(|v| v.as_str())
                .ok_or_else(|| {
                    ConverterError::Other("Recipe result missing 'item' or 'id'".into())
                })?
                .to_string();

            let count = map
                .get("count")
                .and_then(|v| v.as_u64())
                .map(|c| c as u32)
                .unwrap_or(1);

            Ok((item, count))
        }
        _ => Err(ConverterError::Other(
            "Unexpected recipe result type".into(),
        )),
    }
}

/// Parses a modern Minecraft JSON language translation file (e.g. `en_us.json`).
pub fn parse_lang_json(json_str: &str) -> Result<HashMap<String, String>> {
    let mut map = HashMap::new();
    let val: Value = serde_json::from_str(json_str)?;
    if let Some(obj) = val.as_object() {
        for (k, v) in obj {
            if let Some(s) = v.as_str() {
                map.insert(k.clone(), s.to_string());
            }
        }
    }
    Ok(map)
}

/// Parses legacy `.lang` format files (key=value pairs with `#` comments).
pub fn parse_legacy_lang(lang_str: &str) -> HashMap<String, String> {
    let mut map = HashMap::new();
    for line in lang_str.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        if let Some((k, v)) = trimmed.split_once('=') {
            map.insert(k.trim().to_string(), v.trim().to_string());
        }
    }
    map
}

/// Resolves model parent hierarchy starting from `start_model`, with a recursion depth guard.
///
/// Prevents stack overflows and infinite loops caused by circular parent references or excessive nesting.
pub fn resolve_model_parents(
    start_model: &str,
    models: &HashMap<String, Vec<u8>>,
    max_depth: usize,
) -> Result<Vec<String>> {
    let mut chain = vec![start_model.to_string()];
    let mut visited = std::collections::HashSet::new();
    visited.insert(start_model.to_string());

    let mut current = start_model.to_string();

    while chain.len() <= max_depth {
        let json_bytes = match lookup_model_bytes(&current, models) {
            Some(bytes) => bytes,
            None => {
                // Reached an external root model not in this archive (e.g. minecraft:block/cube_all)
                return Ok(chain);
            }
        };

        let parent_opt = parse_parent_from_model_bytes(json_bytes)?;
        let parent = match parent_opt {
            Some(p) if !p.is_empty() => p,
            _ => {
                // Reached top of model hierarchy
                return Ok(chain);
            }
        };

        if visited.contains(&parent) {
            return Err(ConverterError::Other(format!(
                "Circular model parent hierarchy detected: '{parent}' already visited in chain {chain:?}"
            )));
        }

        if chain.len() >= max_depth {
            return Err(ConverterError::Other(format!(
                "Model parent hierarchy exceeded max depth of {max_depth}: chain {chain:?}"
            )));
        }

        visited.insert(parent.clone());
        chain.push(parent.clone());
        current = parent;
    }

    Err(ConverterError::Other(format!(
        "Model parent hierarchy exceeded max depth of {max_depth}"
    )))
}

/// Look up model JSON bytes by flexible identifier or path key.
fn lookup_model_bytes<'a>(key: &str, models: &'a HashMap<String, Vec<u8>>) -> Option<&'a [u8]> {
    if let Some(bytes) = models.get(key) {
        return Some(bytes);
    }

    // Try stripping or adding .json
    if let Some(stripped) = key.strip_suffix(".json") {
        if let Some(bytes) = models.get(stripped) {
            return Some(bytes);
        }
    } else {
        let with_ext = format!("{key}.json");
        if let Some(bytes) = models.get(&with_ext) {
            return Some(bytes);
        }
    }

    // Try path-based lookup if key is `modid:block/name`
    if let Some((ns, path)) = key.split_once(':') {
        let as_path = format!("assets/{ns}/models/{path}.json");
        if let Some(bytes) = models.get(&as_path) {
            return Some(bytes);
        }
    }

    None
}

/// Extracts `"parent"` string property from raw JSON model bytes.
fn parse_parent_from_model_bytes(bytes: &[u8]) -> Result<Option<String>> {
    let val: Value = match serde_json::from_slice(bytes) {
        Ok(v) => v,
        Err(e) => {
            return Err(ConverterError::Other(format!(
                "Malformed model JSON: {e}"
            )));
        }
    };

    Ok(val
        .get("parent")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string()))
}

/// Safely extracts all assets, models, textures, recipes, and lang from a ZIP archive stream.
///
/// Implements strict guards against ZIP slip path traversal and ZIP bomb size exhaustion.
pub fn extract_assets_from_zip<R: Read + Seek>(
    archive: &mut zip::ZipArchive<R>,
    _mod_id: Option<&str>,
) -> Result<ExtractedAssets> {
    let mut extracted = ExtractedAssets::new();
    let mut total_extracted_bytes: u64 = 0;

    for i in 0..archive.len() {
        let mut file = archive.by_index(i)?;
        let raw_name = match file.name() {
            Ok(name) => name.to_string(),
            Err(_) => continue,
        };

        if file.is_dir() {
            continue;
        }

        // Defend against directory traversal attacks (ZIP slip)
        if raw_name.contains("..") || raw_name.starts_with('/') || raw_name.starts_with('\\') {
            warn!("Skipping suspicious archive path: {raw_name}");
            continue;
        }

        let normalized_path = raw_name.replace('\\', "/");

        // Check if this path matches any asset or data pattern we are interested in
        let is_blockstate = normalized_path.starts_with("assets/")
            && normalized_path.contains("/blockstates/")
            && normalized_path.ends_with(".json");
        let is_model = normalized_path.starts_with("assets/")
            && normalized_path.contains("/models/")
            && normalized_path.ends_with(".json");
        let is_texture = normalized_path.starts_with("assets/")
            && normalized_path.contains("/textures/")
            && normalized_path.ends_with(".png");
        let is_lang_json = normalized_path.starts_with("assets/")
            && normalized_path.contains("/lang/")
            && normalized_path.ends_with(".json");
        let is_lang_legacy = normalized_path.starts_with("assets/")
            && normalized_path.contains("/lang/")
            && normalized_path.ends_with(".lang");
        let is_recipe = normalized_path.starts_with("data/")
            && (normalized_path.contains("/recipes/") || normalized_path.contains("/recipe/"))
            && normalized_path.ends_with(".json");
        let is_loot_table = normalized_path.starts_with("data/")
            && (normalized_path.contains("/loot_tables/")
                || normalized_path.contains("/loot_table/"))
            && normalized_path.ends_with(".json");

        if !is_blockstate
            && !is_model
            && !is_texture
            && !is_lang_json
            && !is_lang_legacy
            && !is_recipe
            && !is_loot_table
        {
            continue;
        }

        // Defend against single-file zip bombs
        let uncompressed_size = file.size();
        if uncompressed_size > MAX_SINGLE_FILE_BYTES {
            warn!(
                "Skipping file exceeding max single file limit ({} bytes): {normalized_path}",
                uncompressed_size
            );
            continue;
        }

        // Defend against cumulative zip bombs
        if total_extracted_bytes.saturating_add(uncompressed_size) > MAX_TOTAL_EXTRACT_BYTES {
            return Err(ConverterError::Other(
                "Archive extraction exceeded maximum cumulative size limit (ZIP bomb defense)"
                    .into(),
            ));
        }

        let mut buf = Vec::with_capacity(uncompressed_size as usize);
        file.read_to_end(&mut buf)?;
        total_extracted_bytes += buf.len() as u64;

        if is_blockstate {
            if let Some(key) = parse_asset_key(&normalized_path, "blockstates", ".json") {
                extracted.blockstates.insert(key, buf);
            }
        } else if is_model {
            if let Some(key) = parse_asset_key(&normalized_path, "models", ".json") {
                extracted.models.insert(key, buf);
            }
        } else if is_texture {
            if let Some(key) = parse_asset_key(&normalized_path, "textures", ".png") {
                extracted.textures.insert(key, buf);
            }
        } else if is_lang_json {
            extracted.raw_lang_files.insert(normalized_path.clone(), buf.clone());
            let parsed_lang = std::str::from_utf8(&buf)
                .ok()
                .and_then(|text| parse_lang_json(text).ok());
            if let Some(lang_map) = parsed_lang {
                extracted.lang.extend(lang_map);
            }
        } else if is_lang_legacy {
            if let Ok(text) = std::str::from_utf8(&buf) {
                let lang_map = parse_legacy_lang(text);
                extracted.lang.extend(lang_map);
            }
        } else if is_recipe {
            let maybe_recipe_id = parse_data_key(&normalized_path, "recipes", ".json")
                .or_else(|| parse_data_key(&normalized_path, "recipe", ".json"));
            if let Some(recipe_id) = maybe_recipe_id {
                extracted.raw_recipes.insert(recipe_id.clone(), buf.clone());
                let parsed_recipe = std::str::from_utf8(&buf)
                    .ok()
                    .and_then(|text| parse_recipe_json(&recipe_id, text).ok().flatten());
                if let Some(recipe) = parsed_recipe {
                    extracted.recipes.push(recipe);
                }
            }
        } else if is_loot_table {
            let maybe_key = parse_data_key(&normalized_path, "loot_tables", ".json")
                .or_else(|| parse_data_key(&normalized_path, "loot_table", ".json"));
            if let Some(key) = maybe_key {
                extracted.loot_tables.insert(key, buf);
            }
        }
    }

    Ok(extracted)
}

/// Extracts a normalized asset key `<namespace>:<subpath>` from `assets/<namespace>/<folder>/<subpath><ext>`.
fn parse_asset_key(path: &str, folder: &str, ext: &str) -> Option<String> {
    let rest = path.strip_prefix("assets/")?;
    let (ns, rest) = rest.split_once('/')?;
    let folder_prefix = format!("{folder}/");
    let subpath_with_ext = rest.strip_prefix(&folder_prefix)?;
    let subpath = subpath_with_ext.strip_suffix(ext)?;
    Some(format!("{ns}:{subpath}"))
}

/// Extracts a normalized data key `<namespace>:<subpath>` from `data/<namespace>/<folder>/<subpath><ext>`.
fn parse_data_key(path: &str, folder: &str, ext: &str) -> Option<String> {
    let rest = path.strip_prefix("data/")?;
    let (ns, rest) = rest.split_once('/')?;
    let folder_prefix = format!("{folder}/");
    let subpath_with_ext = rest.strip_prefix(&folder_prefix)?;
    let subpath = subpath_with_ext.strip_suffix(ext)?;
    Some(format!("{ns}:{subpath}"))
}
