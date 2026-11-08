//! Pattern matchers for Forge `DeferredRegister` and `RegistryObject` registration.

use super::{DecodedInstruction, Opcode};
use crate::bytecode::{ClassFile, ClassHierarchy};
use crate::ir::{BlockDef, ContentIR, FoodStats, ItemDef, ToolStats, ToolType};
use crate::manifest::ModMetadata;
use std::collections::HashMap;

use crate::templates::TemplateEngine;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RegistryKind {
    Block,
    Item,
    CreativeTab,
    Other,
}

/// Scans classes for Forge `DeferredRegister` instances and `.register(...)` calls.
pub fn scan_forge_classes(classes: &[ClassFile], meta: &ModMetadata, ir: &mut ContentIR) {
    let hierarchy = ClassHierarchy::from_classes(classes);

    // Phase 1: Discover all DeferredRegister static fields and their registry types
    let mut registers: HashMap<(String, String), (RegistryKind, String)> = HashMap::new();

    for class in classes {
        let class_name = class.class_name();
        for method in &class.methods {
            if let Some(code_info) = &method.code {
                let instructions =
                    super::decode_method_instructions(&code_info.bytecode, &class.constant_pool);
                discover_deferred_registers(&instructions, class_name, meta, &mut registers);
            }
        }
    }

    // Phase 2: Scan for .register(...) calls on DeferredRegister instances
    for class in classes {
        let class_name = class.class_name();
        for method in &class.methods {
            if let Some(code_info) = &method.code {
                let instructions =
                    super::decode_method_instructions(&code_info.bytecode, &class.constant_pool);
                scan_forge_method_registrations(
                    &instructions,
                    class,
                    class_name,
                    meta,
                    &registers,
                    &hierarchy,
                    ir,
                );
            }
        }
    }
}

fn discover_deferred_registers(
    instructions: &[DecodedInstruction],
    enclosing_class: &str,
    meta: &ModMetadata,
    registers: &mut HashMap<(String, String), (RegistryKind, String)>,
) {
    for (i, instr) in instructions.iter().enumerate() {
        let is_deferred_create = match &instr.op {
            Opcode::InvokeStatic {
                class_name, name, ..
            } => {
                class_name.contains("DeferredRegister")
                    && (name == "create" || name == "createBlocks" || name == "createItems")
            }
            _ => false,
        };

        if !is_deferred_create {
            continue;
        }

        let mut kind = RegistryKind::Other;
        let mut modid = meta.id.clone();

        if let Opcode::InvokeStatic { name, .. } = &instr.op {
            if name == "createBlocks" {
                kind = RegistryKind::Block;
            } else if name == "createItems" {
                kind = RegistryKind::Item;
            }
        }

        // Look backwards for registry reference and modid
        for prev in instructions[..i].iter().rev() {
            match &prev.op {
                Opcode::LdcString(s) => {
                    if !s.is_empty() && !s.contains('/') {
                        modid = s.clone();
                    }
                }
                Opcode::GetStatic {
                    name: f_name,
                    descriptor,
                    ..
                } => {
                    if f_name == "BLOCKS" || descriptor.contains("Block") {
                        kind = RegistryKind::Block;
                    } else if f_name == "ITEMS" || descriptor.contains("Item") {
                        kind = RegistryKind::Item;
                    } else if f_name.contains("CREATIVE")
                        || f_name.contains("TAB")
                        || descriptor.contains("CreativeModeTab")
                    {
                        kind = RegistryKind::CreativeTab;
                    }
                }
                _ => {}
            }
        }

        // Look forwards for putstatic to store the DeferredRegister field
        for next in instructions[i + 1..].iter() {
            match &next.op {
                Opcode::PutStatic {
                    class_name: f_class,
                    name: f_name,
                    ..
                } => {
                    registers.insert((f_class.clone(), f_name.clone()), (kind, modid.clone()));
                    break;
                }
                Opcode::InvokeStatic { .. } | Opcode::InvokeVirtual { .. } => break,
                _ => {}
            }
        }

        // Fallback register entry if putstatic was immediate or implicit
        if kind == RegistryKind::Block {
            registers.insert((enclosing_class.to_string(), "BLOCKS".to_string()), (RegistryKind::Block, modid.clone()));
        } else if kind == RegistryKind::Item {
            registers.insert((enclosing_class.to_string(), "ITEMS".to_string()), (RegistryKind::Item, modid.clone()));
        } else if kind == RegistryKind::CreativeTab {
            registers.insert((enclosing_class.to_string(), "CREATIVE_MODE_TABS".to_string()), (RegistryKind::CreativeTab, modid.clone()));
            registers.insert((enclosing_class.to_string(), "TAB".to_string()), (RegistryKind::CreativeTab, modid.clone()));
        }
    }
}

