//! Declarative `content.json` schema definitions and serialization.

use crate::error::Result;
use crate::ir::{ContentIR, RecipeDef, ToolType};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Declarative block schema matching `content.json` specification.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SchemaBlock {
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

/// Tool efficiency and mining statistics.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SchemaToolStats {
    #[serde(rename = "type")]
    pub tool_type: ToolType,
    pub speed: f32,
    pub attack_damage: f32,
    #[serde(default, skip_serializing_if = "is_zero_u32")]
    pub mining_level: u32,
}

fn is_zero_u32(val: &u32) -> bool {
    *val == 0
}

/// Food and nutrition statistics for edible items.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SchemaFoodStats {
    pub nutrition: u32,
    pub saturation_modifier: f32,
    pub always_edible: bool,
}

/// Recipe output result item and quantity.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SchemaRecipeResult {
    pub item: String,
    pub count: u32,
}

/// Supported declarative recipes.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum SchemaRecipe {
    #[serde(rename = "crafting_shaped")]
    Shaped {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        pattern: Vec<String>,
        key: HashMap<String, String>,
        result: SchemaRecipeResult,
    },
    #[serde(rename = "crafting_shapeless")]
    Shapeless {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        ingredients: Vec<String>,
        result: SchemaRecipeResult,
    },
    #[serde(rename = "smelting")]
    Smelting {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        ingredient: String,
        result: SchemaRecipeResult,
        #[serde(default)]
        experience: f32,
        #[serde(default = "default_cooking_time")]
        cooking_time: u32,
    },
}

fn default_cooking_time() -> u32 {
    200
}

/// Declarative item schema.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SchemaItem {
    pub id: String,
    pub max_stack_size: u32,
    pub durability: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool: Option<SchemaToolStats>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub food: Option<SchemaFoodStats>,
    pub rarity: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
}

/// Root declarative `content.json` structure.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ContentSchema {
    pub schema_version: String,
    pub mod_id: String,
    pub blocks: Vec<SchemaBlock>,
    pub items: Vec<SchemaItem>,
    pub recipes: Vec<SchemaRecipe>,
}

impl ContentSchema {
    /// Creates a declarative `ContentSchema` from the unified `ContentIR`.
    pub fn from_ir(ir: &ContentIR) -> Self {
        let blocks = ir
            .blocks
            .iter()
            .map(|b| SchemaBlock {
                id: b.id.clone(),
                hardness: b.hardness,
                resistance: b.resistance,
                tool: b.tool,
                harvest_level: b.harvest_level,
                sound_group: b.sound_group.clone(),
                light_emission: b.light_emission,
                opaque: b.opaque,
                model: b.model.clone(),
            })
            .collect();

        let items = ir
            .items
            .iter()
            .map(|i| SchemaItem {
                id: i.id.clone(),
                max_stack_size: i.max_stack_size,
                durability: i.durability,
                tool: i.tool.as_ref().map(|t| SchemaToolStats {
                    tool_type: t.tool_type,
                    speed: t.speed,
                    attack_damage: t.attack_damage,
                    mining_level: t.mining_level,
                }),
                food: i.food.as_ref().map(|f| SchemaFoodStats {
                    nutrition: f.nutrition,
                    saturation_modifier: f.saturation_modifier,
                    always_edible: f.always_edible,
                }),
                rarity: i.rarity.clone(),
                model: i.model.clone(),
            })
            .collect();

        let recipes = ir
            .recipes
            .iter()
            .map(|r| match r {
                RecipeDef::Shaped {
                    id,
                    pattern,
                    key,
                    result_item,
                    result_count,
                } => SchemaRecipe::Shaped {
                    id: Some(id.clone()),
                    pattern: pattern.clone(),
                    key: key.clone(),
                    result: SchemaRecipeResult {
                        item: result_item.clone(),
                        count: *result_count,
                    },
                },
                RecipeDef::Shapeless {
                    id,
                    ingredients,
                    result_item,
                    result_count,
                } => SchemaRecipe::Shapeless {
                    id: Some(id.clone()),
                    ingredients: ingredients.clone(),
                    result: SchemaRecipeResult {
                        item: result_item.clone(),
                        count: *result_count,
                    },
                },
                RecipeDef::Smelting {
                    id,
                    ingredient,
                    result_item,
                    experience,
                    cooking_time,
                } => SchemaRecipe::Smelting {
                    id: Some(id.clone()),
                    ingredient: ingredient.clone(),
                    result: SchemaRecipeResult {
                        item: result_item.clone(),
                        count: 1,
                    },
                    experience: *experience,
                    cooking_time: *cooking_time,
                },
            })
            .collect();

        Self {
            schema_version: "1.0".to_string(),
            mod_id: ir.metadata.id.clone(),
            blocks,
            items,
            recipes,
        }
    }

    /// Serializes schema into compact JSON string.
    pub fn to_json_string(&self) -> Result<String> {
        serde_json::to_string(self).map_err(Into::into)
    }

    /// Serializes schema into formatted human-readable JSON string.
    pub fn to_json_pretty(&self) -> Result<String> {
        serde_json::to_string_pretty(self).map_err(Into::into)
    }

    /// Deserializes a schema from a JSON string.
    pub fn from_json_str(s: &str) -> Result<Self> {
        serde_json::from_str(s).map_err(Into::into)
    }
}

impl From<&ContentIR> for ContentSchema {
    fn from(ir: &ContentIR) -> Self {
        Self::from_ir(ir)
    }
}
