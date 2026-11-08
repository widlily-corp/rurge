//! Unified Intermediate Representation (IR) for Minecraft content.
//!
//! Captures blocks, items, tools, food attributes, recipes, and localized strings
//! extracted from bytecode and assets across Fabric, Forge, and legacy loaders,
//! as well as behavioral action definitions and block triggers.

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

/// Triggers that can activate an item action.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ItemTrigger {
    /// Right-click in the air or on an entity/block.
    RightClick,
    /// Right-click specifically targeting a block.
    RightClickBlock,
    /// Left-click / swing item.
    LeftClick,
    /// Attack an entity with the item.
    HitEntity,
    /// Finish eating or drinking an edible item.
    Consume,
}

/// Triggers that can activate a block action.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BlockTriggerType {
    /// Right-click / use block.
    Interact,
    /// Block destroyed or mined.
    Break,
    /// Entity or player walks onto or steps inside the block.
    StepOn,
    /// Block receives a redstone signal or change in redstone power.
    Redstone,
    /// Adjacent neighbor block state changed.
    NeighborUpdate,
    /// Random tick (growth, decay, ambient mechanics).
    RandomTick,
}

/// Pre-requisite condition that must evaluate to true before an action executes.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "condition", rename_all = "snake_case")]
pub enum ActionCondition {
    /// Checks if the player is crouching/sneaking.
    Sneaking { sneaking: bool },
    /// Checks if the player is holding a specific item.
    HoldingItem { item_id: String },
    /// Checks if the target block has a specific state property value.
    BlockProperty { property: String, value: String },
    /// Checks if the player has at least the specified health level.
    MinHealth { min_health: f32 },
}

/// Concrete effect payload executed when a block trigger or item action occurs.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum ActionEffect {
    /// Applies a status effect (e.g. "speed", "poison", "regeneration", "strength").
    ApplyEffect {
        effect: String,
        duration_ticks: u32,
        #[serde(default)]
        amplifier: u32,
    },
    /// Triggers an explosion at the target or player location.
    Explode {
        power: f32,
        #[serde(default)]
        causes_fire: bool,
        #[serde(default = "default_true")]
        break_blocks: bool,
    },
    /// Modifies block state properties or transforms the block.
    SetBlockState {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        block_id: Option<String>,
        #[serde(default, skip_serializing_if = "HashMap::is_empty")]
        properties: HashMap<String, String>,
    },
    /// Plays an audio sound event.
    PlaySound {
        sound: String,
        #[serde(default = "default_one_f32")]
        volume: f32,
        #[serde(default = "default_one_f32")]
        pitch: f32,
    },
    /// Restores player or entity health.
    Heal {
        amount: f32,
    },
    /// Deals damage to the player or entity.
    Damage {
        amount: f32,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        damage_type: Option<String>,
    },
    /// Inflicts durability damage to the held item.
    DamageItem {
        #[serde(default = "default_one_u32")]
        amount: u32,
    },
    /// Grants an item to the player inventory.
    GiveItem {
        item_id: String,
        #[serde(default = "default_one_u32")]
        count: u32,
    },
    /// Consumes a given quantity of items from the player's active hand.
    ConsumeItem {
        #[serde(default = "default_one_u32")]
        count: u32,
    },
    /// Spawns an entity in the world at the interaction site.
    SpawnEntity {
        entity_type: String,
        #[serde(default = "default_one_u32")]
        count: u32,
    },
    /// Sends a status or chat message to the invoking player.
    SendMessage {
        message: String,
    },
    /// Teleports the player by the given delta offset.
    Teleport {
        offset_x: f64,
        offset_y: f64,
        offset_z: f64,
    },
}

fn default_true() -> bool {
    true
}

fn default_one_f32() -> f32 {
    1.0
}

fn default_one_u32() -> u32 {
    1
}

/// Declarative specification of an action triggered by item use.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ItemActionDef {
    /// Optional unique identifier for this action definition.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    /// Optional target item ID (used when declared at schema root).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub item_id: Option<String>,
    /// Event trigger activating this item action.
    pub trigger: ItemTrigger,
    /// List of effects sequentially executed when triggered.
    pub actions: Vec<ActionEffect>,
    /// Cooldown period in ticks before this action can trigger again.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cooldown_ticks: Option<u32>,
    /// Optional conditions that must all pass for the action to execute.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub conditions: Vec<ActionCondition>,
}