fn scan_forge_method_registrations(
    instructions: &[DecodedInstruction],
    class: &ClassFile,
    enclosing_class: &str,
    meta: &ModMetadata,
    registers: &HashMap<(String, String), (RegistryKind, String)>,
    hierarchy: &ClassHierarchy,
    ir: &mut ContentIR,
) {
    let mut reg_call_indices = Vec::new();

    for (i, instr) in instructions.iter().enumerate() {
        if is_forge_register_call(instr, enclosing_class) {
            reg_call_indices.push(i);
        }
    }

    let mut last_split = 0;

    for (lambda_counter, &call_idx) in reg_call_indices.iter().enumerate() {
        let window = &instructions[last_split..=call_idx];
        process_forge_registration_window(
            window,
            class,
            enclosing_class,
            meta,
            registers,
            hierarchy,
            lambda_counter,
            ir,
        );
        last_split = call_idx + 1;
    }
}

fn is_forge_register_call(instr: &DecodedInstruction, enclosing_class: &str) -> bool {
    match &instr.op {
        Opcode::InvokeVirtual {
            class_name, name, ..
        }
        | Opcode::InvokeInterface {
            class_name, name, ..
        } => {
            name == "register"
                && (class_name.contains("DeferredRegister")
                    || class_name.contains("Registration")
                    || class_name.contains("RegisterEvent")
                    || class_name.contains("IForgeRegistry"))
        }
        Opcode::InvokeStatic {
            class_name,
            name,
            descriptor,
        } => {
            let matches_name = name == "register"
                || name.starts_with("register")
                || name.starts_with("reg");
            let matches_class = class_name == enclosing_class
                || class_name.contains("Registry")
                || class_name.contains("Register")
                || class_name.contains("Item")
                || class_name.contains("Block");
            let returns_deferred = descriptor.ends_with("DeferredItem;")
                || descriptor.ends_with("DeferredBlock;")
                || descriptor.ends_with("DeferredHolder;")
                || descriptor.ends_with("RegistryObject;");

            (matches_name && matches_class) || returns_deferred
        }
        _ => false,
    }
}

