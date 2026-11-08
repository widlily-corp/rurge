//! Declarative Content Schema runtime executor and registry store.
//!
//! Maintains in-memory registries for custom blocks, items, tools, and recipes
//! defined declaratively in `content.json` within `.rmod` mod packages.

use parking_lot::RwLock;
pub use rurge_converter::ir::{BlockTriggerDef, ItemActionDef, ToolType};
pub use rurge_converter::schema::{
    ContentSchema, SchemaBlock, SchemaFoodStats, SchemaItem, SchemaRecipe, SchemaRecipeResult,
    SchemaToolStats,
};
use std::collections::{HashMap, HashSet};

/// Internal storage for all declarative content schema definitions.
#[derive(Debug, Default)]
struct ContentStore {
    blocks: HashMap<String, SchemaBlock>,
    items: HashMap<String, SchemaItem>,
    recipes: Vec<SchemaRecipe>,
    origins: HashMap<String, String>,
    item_actions: HashMap<String, Vec<ItemActionDef>>,
    block_triggers: HashMap<String, Vec<BlockTriggerDef>>,
}

/// Thread-safe runtime executor managing declarative blocks, items, and recipes
/// extracted from converted or data-driven `.rmod` archives.
#[derive(Debug, Default)]
pub struct ContentSchemaExecutor {
    store: RwLock<ContentStore>,
}

impl ContentSchemaExecutor {
    /// Creates a new empty `ContentSchemaExecutor`.
    pub fn new() -> Self {
        Self {
            store: RwLock::new(ContentStore::default()),
        }
    }

    /// Registers a mod's declarative `ContentSchema`.
    ///
    /// Validates namespace collisions against existing registered content and
    /// assigns canonical `<mod_id>:<id>` identifiers.
    pub fn register_schema(&self, mod_id: &str, schema: &ContentSchema) -> Result<(), String> {
        let trimmed_mod_id = mod_id.trim();
        if trimmed_mod_id.is_empty() {
            return Err("Mod ID cannot be empty".to_string());
        }

        let mut store = self.store.write();

        // 1. Validate block IDs for cross-mod collisions
        for block in &schema.blocks {
            let full_id = if block.id.contains(':') {
                block.id.clone()
            } else {
                format!("{trimmed_mod_id}:{}", block.id)
            };

            if let Some(existing_mod) = store.origins.get(&full_id)
                && existing_mod != trimmed_mod_id
            {
                return Err(format!(
                    "Block ID '{full_id}' already registered by mod '{existing_mod}'"
                ));
            }
        }

        // 2. Validate item IDs for cross-mod collisions
        for item in &schema.items {
            let full_id = if item.id.contains(':') {
                item.id.clone()
            } else {
                format!("{trimmed_mod_id}:{}", item.id)
            };

            if let Some(existing_mod) = store.origins.get(&full_id)
                && existing_mod != trimmed_mod_id
            {
                return Err(format!(
                    "Item ID '{full_id}' already registered by mod '{existing_mod}'"
                ));
            }
        }

        // 3. Register blocks
        for block in &schema.blocks {
            let full_id = if block.id.contains(':') {
                block.id.clone()
            } else {
                format!("{trimmed_mod_id}:{}", block.id)
            };

            let mut normalized_block = block.clone();
            normalized_block.id = full_id.clone();

            store.origins.insert(full_id.clone(), trimmed_mod_id.to_string());
            store.blocks.insert(full_id, normalized_block);
        }

        // 4. Register items
        for item in &schema.items {
            let full_id = if item.id.contains(':') {
                item.id.clone()
            } else {
                format!("{trimmed_mod_id}:{}", item.id)
            };

            let mut normalized_item = item.clone();
            normalized_item.id = full_id.clone();

            store.origins.insert(full_id.clone(), trimmed_mod_id.to_string());
            if !item.actions.is_empty() {
                store.item_actions.entry(full_id.clone()).or_default().extend(item.actions.clone());
            }
            store.items.insert(full_id, normalized_item);
        }

        // 5. Register top-level schema item actions
        for action in &schema.item_actions {
            if let Some(ref target) = action.item_id {
                let full_id = if target.contains(':') {
                    target.clone()
                } else {
                    format!("{trimmed_mod_id}:{target}")
                };
                store.item_actions.entry(full_id).or_default().push(action.clone());
            }
        }

        // 6. Register block triggers
        for trigger in &schema.block_triggers {
            if let Some(ref target) = trigger.block_id {
                let full_id = if target.contains(':') {
                    target.clone()
                } else {
                    format!("{trimmed_mod_id}:{target}")
                };
                store.block_triggers.entry(full_id).or_default().push(trigger.clone());
            }
        }

        // 7. Register recipes
        for recipe in &schema.recipes {
            store.recipes.push(recipe.clone());
        }

        Ok(())
    }

    /// Deserializes and registers a `ContentSchema` from a JSON string.
    pub fn register_from_json_str(&self, mod_id: &str, json_str: &str) -> Result<(), String> {
        let schema = ContentSchema::from_json_str(json_str)
            .map_err(|e| format!("Failed to parse content schema JSON for mod '{mod_id}': {e}"))?;
        self.register_schema(mod_id, &schema)
    }

