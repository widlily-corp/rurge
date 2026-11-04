//! Unified Intermediate Representation (IR) for Minecraft content.
//!
//! Captures blocks, items, tools, food attributes, recipes, and localized strings
//! extracted from bytecode and assets across Fabric, Forge, and legacy loaders.

use crate::manifest::ModMetadata;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Minecraft tool classification.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ToolType {
    Pickaxe,
    Axe,
    Shovel,
    Hoe,
    Sword,
}

impl ToolType {
    /// Attempts to determine the tool type from a class or identifier name.
    pub fn from_name(name: &str) -> Option<Self> {
        let lower = name.to_lowercase();
        if lower.contains("pickaxe") {
            Some(Self::Pickaxe)
        } else if lower.contains("axe") {
            Some(Self::Axe)
        } else if lower.contains("shovel") || lower.contains("spade") {
            Some(Self::Shovel)
        } else if lower.contains("hoe") {
            Some(Self::Hoe)
        } else if lower.contains("sword") {
            Some(Self::Sword)
        } else {
            None
        }
    }
}

/// Tool efficiency and damage statistics.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ToolStats {
    pub tool_type: ToolType,
    pub speed: f32,
    pub attack_damage: f32,
    pub mining_level: u32,
}

/// Food and nutrition statistics for edible items.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FoodStats {
    pub nutrition: u32,
    pub saturation_modifier: f32,
    pub always_edible: bool,
}

/// Declarative block definition extracted from mod registration calls and properties.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BlockDef {
    pub id: String,
    pub hardness: f32,
    pub resistance: f32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool: Option<ToolType>,
    pub harvest_level: u32,
    pub sound_group: String,
    pub light_emission: u32,
    pub opaque: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
}

impl Default for BlockDef {
    fn default() -> Self {
        Self {
            id: String::new(),
            hardness: 1.5,
            resistance: 6.0,
            tool: None,
            harvest_level: 0,
            sound_group: "stone".to_string(),
            light_emission: 0,
            opaque: true,
            model: None,
        }
    }
}

impl BlockDef {
    /// Creates a new `BlockDef` with reasonable defaults.
    pub fn new(id: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            ..Default::default()
        }
    }
}

/// Declarative item definition.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ItemDef {
    pub id: String,
    pub max_stack_size: u32,
    pub durability: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool: Option<ToolStats>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub food: Option<FoodStats>,
    pub rarity: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
}

impl Default for ItemDef {
    fn default() -> Self {
        Self {
            id: String::new(),
            max_stack_size: 64,
            durability: 0,
            tool: None,
            food: None,
            rarity: "COMMON".to_string(),
            model: None,
        }
    }
}

impl ItemDef {
    /// Creates a new `ItemDef` with reasonable defaults.
    pub fn new(id: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            ..Default::default()
        }
    }
}

/// Unified crafting and cooking recipe intermediate representation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum RecipeDef {
    #[serde(rename = "crafting_shaped")]
    Shaped {
        id: String,
        pattern: Vec<String>,
        key: HashMap<String, String>,
        result_item: String,
        result_count: u32,
    },
    #[serde(rename = "crafting_shapeless")]
    Shapeless {
        id: String,
        ingredients: Vec<String>,
        result_item: String,
        result_count: u32,
    },
    #[serde(rename = "smelting")]
    Smelting {
        id: String,
        ingredient: String,
        result_item: String,
        experience: f32,
        cooking_time: u32,
    },
}

impl RecipeDef {
    /// Returns the unique identifier of the recipe.
    pub fn id(&self) -> &str {
        match self {
            RecipeDef::Shaped { id, .. } => id,
            RecipeDef::Shapeless { id, .. } => id,
            RecipeDef::Smelting { id, .. } => id,
        }
    }
}

/// Unified content intermediate representation holding all extracted assets and registrations.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ContentIR {
    pub metadata: ModMetadata,
    pub blocks: Vec<BlockDef>,
    pub items: Vec<ItemDef>,
    pub recipes: Vec<RecipeDef>,
    pub lang: HashMap<String, String>,
}

impl ContentIR {
    /// Creates a new empty `ContentIR` for the given mod metadata.
    pub fn new(metadata: ModMetadata) -> Self {
        Self {
            metadata,
            blocks: Vec::new(),
            items: Vec::new(),
            recipes: Vec::new(),
            lang: HashMap::new(),
        }
    }

    /// Adds or updates a block definition by ID.
    pub fn add_block(&mut self, block: BlockDef) {
        if let Some(existing) = self.blocks.iter_mut().find(|b| b.id == block.id) {
            *existing = block;
        } else {
            self.blocks.push(block);
        }
    }

    /// Adds or updates an item definition by ID.
    pub fn add_item(&mut self, item: ItemDef) {
        if let Some(existing) = self.items.iter_mut().find(|i| i.id == item.id) {
            *existing = item;
        } else {
            self.items.push(item);
        }
    }

    /// Appends a recipe definition.
    pub fn add_recipe(&mut self, recipe: RecipeDef) {
        self.recipes.push(recipe);
    }

    /// Looks up a block definition by ID.
    pub fn get_block(&self, id: &str) -> Option<&BlockDef> {
        self.blocks.iter().find(|b| b.id == id)
    }

    /// Looks up an item definition by ID.
    pub fn get_item(&self, id: &str) -> Option<&ItemDef> {
        self.items.iter().find(|i| i.id == id)
    }
}