#[allow(clippy::too_many_arguments)]
fn process_forge_registration_window(
    window: &[DecodedInstruction],
    class: &ClassFile,
    enclosing_class: &str,
    meta: &ModMetadata,
    registers: &HashMap<(String, String), (RegistryKind, String)>,
    hierarchy: &ClassHierarchy,
    lambda_index: usize,
    ir: &mut ContentIR,
) {
    // 1. Determine target registry and modid
    let mut kind = RegistryKind::Other;
    let mut modid = meta.id.clone();
    let mut entry_name = String::new();

    for instr in window.iter().rev() {
        match &instr.op {
            Opcode::GetStatic {
                class_name, name, ..
            } => {
                if let Some((k, mid)) = registers.get(&(class_name.clone(), name.clone())) {
                    kind = *k;
                    modid = mid.clone();
                } else if let Some((k, mid)) = registers.get(&(enclosing_class.to_string(), name.clone())) {
                    kind = *k;
                    modid = mid.clone();
                } else if name.contains("BLOCK") {
                    kind = RegistryKind::Block;
                } else if name.contains("ITEM") {
                    kind = RegistryKind::Item;
                }
            }
            Opcode::LdcString(s)
                if entry_name.is_empty()
                    && !s.is_empty()
                    && !s.contains('/')
                    && !s.starts_with('L') =>
            {
                entry_name = s.clone();
            }
            _ => {}
        }
    }

    if entry_name.is_empty() {
        return;
    }

    // Inspect the triggering register instruction itself if available
    if let Some(last_instr) = window.last()
        && let Opcode::InvokeStatic { name, descriptor, .. } = &last_instr.op
    {
        let desc_lower = descriptor.to_lowercase();
        let name_lower = name.to_lowercase();
        if desc_lower.ends_with("deferredblock;") || desc_lower.ends_with("/block;") || name_lower.contains("block") {
            kind = RegistryKind::Block;
        } else if desc_lower.ends_with("deferreditem;") || desc_lower.ends_with("/item;") || name_lower.contains("item") || name_lower.contains("wand") {
            kind = RegistryKind::Item;
        }
    }

    if kind == RegistryKind::CreativeTab || (kind == RegistryKind::Other && (enclosing_class.contains("Creative") || enclosing_class.contains("Tab"))) {
        let title_key = format!("creativetab.{entry_name}");
        let default_icon = format!("{modid}:{entry_name}");
        ir.add_creative_tab(crate::ir::CreativeTabDef::new(entry_name, title_key, default_icon));
        return;
    }

    // Skip registrations from non-content registries (Sounds, Particles, MobEffects, DamageSources, Menus, Biomes, Recipes, Entities, Tiers, Elements)
    // or entries with dot-separated paths (e.g. sound events, translation keys)
    if enclosing_class.contains("Sound")
        || enclosing_class.contains("Particle")
        || enclosing_class.contains("Damage")
        || enclosing_class.contains("Menu")
        || enclosing_class.contains("MobEffect")
        || enclosing_class.contains("Recipe")
        || enclosing_class.contains("Entity")
        || enclosing_class.contains("Entities")
        || enclosing_class.contains("Tier")
        || enclosing_class.contains("Element")
        || enclosing_class.contains("Loot")
        || enclosing_class.contains("Feature")
        || enclosing_class.contains("Structure")
        || enclosing_class.contains("Criterion")
        || enclosing_class.contains("Trigger")
        || enclosing_class.contains("Enchantment")
        || enclosing_class.contains("Attribute")
        || enclosing_class.contains("Material")
        || enclosing_class.contains("Component")
        || enclosing_class.contains("Potion")
        || entry_name.contains('.')
    {
        return;
    }

    let full_id = if entry_name.contains(':') {
        entry_name.clone()
    } else {
        format!("{modid}:{entry_name}")
    };

    // 2. Scan properties in window AND in supplier lambda methods
    let mut lambda_instructions = Vec::new();
    find_supplier_instructions(class, lambda_index, &mut lambda_instructions);

    let instantiated_class = find_instantiated_class(window, &lambda_instructions);

    if kind == RegistryKind::Other
        && let Some(ref cls) = instantiated_class
    {
        if hierarchy.is_block(cls) {
            kind = RegistryKind::Block;
        } else if hierarchy.is_item(cls)
            || hierarchy.is_sword(cls)
            || hierarchy.is_axe(cls)
            || hierarchy.is_pickaxe(cls)
            || hierarchy.is_armor(cls)
            || hierarchy.is_bow(cls)
        {
            kind = RegistryKind::Item;
        }
    }

    // If kind is still Other, infer from entry name
    if kind == RegistryKind::Other {
        let lower = entry_name.to_lowercase();
        if lower.contains("block") || lower.contains("ore") || lower.contains("furnace") {
            kind = RegistryKind::Block;
        } else {
            kind = RegistryKind::Item;
        }
    }

    if kind == RegistryKind::Block {
        let mut block = BlockDef::new(&full_id);
        let block_template = TemplateEngine::infer_block_template(
            &entry_name,
            instantiated_class.as_deref().or(Some(enclosing_class)),
            Some(hierarchy),
        );
        block.hardness = block_template.hardness;
        block.resistance = block_template.resistance;
        block.light_emission = block_template.light_level as u32;
        if block_template.requires_tool {
            block.tool = Some(ToolType::Pickaxe);
        }

        extract_forge_block_properties(window, &mut block);
        if !lambda_instructions.is_empty() {
            extract_forge_block_properties(&lambda_instructions, &mut block);
        }
        ir.add_block(block);
    } else {
        let mut item = ItemDef::new(&full_id);
        let item_template = TemplateEngine::infer_item_template(
            &entry_name,
            instantiated_class.as_deref().or(Some(enclosing_class)),
            Some(hierarchy),
        );
        item.max_stack_size = item_template.max_stack_size;
        if let Some(dur) = item_template.durability {
            item.durability = dur;
        }

        let detected_tool_type = instantiated_class
            .as_deref()
            .and_then(|cls| {
                if hierarchy.is_pickaxe(cls) {
                    Some(ToolType::Pickaxe)
                } else if hierarchy.is_axe(cls) {
                    Some(ToolType::Axe)
                } else if hierarchy.is_sword(cls) {
                    Some(ToolType::Sword)
                } else {
                    ToolType::from_name(cls)
                }
            })
            .or_else(|| ToolType::from_name(&entry_name));

        if let Some(tool_type) = detected_tool_type {
            let dmg = item_template.attack_damage.unwrap_or(match tool_type {
                ToolType::Pickaxe => 2.0,
                ToolType::Axe => 6.0,
                ToolType::Shovel => 1.5,
                ToolType::Hoe => 0.0,
                ToolType::Sword => 4.0,
            });
            let spd = item_template.attack_speed.unwrap_or(6.0);
            item.tool = Some(ToolStats {
                tool_type,
                speed: spd,
                attack_damage: dmg,
                mining_level: 2,
            });
        }

        extract_forge_item_properties(window, &full_id, &mut item);
        if !lambda_instructions.is_empty() {
            extract_forge_item_properties(&lambda_instructions, &full_id, &mut item);
        }
        ir.add_item(item);
    }
}