impl ItemActionDef {
    /// Creates a new `ItemActionDef` with the given trigger and action effects.
    pub fn new(trigger: ItemTrigger, actions: Vec<ActionEffect>) -> Self {
        Self {
            id: None,
            item_id: None,
            trigger,
            actions,
            cooldown_ticks: None,
            conditions: Vec::new(),
        }
    }

    /// Sets the target item ID.
    pub fn with_item_id(mut self, item_id: impl Into<String>) -> Self {
        self.item_id = Some(item_id.into());
        self
    }

    /// Sets the action cooldown in ticks.
    pub fn with_cooldown(mut self, ticks: u32) -> Self {
        self.cooldown_ticks = Some(ticks);
        self
    }

    /// Adds a prerequisite condition.
    pub fn with_condition(mut self, condition: ActionCondition) -> Self {
        self.conditions.push(condition);
        self
    }
}

/// Declarative specification of a trigger and action executed by a block.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BlockTriggerDef {
    /// Optional unique identifier for this trigger definition.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    /// Optional target block ID (used when declared at schema root).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub block_id: Option<String>,
    /// Type of interaction or block event triggering execution.
    pub trigger: BlockTriggerType,
    /// List of effects sequentially executed when triggered.
    pub actions: Vec<ActionEffect>,
    /// Optional conditions that must all pass for the trigger to fire.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub conditions: Vec<ActionCondition>,
}

impl BlockTriggerDef {
    /// Creates a new `BlockTriggerDef` with the given trigger and action effects.
    pub fn new(trigger: BlockTriggerType, actions: Vec<ActionEffect>) -> Self {
        Self {
            id: None,
            block_id: None,
            trigger,
            actions,
            conditions: Vec::new(),
        }
    }

    /// Sets the target block ID.
    pub fn with_block_id(mut self, block_id: impl Into<String>) -> Self {
        self.block_id = Some(block_id.into());
        self
    }

    /// Adds a prerequisite condition.
    pub fn with_condition(mut self, condition: ActionCondition) -> Self {
        self.conditions.push(condition);
        self
    }
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
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub triggers: Vec<BlockTriggerDef>,
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
            triggers: Vec::new(),
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

    /// Adds a trigger to this block definition.
    pub fn with_trigger(mut self, trigger: BlockTriggerDef) -> Self {
        self.triggers.push(trigger);
        self
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
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub actions: Vec<ItemActionDef>,
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
            actions: Vec::new(),
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

    /// Adds an action definition to this item.
    pub fn with_action(mut self, action: ItemActionDef) -> Self {
        self.actions.push(action);
        self
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

/// Definition of a creative inventory tab.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CreativeTabDef {
    pub id: String,
    pub title: String,
    pub icon: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub items: Vec<String>,
}

impl CreativeTabDef {
    /// Creates a new `CreativeTabDef`.
    pub fn new(id: impl Into<String>, title: impl Into<String>, icon: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            title: title.into(),
            icon: icon.into(),
            items: Vec::new(),
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
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub item_actions: Vec<ItemActionDef>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub block_triggers: Vec<BlockTriggerDef>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub creative_tabs: Vec<CreativeTabDef>,
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
            item_actions: Vec::new(),
            block_triggers: Vec::new(),
            creative_tabs: Vec::new(),
        }
    }

    /// Adds or updates a creative tab definition by ID.
    pub fn add_creative_tab(&mut self, tab: CreativeTabDef) {
        if let Some(existing) = self.creative_tabs.iter_mut().find(|t| t.id == tab.id) {
            *existing = tab;
        } else {
            self.creative_tabs.push(tab);
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

    /// Appends a top-level item action definition.
    pub fn add_item_action(&mut self, action: ItemActionDef) {
        self.item_actions.push(action);
    }

    /// Appends a top-level block trigger definition.
    pub fn add_block_trigger(&mut self, trigger: BlockTriggerDef) {
        self.block_triggers.push(trigger);
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
