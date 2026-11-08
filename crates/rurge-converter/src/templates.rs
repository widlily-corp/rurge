//! Semantic template deduction engine for Minecraft items and blocks.
//!
//! Maps items and blocks to appropriate vanilla templates and realistic default attributes
//! (durability, attack damage, speed, defense, hardness, resistance, light levels) based on
//! identifier naming semantics and JVM class hierarchy analysis.

use crate::bytecode::hierarchy::ClassHierarchy;
use serde::{Deserialize, Serialize};

/// Deduced template and attributes for an item.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ItemTemplateInfo {
    /// Vanilla template item ID (e.g. `"minecraft:iron_sword"`, `"minecraft:iron_chestplate"`).
    pub template: String,
    /// Maximum stack size (1 for tools/armor/weapons, 16 for projectiles/pearls, 64 for standard items).
    pub max_stack_size: u32,
    /// Target equipment slot (`"head"`, `"chest"`, `"legs"`, `"feet"`, `"mainhand"`, `"offhand"`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub equipment_slot: Option<String>,
    /// Armor protection defense points (e.g. helmet 2-3, chestplate 6-8, leggings 5-6, boots 2-3).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub armor_defense: Option<f32>,
    /// Attack damage points (e.g. sword 6-8, axe 7-9).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub attack_damage: Option<f32>,
    /// Attack speed modifier.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub attack_speed: Option<f32>,
    /// Item durability (e.g. 250..1561 for tools/weapons/armor/wands).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub durability: Option<u32>,
}

/// Deduced template and attributes for a block.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BlockTemplateInfo {
    /// Vanilla template block ID (e.g. `"minecraft:stone"`, `"minecraft:oak_planks"`, `"minecraft:iron_block"`).
    pub template: String,
    /// Block destroy time / hardness.
    pub hardness: f32,
    /// Blast resistance.
    pub resistance: f32,
    /// Emitted light level (0..15).
    pub light_level: u8,
    /// Whether the block requires an appropriate tool to drop items.
    pub requires_tool: bool,
}

#[derive(Debug, Clone, Copy)]
struct MaterialTier {
    durability_mult: f32,
    damage_bonus: f32,
    defense_bonus: f32,
    armor_durability_mult: f32,
}

fn detect_tier(s: &str) -> Option<MaterialTier> {
    let lower = s.to_lowercase();
    if lower.contains("netherite") {
        Some(MaterialTier {
            durability_mult: 8.124, // 250 * 8.124 ~ 2031
            damage_bonus: 2.0,
            defense_bonus: 1.0,
            armor_durability_mult: 2.0,
        })
    } else if lower.contains("diamond")
        || lower.contains("ruby")
        || lower.contains("sapphire")
        || lower.contains("emerald")
    {
        Some(MaterialTier {
            durability_mult: 6.244, // 250 * 6.244 ~ 1561
            damage_bonus: 1.0,
            defense_bonus: 1.0,
            armor_durability_mult: 1.8,
        })
    } else if lower.contains("gold") || lower.contains("gilded") {
        Some(MaterialTier {
            durability_mult: 0.128, // 250 * 0.128 ~ 32
            damage_bonus: -1.0,
            defense_bonus: -1.0,
            armor_durability_mult: 0.4,
        })
    } else if lower.contains("steel") || lower.contains("titanium") {
        Some(MaterialTier {
            durability_mult: 3.0, // 250 * 3.0 = 750
            damage_bonus: 1.0,
            defense_bonus: 0.5,
            armor_durability_mult: 1.4,
        })
    } else if lower.contains("copper") || lower.contains("bronze") {
        Some(MaterialTier {
            durability_mult: 0.75, // ~187
            damage_bonus: -0.5,
            defense_bonus: -0.5,
            armor_durability_mult: 0.8,
        })
    } else if lower.contains("stone") {
        Some(MaterialTier {
            durability_mult: 0.524, // ~131
            damage_bonus: -1.0,
            defense_bonus: -1.0,
            armor_durability_mult: 0.6,
        })
    } else if lower.contains("wood") {
        Some(MaterialTier {
            durability_mult: 0.236, // ~59
            damage_bonus: -2.0,
            defense_bonus: -1.0,
            armor_durability_mult: 0.3,
        })
    } else {
        None
    }
}