fn find_instantiated_class(
    window: &[DecodedInstruction],
    lambda_instructions: &[DecodedInstruction],
) -> Option<String> {
    for instr in lambda_instructions.iter().chain(window.iter()).rev() {
        if let Opcode::New(cls) = &instr.op {
            let simple = cls.rsplit('/').next().unwrap_or(cls);
            if !simple.ends_with("Properties")
                && !simple.contains("DeferredRegister")
                && !simple.contains("RegistryObject")
                && !simple.contains("Supplier")
            {
                return Some(cls.clone());
            }
        }
    }
    None
}

fn find_supplier_instructions(
    class: &ClassFile,
    lambda_index: usize,
    output: &mut Vec<DecodedInstruction>,
) {
    let lambda_methods: Vec<_> = class
        .methods
        .iter()
        .filter(|m| m.name.starts_with("lambda$") || m.name.contains("$lambda$"))
        .collect();

    let target_method = lambda_methods.get(lambda_index).or_else(|| lambda_methods.first());

    if let Some(code_info) = target_method.and_then(|m| m.code.as_ref()) {
        *output = super::decode_method_instructions(&code_info.bytecode, &class.constant_pool);
    }
}

fn extract_forge_block_properties(instructions: &[DecodedInstruction], block: &mut BlockDef) {
    for (i, instr) in instructions.iter().enumerate() {
        match &instr.op {
            Opcode::InvokeVirtual { name, .. } | Opcode::InvokeStatic { name, .. } => {
                let lower = name.to_lowercase();
                if lower == "strength" || lower == "destroytime" {
                    let mut floats = Vec::new();
                    for prev in instructions[..i].iter().rev() {
                        match &prev.op {
                            Opcode::LdcFloat(f) => floats.push(*f),
                            Opcode::ConstFloat(f) => floats.push(*f),
                            Opcode::LdcInt(n) => floats.push(*n as f32),
                            Opcode::ConstInt(n) => floats.push(*n as f32),
                            Opcode::InvokeVirtual { .. }
                            | Opcode::InvokeStatic { .. }
                            | Opcode::New(_) => break,
                            _ => {}
                        }
                        if floats.len() >= 2 {
                            break;
                        }
                    }
                    floats.reverse();
                    if floats.len() >= 2 {
                        block.hardness = floats[0];
                        block.resistance = floats[1];
                    } else if floats.len() == 1 {
                        block.hardness = floats[0];
                        block.resistance = floats[0];
                    }
                } else if lower == "lightlevel" || lower == "luminance" {
                    for prev in instructions[..i].iter().rev() {
                        match &prev.op {
                            Opcode::ConstInt(n) | Opcode::LdcInt(n) => {
                                block.light_emission = (*n).clamp(0, 15) as u32;
                                break;
                            }
                            _ => {}
                        }
                    }
                } else if lower == "noocclusion" || lower == "nonopaque" {
                    block.opaque = false;
                } else if lower.contains("requirestool") || lower.contains("requirescorrecttool") {
                    block.tool = Some(ToolType::Pickaxe);
                }
            }
            _ => {}
        }
    }
}