    /// Retrieves a block definition by its canonical full ID (or short ID suffix if unambiguous).
    pub fn get_block(&self, id: &str) -> Option<SchemaBlock> {
        let store = self.store.read();
        if let Some(block) = store.blocks.get(id) {
            return Some(block.clone());
        }

        if !id.contains(':') {
            let suffix = format!(":{id}");
            for (k, v) in &store.blocks {
                if k.ends_with(&suffix) {
                    return Some(v.clone());
                }
            }
        }

        None
    }

    /// Retrieves an item definition by its canonical full ID (or short ID suffix if unambiguous).
    pub fn get_item(&self, id: &str) -> Option<SchemaItem> {
        let store = self.store.read();
        if let Some(item) = store.items.get(id) {
            return Some(item.clone());
        }

        if !id.contains(':') {
            let suffix = format!(":{id}");
            for (k, v) in &store.items {
                if k.ends_with(&suffix) {
                    return Some(v.clone());
                }
            }
        }

        None
    }

    /// Retrieves all declarative actions registered for an item ID.
    pub fn get_item_actions(&self, id: &str) -> Vec<ItemActionDef> {
        let store = self.store.read();
        if let Some(actions) = store.item_actions.get(id) {
            return actions.clone();
        }

        if !id.contains(':') {
            let suffix = format!(":{id}");
            for (k, v) in &store.item_actions {
                if k.ends_with(&suffix) {
                    return v.clone();
                }
            }
        }

        Vec::new()
    }

    /// Retrieves all declarative triggers registered for a block ID.
    pub fn get_block_triggers(&self, id: &str) -> Vec<BlockTriggerDef> {
        let store = self.store.read();
        if let Some(triggers) = store.block_triggers.get(id) {
            return triggers.clone();
        }

        if !id.contains(':') {
            let suffix = format!(":{id}");
            for (k, v) in &store.block_triggers {
                if k.ends_with(&suffix) {
                    return v.clone();
                }
            }
        }

        Vec::new()
    }

    /// Returns all registered blocks sorted alphabetically by ID.
    pub fn all_blocks(&self) -> Vec<SchemaBlock> {
        let store = self.store.read();
        let mut list: Vec<SchemaBlock> = store.blocks.values().cloned().collect();
        list.sort_by(|a, b| a.id.cmp(&b.id));
        list
    }

    /// Returns all registered items sorted alphabetically by ID.
    pub fn all_items(&self) -> Vec<SchemaItem> {
        let store = self.store.read();
        let mut list: Vec<SchemaItem> = store.items.values().cloned().collect();
        list.sort_by(|a, b| a.id.cmp(&b.id));
        list
    }

    /// Returns a list of all registered declarative recipes.
    pub fn recipes(&self) -> Vec<SchemaRecipe> {
        self.store.read().recipes.clone()
    }

    /// Returns the total count of registered blocks across all mods.
    pub fn block_count(&self) -> usize {
        self.store.read().blocks.len()
    }

    /// Returns the total count of registered items across all mods.
    pub fn item_count(&self) -> usize {
        self.store.read().items.len()
    }

    /// Returns the total count of registered recipes across all mods.
    pub fn recipe_count(&self) -> usize {
        self.store.read().recipes.len()
    }

    /// Returns the originating mod ID for a registered block or item ID.
    pub fn origin_for(&self, id: &str) -> Option<String> {
        let store = self.store.read();
        if let Some(origin) = store.origins.get(id) {
            return Some(origin.clone());
        }

        if !id.contains(':') {
            let suffix = format!(":{id}");
            for (k, origin) in &store.origins {
                if k.ends_with(&suffix) {
                    return Some(origin.clone());
                }
            }
        }

        None
    }

    /// Returns a sorted list of unique mod IDs that have registered content.
    pub fn loaded_mods(&self) -> Vec<String> {
        let store = self.store.read();
        let set: HashSet<String> = store.origins.values().cloned().collect();
        let mut list: Vec<String> = set.into_iter().collect();
        list.sort();
        list
    }

    /// Returns all blocks belonging to a specific mod ID.
    pub fn blocks_for_mod(&self, mod_id: &str) -> Vec<SchemaBlock> {
        let store = self.store.read();
        let mut list: Vec<SchemaBlock> = store
            .blocks
            .iter()
            .filter(|(id, _)| {
                store
                    .origins
                    .get(*id)
                    .map(|o| o == mod_id)
                    .unwrap_or(false)
            })
            .map(|(_, b)| b.clone())
            .collect();
        list.sort_by(|a, b| a.id.cmp(&b.id));
        list
    }

    /// Returns all items belonging to a specific mod ID.
    pub fn items_for_mod(&self, mod_id: &str) -> Vec<SchemaItem> {
        let store = self.store.read();
        let mut list: Vec<SchemaItem> = store
            .items
            .iter()
            .filter(|(id, _)| {
                store
                    .origins
                    .get(*id)
                    .map(|o| o == mod_id)
                    .unwrap_or(false)
            })
            .map(|(_, b)| b.clone())
            .collect();
        list.sort_by(|a, b| a.id.cmp(&b.id));
        list
    }

    /// Clears all registered blocks, items, recipes, and origins.
    pub fn clear(&self) {
        let mut store = self.store.write();
        store.blocks.clear();
        store.items.clear();
        store.recipes.clear();
        store.origins.clear();
    }
}