/// Semantic inference engine for Minecraft item and block properties.
#[derive(Debug, Clone, Default)]
pub struct TemplateEngine<'a> {
    hierarchy: Option<&'a ClassHierarchy>,
}

impl<'a> TemplateEngine<'a> {
    /// Creates a new template engine without hierarchy context.
    pub fn new() -> Self {
        Self { hierarchy: None }
    }

    /// Creates a new template engine bound to a class hierarchy.
    pub fn with_hierarchy(hierarchy: &'a ClassHierarchy) -> Self {
        Self {
            hierarchy: Some(hierarchy),
        }
    }

    /// Infers item template attributes directly without instantiating an engine.
    pub fn infer_item_template(
        id: &str,
        class_name: Option<&str>,
        hierarchy: Option<&'a ClassHierarchy>,
    ) -> ItemTemplateInfo {
        let engine = Self { hierarchy };
        engine.infer_item(id, class_name)
    }

    /// Infers block template attributes directly without instantiating an engine.
    pub fn infer_block_template(
        id: &str,
        class_name: Option<&str>,
        hierarchy: Option<&'a ClassHierarchy>,
    ) -> BlockTemplateInfo {
        let engine = Self { hierarchy };
        engine.infer_block(id, class_name)
    }

    /// Infers semantic attributes and base template for an item.
    pub fn infer_item(&self, id: &str, class_name: Option<&str>) -> ItemTemplateInfo {
        let clean_id = id.split(':').next_back().unwrap_or(id).to_lowercase();
        let clean_class = class_name
            .map(|c| c.rsplit('/').next().unwrap_or(c).to_lowercase())
            .unwrap_or_default();
        let combined = format!("{clean_id} {clean_class}");

        let tier = detect_tier(&combined);

        // 1. Sword / Blade / Dagger / Katana
        let is_sword = self
            .hierarchy
            .is_some_and(|h| class_name.is_some_and(|c| h.is_sword(c)))
            || combined.contains("sword")
            || combined.contains("blade")
            || combined.contains("katana")
            || combined.contains("saber")
            || combined.contains("rapier")
            || combined.contains("scythe")
            || combined.contains("dagger");

        if is_sword {
            let base_damage = 6.0 + tier.map_or(0.0, |t| t.damage_bonus);
            let base_durability = (250.0 * tier.map_or(1.0, |t| t.durability_mult)).round() as u32;
            return ItemTemplateInfo {
                template: "minecraft:iron_sword".to_string(),
                max_stack_size: 1,
                equipment_slot: Some("mainhand".to_string()),
                armor_defense: None,
                attack_damage: Some(base_damage),
                attack_speed: Some(1.6),
                durability: Some(base_durability),
            };
        }

        // 2. Axe / Battleaxe / Hatchet
        let is_axe = self
            .hierarchy
            .is_some_and(|h| class_name.is_some_and(|c| h.is_axe(c)))
            || (combined.contains("axe") && !combined.contains("pickaxe"))
            || combined.contains("battleaxe")
            || combined.contains("hatchet")
            || combined.contains("tomahawk");

        if is_axe {
            let base_damage = 9.0 + tier.map_or(0.0, |t| t.damage_bonus);
            let base_durability = (250.0 * tier.map_or(1.0, |t| t.durability_mult)).round() as u32;
            return ItemTemplateInfo {
                template: "minecraft:iron_axe".to_string(),
                max_stack_size: 1,
                equipment_slot: Some("mainhand".to_string()),
                armor_defense: None,
                attack_damage: Some(base_damage),
                attack_speed: Some(0.9),
                durability: Some(base_durability),
            };
        }

        // 3. Pickaxe / Mattock
        let is_pickaxe = self
            .hierarchy
            .is_some_and(|h| class_name.is_some_and(|c| h.is_pickaxe(c)))
            || combined.contains("pickaxe")
            || combined.contains("mattock");

        if is_pickaxe {
            let base_damage = 4.0 + tier.map_or(0.0, |t| t.damage_bonus);
            let base_durability = (250.0 * tier.map_or(1.0, |t| t.durability_mult)).round() as u32;
            return ItemTemplateInfo {
                template: "minecraft:iron_pickaxe".to_string(),
                max_stack_size: 1,
                equipment_slot: Some("mainhand".to_string()),
                armor_defense: None,
                attack_damage: Some(base_damage),
                attack_speed: Some(1.2),
                durability: Some(base_durability),
            };
        }

        // 4. Shovel / Spade
        let is_shovel = self
            .hierarchy
            .is_some_and(|h| class_name.is_some_and(|c| h.is_subclass_of(c, "ShovelItem")))
            || combined.contains("shovel")
            || combined.contains("spade");

        if is_shovel {
            let base_damage = 4.5 + tier.map_or(0.0, |t| t.damage_bonus);
            let base_durability = (250.0 * tier.map_or(1.0, |t| t.durability_mult)).round() as u32;
            return ItemTemplateInfo {
                template: "minecraft:iron_shovel".to_string(),
                max_stack_size: 1,
                equipment_slot: Some("mainhand".to_string()),
                armor_defense: None,
                attack_damage: Some(base_damage),
                attack_speed: Some(1.0),
                durability: Some(base_durability),
            };
        }

        // 5. Hoe
        let is_hoe = self
            .hierarchy
            .is_some_and(|h| class_name.is_some_and(|c| h.is_subclass_of(c, "HoeItem")))
            || combined.contains("hoe");

        if is_hoe {
            let base_damage = 1.0;
            let base_durability = (250.0 * tier.map_or(1.0, |t| t.durability_mult)).round() as u32;
            return ItemTemplateInfo {
                template: "minecraft:iron_hoe".to_string(),
                max_stack_size: 1,
                equipment_slot: Some("mainhand".to_string()),
                armor_defense: None,
                attack_damage: Some(base_damage),
                attack_speed: Some(2.0),
                durability: Some(base_durability),
            };
        }

        // 6. Armor (Helmet, Chestplate, Leggings, Boots)
        let is_armor = self
            .hierarchy
            .is_some_and(|h| class_name.is_some_and(|c| h.is_armor(c)))
            || combined.contains("helmet")
            || combined.contains("chestplate")
            || combined.contains("leggings")
            || combined.contains("boots")
            || combined.contains("armor")
            || combined.contains("crown")
            || combined.contains("cap")
            || combined.contains("hat")
            || combined.contains("hood")
            || combined.contains("cowl")
            || combined.contains("circlet")
            || combined.contains("mask")
            || combined.contains("tunic")
            || combined.contains("robe")
            || combined.contains("cuirass")
            || combined.contains("pants")
            || combined.contains("greaves")
            || combined.contains("sabatons");

        if is_armor {
            let (template, slot, base_def, base_dur) = if combined.contains("helmet")
                || combined.contains("hat")
                || combined.contains("cap")
                || combined.contains("crown")
                || combined.contains("hood")
                || combined.contains("cowl")
                || combined.contains("circlet")
                || combined.contains("mask")
                || combined.contains("head")
            {
                ("minecraft:iron_helmet", "head", 2.0, 165.0)
            } else if combined.contains("legging")
                || combined.contains("pants")
                || combined.contains("trousers")
                || combined.contains("greaves")
                || combined.contains("kilt")
                || combined.contains("legs")
            {
                ("minecraft:iron_leggings", "legs", 5.0, 225.0)
            } else if combined.contains("boot")
                || combined.contains("shoe")
                || combined.contains("feet")
                || combined.contains("sabatons")
            {
                ("minecraft:iron_boots", "feet", 2.0, 195.0)
            } else {
                // Chestplate / Tunic / Default armor
                ("minecraft:iron_chestplate", "chest", 6.0, 240.0)
            };

            let defense = (base_def + tier.map_or(0.0, |t| t.defense_bonus)).max(1.0);
            let durability =
                (base_dur * tier.map_or(1.0, |t| t.armor_durability_mult)).round() as u32;

            return ItemTemplateInfo {
                template: template.to_string(),
                max_stack_size: 1,
                equipment_slot: Some(slot.to_string()),
                armor_defense: Some(defense),
                attack_damage: None,
                attack_speed: None,
                durability: Some(durability),
            };
        }

        // 7. Bow / Crossbow
        let is_bow = self
            .hierarchy
            .is_some_and(|h| class_name.is_some_and(|c| h.is_bow(c)))
            || combined.contains("bow")
            || combined.contains("crossbow");

        if is_bow {
            if combined.contains("crossbow") {
                return ItemTemplateInfo {
                    template: "minecraft:crossbow".to_string(),
                    max_stack_size: 1,
                    equipment_slot: Some("mainhand".to_string()),
                    armor_defense: None,
                    attack_damage: Some(9.0),
                    attack_speed: None,
                    durability: Some(465),
                };
            } else {
                return ItemTemplateInfo {
                    template: "minecraft:bow".to_string(),
                    max_stack_size: 1,
                    equipment_slot: Some("mainhand".to_string()),
                    armor_defense: None,
                    attack_damage: Some(6.0),
                    attack_speed: None,
                    durability: Some(384),
                };
            }
        }

        // 8. Shield
        let is_shield = self
            .hierarchy
            .is_some_and(|h| class_name.is_some_and(|c| h.is_subclass_of(c, "ShieldItem")))
            || combined.contains("shield");

        if is_shield {
            return ItemTemplateInfo {
                template: "minecraft:shield".to_string(),
                max_stack_size: 1,
                equipment_slot: Some("offhand".to_string()),
                armor_defense: None,
                attack_damage: None,
                attack_speed: None,
                durability: Some(336),
            };
        }

        // 9. Projectile / Ammunition / Pearls
        let is_projectile = self
            .hierarchy
            .is_some_and(|h| class_name.is_some_and(|c| h.is_projectile(c)))
            || combined.contains("arrow")
            || combined.contains("bolt")
            || combined.contains("projectile")
            || combined.contains("pearl")
            || combined.contains("snowball")
            || combined.contains("bullet");

        if is_projectile {
            let (template, stack_size) = if combined.contains("pearl") {
                ("minecraft:ender_pearl", 16)
            } else if combined.contains("snowball") {
                ("minecraft:snowball", 16)
            } else if combined.contains("arrow") || combined.contains("bolt") {
                ("minecraft:arrow", 64)
            } else {
                ("minecraft:snowball", 16)
            };

            return ItemTemplateInfo {
                template: template.to_string(),
                max_stack_size: stack_size,
                equipment_slot: None,
                armor_defense: None,
                attack_damage: None,
                attack_speed: None,
                durability: None,
            };
        }

        // 10. Potion / Elixir / Flask / Bottle
        let is_potion = self
            .hierarchy
            .is_some_and(|h| class_name.is_some_and(|c| h.is_subclass_of(c, "PotionItem")))
            || combined.contains("potion")
            || combined.contains("flask")
            || combined.contains("elixir")
            || combined.contains("vial")
            || combined.contains("bottle")
            || combined.contains("brew");

        if is_potion {
            let stack = if combined.contains("empty") || combined.contains("bottle") {
                16
            } else {
                1
            };
            return ItemTemplateInfo {
                template: "minecraft:potion".to_string(),
                max_stack_size: stack,
                equipment_slot: None,
                armor_defense: None,
                attack_damage: None,
                attack_speed: None,
                durability: None,
            };
        }

        // 11. Wand / Staff / Scepter / Stick
        let is_wand = combined.contains("wand")
            || combined.contains("staff")
            || combined.contains("scepter")
            || combined.contains("rod")
            || combined.contains("stick");

        if is_wand {
            let is_weapon_wand = combined.contains("wand")
                || combined.contains("staff")
                || combined.contains("scepter");

            if is_weapon_wand {
                let durability = (300.0 * tier.map_or(1.0, |t| t.durability_mult)).round() as u32;
                return ItemTemplateInfo {
                    template: "minecraft:stick".to_string(),
                    max_stack_size: 1,
                    equipment_slot: Some("mainhand".to_string()),
                    armor_defense: None,
                    attack_damage: Some(4.0),
                    attack_speed: Some(1.2),
                    durability: Some(durability),
                };
            } else {
                return ItemTemplateInfo {
                    template: "minecraft:stick".to_string(),
                    max_stack_size: 64,
                    equipment_slot: None,
                    armor_defense: None,
                    attack_damage: None,
                    attack_speed: None,
                    durability: None,
                };
            }
        }

        // 11b. Spells, Books, Scrolls, Flasks, Bombs & Artifacts
        if combined.contains("scroll") {
            return ItemTemplateInfo {
                template: "minecraft:paper".to_string(),
                max_stack_size: 16,
                equipment_slot: None,
                armor_defense: None,
                attack_damage: None,
                attack_speed: None,
                durability: None,
            };
        }

        if combined.contains("spell_book") || combined.contains("tome") || combined.contains("handbook") || combined.contains("book") {
            return ItemTemplateInfo {
                template: "minecraft:enchanted_book".to_string(),
                max_stack_size: 1,
                equipment_slot: None,
                armor_defense: None,
                attack_damage: None,
                attack_speed: None,
                durability: None,
            };
        }

        if combined.contains("flask") || combined.contains("elixir") || combined.contains("potion") {
            return ItemTemplateInfo {
                template: "minecraft:potion".to_string(),
                max_stack_size: 16,
                equipment_slot: None,
                armor_defense: None,
                attack_damage: None,
                attack_speed: None,
                durability: None,
            };
        }

        if combined.contains("bomb") {
            return ItemTemplateInfo {
                template: "minecraft:snowball".to_string(),
                max_stack_size: 16,
                equipment_slot: None,
                armor_defense: None,
                attack_damage: None,
                attack_speed: None,
                durability: None,
            };
        }

        if combined.contains("ring") || combined.contains("amulet") || combined.contains("charm") || combined.contains("bauble") {
            return ItemTemplateInfo {
                template: "minecraft:iron_nugget".to_string(),
                max_stack_size: 1,
                equipment_slot: None,
                armor_defense: None,
                attack_damage: None,
                attack_speed: None,
                durability: None,
            };
        }

        // 12. Food items
        let is_food = combined.contains("apple")
            || combined.contains("bread")
            || combined.contains("meat")
            || combined.contains("berry")
            || combined.contains("fruit")
            || combined.contains("soup")
            || combined.contains("stew")
            || combined.contains("pie")
            || combined.contains("cake")
            || combined.contains("cookie")
            || combined.contains("food");

        if is_food {
            let is_bowl = combined.contains("soup") || combined.contains("stew");
            let template = if is_bowl {
                "minecraft:mushroom_stew"
            } else {
                "minecraft:apple"
            };
            let stack = if is_bowl { 1 } else { 64 };
            return ItemTemplateInfo {
                template: template.to_string(),
                max_stack_size: stack,
                equipment_slot: None,
                armor_defense: None,
                attack_damage: None,
                attack_speed: None,
                durability: None,
            };
        }

        // 13. Raw materials & components
        if combined.contains("ingot") {
            return ItemTemplateInfo {
                template: "minecraft:iron_ingot".to_string(),
                max_stack_size: 64,
                equipment_slot: None,
                armor_defense: None,
                attack_damage: None,
                attack_speed: None,
                durability: None,
            };
        }
        if combined.contains("gem")
            || combined.contains("crystal")
            || combined.contains("ruby")
            || combined.contains("sapphire")
            || combined.contains("diamond")
            || combined.contains("emerald")
        {
            return ItemTemplateInfo {
                template: "minecraft:diamond".to_string(),
                max_stack_size: 64,
                equipment_slot: None,
                armor_defense: None,
                attack_damage: None,
                attack_speed: None,
                durability: None,
            };
        }
        if combined.contains("dust") || combined.contains("powder") {
            return ItemTemplateInfo {
                template: "minecraft:redstone".to_string(),
                max_stack_size: 64,
                equipment_slot: None,
                armor_defense: None,
                attack_damage: None,
                attack_speed: None,
                durability: None,
            };
        }
        if combined.contains("nugget") {
            return ItemTemplateInfo {
                template: "minecraft:iron_nugget".to_string(),
                max_stack_size: 64,
                equipment_slot: None,
                armor_defense: None,
                attack_damage: None,
                attack_speed: None,
                durability: None,
            };
        }

        // 14. Default fallback item (standard craftable resource)
        ItemTemplateInfo {
            template: "minecraft:iron_ingot".to_string(),
            max_stack_size: 64,
            equipment_slot: None,
            armor_defense: None,
            attack_damage: None,
            attack_speed: None,
            durability: None,
        }
    }