fn extract_forge_item_properties(instructions: &[DecodedInstruction], id: &str, item: &mut ItemDef) {
    if item.tool.is_none() {
        for instr in instructions {
            let tool_from_class = match &instr.op {
                Opcode::New(cls) => ToolType::from_name(cls),
                _ => None,
            };
            if let Some(tool_type) = tool_from_class {
                item.tool = Some(ToolStats {
                    tool_type,
                    speed: 6.0,
                    attack_damage: match tool_type {
                        ToolType::Pickaxe => 2.0,
                        ToolType::Axe => 6.0,
                        ToolType::Shovel => 1.5,
                        ToolType::Hoe => 0.0,
                        ToolType::Sword => 4.0,
                    },
                    mining_level: 2,
                });
                break;
            }
        }
    }

    if item.tool.is_none() {
        item.tool = ToolType::from_name(id).map(|tool_type| ToolStats {
            tool_type,
            speed: 6.0,
            attack_damage: match tool_type {
                ToolType::Pickaxe => 2.0,
                ToolType::Axe => 6.0,
                ToolType::Shovel => 1.5,
                ToolType::Hoe => 0.0,
                ToolType::Sword => 4.0,
            },
            mining_level: 2,
        });
    }

    for (i, instr) in instructions.iter().enumerate() {
        match &instr.op {
            Opcode::InvokeVirtual { name, .. } | Opcode::InvokeStatic { name, .. } => {
                let lower = name.to_lowercase();
                if lower == "stacksto" || lower == "maxcount" {
                    for prev in instructions[..i].iter().rev() {
                        match &prev.op {
                            Opcode::ConstInt(n) | Opcode::LdcInt(n) if *n > 0 => {
                                item.max_stack_size = *n as u32;
                                break;
                            }
                            _ => {}
                        }
                    }
                } else if lower == "durability" || lower == "maxdamage" {
                    for prev in instructions[..i].iter().rev() {
                        match &prev.op {
                            Opcode::ConstInt(n) | Opcode::LdcInt(n) if *n > 0 => {
                                item.durability = *n as u32;
                                break;
                            }
                            _ => {}
                        }
                    }
                } else if lower == "food" {
                    item.food = Some(FoodStats {
                        nutrition: 4,
                        saturation_modifier: 0.6,
                        always_edible: false,
                    });
                }
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::manifest::ModLoader;
    use crate::bytecode::constant_pool::ConstantPool;

    fn make_empty_class(_name: &str) -> ClassFile {
        ClassFile {
            minor_version: 0,
            major_version: 65,
            constant_pool: ConstantPool::new(),
            access_flags: 1,
            this_class: 0,
            super_class: 0,
            interfaces: vec![],
            fields: vec![],
            methods: vec![],
            attributes: vec![],
        }
    }

    #[test]
    fn test_find_instantiated_class() {
        let window = vec![
            DecodedInstruction {
                offset: 0,
                raw_opcode: 0xBB,
                op: Opcode::New("net/minecraft/world/item/Item$Properties".to_string()),
            },
        ];
        let lambda = vec![
            DecodedInstruction {
                offset: 5,
                raw_opcode: 0xBB,
                op: Opcode::New("com/example/mod/RubySword".to_string()),
            },
        ];
        assert_eq!(
            find_instantiated_class(&window, &lambda),
            Some("com/example/mod/RubySword".to_string())
        );
    }

    #[test]
    fn test_forge_block_template_defaults() {
        let meta = ModMetadata {
            id: "mymod".to_string(),
            name: "My Mod".to_string(),
            version: "1.0.0".to_string(),
            authors: vec![],
            description: "".to_string(),
            loader: ModLoader::Forge,
        };
        let mut ir = ContentIR::new(meta.clone());
        let mut registers = HashMap::new();
        registers.insert(
            ("mymod/ModBlocks".to_string(), "BLOCKS".to_string()),
            (RegistryKind::Block, "mymod".to_string()),
        );

        let window = vec![
            DecodedInstruction {
                offset: 0,
                raw_opcode: 0xB2,
                op: Opcode::GetStatic {
                    class_name: "mymod/ModBlocks".to_string(),
                    name: "BLOCKS".to_string(),
                    descriptor: "Lnet/minecraftforge/registries/DeferredRegister;".to_string(),
                },
            },
            DecodedInstruction {
                offset: 3,
                raw_opcode: 0x12,
                op: Opcode::LdcString("deepslate_tin_ore".to_string()),
            },
        ];

        let class = make_empty_class("mymod/ModBlocks");
        let hierarchy = ClassHierarchy::new();

        process_forge_registration_window(
            &window,
            &class,
            "mymod/ModBlocks",
            &meta,
            &registers,
            &hierarchy,
            0,
            &mut ir,
        );

        let block = ir.get_block("mymod:deepslate_tin_ore").expect("Block registered");
        assert_eq!(block.hardness, 4.5);
        assert_eq!(block.resistance, 4.5);
        assert_eq!(block.tool, Some(ToolType::Pickaxe));
    }

    #[test]
    fn test_forge_item_template_and_hierarchy_defaults() {
        let meta = ModMetadata {
            id: "mymod".to_string(),
            name: "My Mod".to_string(),
            version: "1.0.0".to_string(),
            authors: vec![],
            description: "".to_string(),
            loader: ModLoader::Forge,
        };
        let mut ir = ContentIR::new(meta.clone());
        let mut registers = HashMap::new();
        registers.insert(
            ("mymod/ModItems".to_string(), "ITEMS".to_string()),
            (RegistryKind::Item, "mymod".to_string()),
        );

        let mut hierarchy = ClassHierarchy::new();
        hierarchy.add_class(
            "com/example/mod/ObsidianBattleAxe",
            Some("net/minecraft/world/item/AxeItem"),
            &[],
        );

        let window = vec![
            DecodedInstruction {
                offset: 0,
                raw_opcode: 0xB2,
                op: Opcode::GetStatic {
                    class_name: "mymod/ModItems".to_string(),
                    name: "ITEMS".to_string(),
                    descriptor: "Lnet/minecraftforge/registries/DeferredRegister;".to_string(),
                },
            },
            DecodedInstruction {
                offset: 3,
                raw_opcode: 0x12,
                op: Opcode::LdcString("obsidian_axe".to_string()),
            },
            DecodedInstruction {
                offset: 6,
                raw_opcode: 0xBB,
                op: Opcode::New("com/example/mod/ObsidianBattleAxe".to_string()),
            },
        ];

        let class = make_empty_class("mymod/ModItems");

        process_forge_registration_window(
            &window,
            &class,
            "mymod/ModItems",
            &meta,
            &registers,
            &hierarchy,
            0,
            &mut ir,
        );

        let item = ir.get_item("mymod:obsidian_axe").expect("Item registered");
        assert_eq!(item.max_stack_size, 1);
        let tool = item.tool.as_ref().expect("Tool stats must exist");
        assert_eq!(tool.tool_type, ToolType::Axe);
        assert!(tool.attack_damage >= 9.0);
    }
}