    /// Infers semantic attributes and base template for a block.
    pub fn infer_block(&self, id: &str, class_name: Option<&str>) -> BlockTemplateInfo {
        let clean_id = id.split(':').next_back().unwrap_or(id).to_lowercase();
        let clean_class = class_name
            .map(|c| c.rsplit('/').next().unwrap_or(c).to_lowercase())
            .unwrap_or_default();
        let combined = format!("{clean_id} {clean_class}");

        // 1. Ores
        if combined.contains("ore") {
            if combined.contains("deepslate") {
                return BlockTemplateInfo {
                    template: "minecraft:deepslate_iron_ore".to_string(),
                    hardness: 4.5,
                    resistance: 4.5,
                    light_level: 0,
                    requires_tool: true,
                };
            } else {
                let light = if combined.contains("redstone") || combined.contains("glow") {
                    9
                } else {
                    0
                };
                return BlockTemplateInfo {
                    template: "minecraft:iron_ore".to_string(),
                    hardness: 3.0,
                    resistance: 3.0,
                    light_level: light,
                    requires_tool: true,
                };
            }
        }

        // 2. Metal and storage blocks
        let is_metal_block = (combined.contains("block")
            || combined.contains("metal")
            || combined.contains("alloy"))
            && (combined.contains("iron")
                || combined.contains("gold")
                || combined.contains("copper")
                || combined.contains("bronze")
                || combined.contains("steel")
                || combined.contains("tin")
                || combined.contains("lead")
                || combined.contains("silver")
                || combined.contains("titanium")
                || combined.contains("ruby")
                || combined.contains("sapphire")
                || combined.contains("diamond")
                || combined.contains("netherite")
                || combined.contains("emerald"));

        if is_metal_block {
            return BlockTemplateInfo {
                template: "minecraft:iron_block".to_string(),
                hardness: 5.0,
                resistance: 6.0,
                light_level: 0,
                requires_tool: true,
            };
        }

        // 3. Stone, Cobblestone, Bricks, Slabs, Walls
        let is_stone = combined.contains("stone")
            || combined.contains("rock")
            || combined.contains("cobble")
            || combined.contains("granite")
            || combined.contains("diorite")
            || combined.contains("andesite")
            || combined.contains("basalt")
            || combined.contains("brick")
            || combined.contains("slab")
            || combined.contains("stairs")
            || combined.contains("wall");

        if is_stone {
            if combined.contains("cobble") {
                return BlockTemplateInfo {
                    template: "minecraft:cobblestone".to_string(),
                    hardness: 2.0,
                    resistance: 6.0,
                    light_level: 0,
                    requires_tool: true,
                };
            } else if combined.contains("brick") {
                return BlockTemplateInfo {
                    template: "minecraft:stone_bricks".to_string(),
                    hardness: 1.5,
                    resistance: 6.0,
                    light_level: 0,
                    requires_tool: true,
                };
            } else {
                return BlockTemplateInfo {
                    template: "minecraft:stone".to_string(),
                    hardness: 1.5,
                    resistance: 6.0,
                    light_level: 0,
                    requires_tool: true,
                };
            }
        }

        // 4. Wood, Planks, Logs, Doors, Fences
        let is_wood = combined.contains("wood")
            || combined.contains("plank")
            || combined.contains("log")
            || combined.contains("stem")
            || combined.contains("bark")
            || combined.contains("door")
            || combined.contains("fence")
            || combined.contains("gate")
            || combined.contains("trapdoor")
            || combined.contains("chest")
            || combined.contains("barrel")
            || combined.contains("bookshelf")
            || combined.contains("table")
            || combined.contains("bench");

        if is_wood {
            if combined.contains("log") || combined.contains("stem") {
                return BlockTemplateInfo {
                    template: "minecraft:oak_log".to_string(),
                    hardness: 2.0,
                    resistance: 2.0,
                    light_level: 0,
                    requires_tool: false,
                };
            } else {
                return BlockTemplateInfo {
                    template: "minecraft:oak_planks".to_string(),
                    hardness: 2.0,
                    resistance: 3.0,
                    light_level: 0,
                    requires_tool: false,
                };
            }
        }

        // 5. Glass, Crystal, Windows, Ice
        let is_glass = combined.contains("glass")
            || combined.contains("crystal")
            || combined.contains("pane")
            || combined.contains("ice")
            || combined.contains("window");

        if is_glass {
            return BlockTemplateInfo {
                template: "minecraft:glass".to_string(),
                hardness: 0.3,
                resistance: 0.3,
                light_level: 0,
                requires_tool: false,
            };
        }

        // 6. Light emitters (Torch, Lantern, Lamp, Glowstone)
        let is_light = combined.contains("torch")
            || combined.contains("lantern")
            || combined.contains("lamp")
            || combined.contains("glow")
            || combined.contains("light")
            || combined.contains("candle")
            || combined.contains("fire");

        if is_light {
            if combined.contains("lantern") {
                return BlockTemplateInfo {
                    template: "minecraft:lantern".to_string(),
                    hardness: 3.5,
                    resistance: 3.5,
                    light_level: 15,
                    requires_tool: true,
                };
            } else {
                return BlockTemplateInfo {
                    template: "minecraft:glowstone".to_string(),
                    hardness: 0.3,
                    resistance: 0.3,
                    light_level: 15,
                    requires_tool: false,
                };
            }
        }

        // 7. Dirt, Sand, Gravel, Clay, Mud
        let is_dirt = combined.contains("dirt")
            || combined.contains("sand")
            || combined.contains("gravel")
            || combined.contains("clay")
            || combined.contains("mud")
            || combined.contains("soil")
            || combined.contains("farmland");

        if is_dirt {
            let (template, hardness, resistance) = if combined.contains("sand") {
                ("minecraft:sand", 0.5, 0.5)
            } else if combined.contains("gravel") {
                ("minecraft:gravel", 0.6, 0.6)
            } else {
                ("minecraft:dirt", 0.5, 0.5)
            };

            return BlockTemplateInfo {
                template: template.to_string(),
                hardness,
                resistance,
                light_level: 0,
                requires_tool: false,
            };
        }

        // 8. Leaves and plants
        let is_plant = combined.contains("leaf")
            || combined.contains("leaves")
            || combined.contains("flower")
            || combined.contains("plant")
            || combined.contains("crop")
            || combined.contains("bush")
            || combined.contains("sapling")
            || combined.contains("vine")
            || combined.contains("grass");

        if is_plant {
            let template = if combined.contains("leaf") || combined.contains("leaves") {
                "minecraft:oak_leaves"
            } else {
                "minecraft:dandelion"
            };

            return BlockTemplateInfo {
                template: template.to_string(),
                hardness: 0.2,
                resistance: 0.2,
                light_level: 0,
                requires_tool: false,
            };
        }

        // 9. Machines, Furnaces, and Block Entities
        let is_machine = self
            .hierarchy
            .is_some_and(|h| class_name.is_some_and(|c| h.is_block_entity(c)))
            || combined.contains("furnace")
            || combined.contains("machine")
            || combined.contains("generator")
            || combined.contains("smelter")
            || combined.contains("press")
            || combined.contains("altar")
            || combined.contains("station")
            || combined.contains("conduit");

        if is_machine {
            let light = if combined.contains("lit")
                || combined.contains("active")
                || combined.contains("on")
            {
                13
            } else {
                0
            };

            return BlockTemplateInfo {
                template: "minecraft:furnace".to_string(),
                hardness: 3.5,
                resistance: 3.5,
                light_level: light,
                requires_tool: true,
            };
        }

        // 10. Obsidian and reinforced blocks
        let is_obsidian = combined.contains("obsidian")
            || combined.contains("reinforced")
            || combined.contains("crying")
            || combined.contains("bedrock");

        if is_obsidian {
            return BlockTemplateInfo {
                template: "minecraft:obsidian".to_string(),
                hardness: 50.0,
                resistance: 1200.0,
                light_level: 0,
                requires_tool: true,
            };
        }

        // 11. General default block
        BlockTemplateInfo {
            template: "minecraft:stone".to_string(),
            hardness: 1.5,
            resistance: 6.0,
            light_level: 0,
            requires_tool: true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_infer_sword_template() {
        let info = TemplateEngine::infer_item_template("mymod:ruby_sword", None, None);
        assert_eq!(info.template, "minecraft:iron_sword");
        assert_eq!(info.max_stack_size, 1);
        assert_eq!(info.equipment_slot.as_deref(), Some("mainhand"));
        assert!(info.attack_damage.unwrap() >= 7.0);
        assert!(info.durability.unwrap() >= 1000);
    }

    #[test]
    fn test_infer_armor_templates() {
        let helmet = TemplateEngine::infer_item_template("mymod:diamond_helmet", None, None);
        assert_eq!(helmet.template, "minecraft:iron_helmet");
        assert_eq!(helmet.equipment_slot.as_deref(), Some("head"));
        assert_eq!(helmet.max_stack_size, 1);
        assert!(helmet.armor_defense.unwrap() >= 3.0);

        let chest = TemplateEngine::infer_item_template("mymod:iron_chestplate", None, None);
        assert_eq!(chest.template, "minecraft:iron_chestplate");
        assert_eq!(chest.equipment_slot.as_deref(), Some("chest"));
        assert_eq!(chest.armor_defense.unwrap(), 6.0);

        let leggings = TemplateEngine::infer_item_template("mymod:netherite_leggings", None, None);
        assert_eq!(leggings.template, "minecraft:iron_leggings");
        assert_eq!(leggings.equipment_slot.as_deref(), Some("legs"));
        assert!(leggings.armor_defense.unwrap() >= 6.0);

        let boots = TemplateEngine::infer_item_template("mymod:leather_boots", None, None);
        assert_eq!(boots.template, "minecraft:iron_boots");
        assert_eq!(boots.equipment_slot.as_deref(), Some("feet"));
    }

    #[test]
    fn test_infer_tool_templates() {
        let pickaxe = TemplateEngine::infer_item_template("mymod:steel_pickaxe", None, None);
        assert_eq!(pickaxe.template, "minecraft:iron_pickaxe");
        assert_eq!(pickaxe.max_stack_size, 1);
        assert_eq!(pickaxe.equipment_slot.as_deref(), Some("mainhand"));

        let axe = TemplateEngine::infer_item_template("mymod:battleaxe", None, None);
        assert_eq!(axe.template, "minecraft:iron_axe");
        assert_eq!(axe.max_stack_size, 1);
        assert!(axe.attack_damage.unwrap() >= 9.0);
    }

    #[test]
    fn test_infer_bow_and_potion() {
        let bow = TemplateEngine::infer_item_template("mymod:longbow", None, None);
        assert_eq!(bow.template, "minecraft:bow");
        assert_eq!(bow.max_stack_size, 1);

        let pot = TemplateEngine::infer_item_template("mymod:healing_potion", None, None);
        assert_eq!(pot.template, "minecraft:potion");
        assert_eq!(pot.max_stack_size, 1);
    }

    #[test]
    fn test_infer_wand_staff() {
        let wand = TemplateEngine::infer_item_template("mymod:fire_wand", None, None);
        assert_eq!(wand.template, "minecraft:stick");
        assert_eq!(wand.max_stack_size, 1);
        assert!(wand.durability.is_some());
    }

    #[test]
    fn test_infer_block_templates() {
        let ore = TemplateEngine::infer_block_template("mymod:deepslate_ruby_ore", None, None);
        assert_eq!(ore.template, "minecraft:deepslate_iron_ore");
        assert_eq!(ore.hardness, 4.5);
        assert!(ore.requires_tool);

        let planks = TemplateEngine::infer_block_template("mymod:maple_planks", None, None);
        assert_eq!(planks.template, "minecraft:oak_planks");
        assert_eq!(planks.hardness, 2.0);
        assert!(!planks.requires_tool);

        let glass = TemplateEngine::infer_block_template("mymod:tinted_glass_window", None, None);
        assert_eq!(glass.template, "minecraft:glass");
        assert_eq!(glass.hardness, 0.3);

        let lamp = TemplateEngine::infer_block_template("mymod:magical_lantern", None, None);
        assert_eq!(lamp.template, "minecraft:lantern");
        assert_eq!(lamp.light_level, 15);
    }

    #[test]
    fn test_hierarchy_guided_item_inference() {
        let mut hierarchy = ClassHierarchy::new();
        hierarchy.add_class(
            "com/example/MyCustomBlade",
            Some("net/minecraft/world/item/SwordItem"),
            &[],
        );

        let info = TemplateEngine::infer_item_template(
            "mymod:custom_weapon",
            Some("com/example/MyCustomBlade"),
            Some(&hierarchy),
        );
        assert_eq!(info.template, "minecraft:iron_sword");
        assert_eq!(info.max_stack_size, 1);
        assert_eq!(info.equipment_slot.as_deref(), Some("mainhand"));
    }
}
