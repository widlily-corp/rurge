//! Pattern matchers for Fabric block and item registration.

use super::{DecodedInstruction, Opcode};
use crate::bytecode::ClassFile;
use crate::ir::{BlockDef, ContentIR, FoodStats, ItemDef, ToolStats, ToolType};
use crate::manifest::ModMetadata;

/// Scans classes for Fabric-style `Registry.register` or `Registries.register` calls.
pub fn scan_fabric_classes(classes: &[ClassFile], meta: &ModMetadata, ir: &mut ContentIR) {
    for class in classes {
        for method in &class.methods {
            if let Some(code_info) = &method.code {
                let instructions =
                    super::decode_method_instructions(&code_info.bytecode, &class.constant_pool);
                scan_fabric_instructions(&instructions, meta, ir);
            }
        }
    }
}

fn scan_fabric_instructions(
    instructions: &[DecodedInstruction],
    meta: &ModMetadata,
    ir: &mut ContentIR,
) {
    let mut last_split = 0;

    for (i, instr) in instructions.iter().enumerate() {
        if is_fabric_register_call(instr) {
            let window = &instructions[last_split..=i];
            process_fabric_registration_window(window, meta, ir);
            last_split = i + 1;
        }
    }
}

fn is_fabric_register_call(instr: &DecodedInstruction) -> bool {
    match &instr.op {
        Opcode::InvokeStatic {
            class_name,
            name,
            ..
        } => {
            name == "register"
                && (class_name.contains("registry/Registry")
                    || class_name.contains("registry/Registries")
                    || class_name.ends_with("/Registry")
                    || class_name.ends_with("/Registries"))
        }
        _ => false,
    }
}

fn process_fabric_registration_window(
    window: &[DecodedInstruction],
    meta: &ModMetadata,
    ir: &mut ContentIR,
) {
    // 1. Determine target registry: BLOCK or ITEM
    let mut is_block = false;
    let mut is_item = false;

    for instr in window {
        if let Opcode::GetStatic {
            name,
            class_name,
            descriptor,
        } = &instr.op
        {
            if !class_name.contains("Registry") && !class_name.contains("Registries") {
                continue;
            }
            if name == "BLOCK" || descriptor.contains("Block") {
                is_block = true;
                break;
            } else if name == "ITEM" || descriptor.contains("Item") {
                is_item = true;
                break;
            }
        }
    }

    // Fallback: check New class instantiation in the window
    if !is_block && !is_item {
        for instr in window {
            if let Opcode::New(cls) = &instr.op {
                if cls.contains("Block") && !cls.contains("BlockItem") {
                    is_block = true;
                    break;
                } else if cls.contains("Item") {
                    is_item = true;
                    break;
                }
            }
        }
    }

    // 2. Extract Identifier / Name
    let id = extract_identifier_from_window(window, meta);
    if id.is_empty() {
        return;
    }

    // 3. Extract Block or Item properties
    if is_block {
        let mut block = BlockDef::new(&id);
        extract_block_properties(window, &mut block);
        ir.add_block(block);
    } else if is_item {
        let mut item = ItemDef::new(&id);
        extract_item_properties(window, &id, &mut item);
        ir.add_item(item);
    }
}

fn extract_identifier_from_window(window: &[DecodedInstruction], meta: &ModMetadata) -> String {
    // Check for Identifier.<init> or Identifier.of
    for (i, instr) in window.iter().enumerate() {
        match &instr.op {
            Opcode::InvokeSpecial {
                class_name, name, ..
            }
            | Opcode::InvokeStatic {
                class_name, name, ..
            } if (class_name.contains("Identifier") || class_name.contains("ResourceLocation"))
                && (name == "<init>" || name == "of" || name == "tryParse") =>
            {
                // Look backwards for strings pushed
                let mut strings = Vec::new();
                for prev in window[..i].iter().rev() {
                    match &prev.op {
                        Opcode::LdcString(s) => strings.push(s.clone()),
                        Opcode::New(_) | Opcode::GetStatic { .. } => break,
                        _ => {}
                    }
                    if strings.len() >= 2 {
                        break;
                    }
                }
                strings.reverse();

                if strings.len() >= 2 {
                    return format!("{}:{}", strings[0], strings[1]);
                } else if strings.len() == 1 {
                    let s = &strings[0];
                    if s.contains(':') {
                        return s.clone();
                    } else {
                        return format!("{}:{}", meta.id, s);
                    }
                }
            }
            _ => {}
        }
    }

    // Fallback: look for strings in the window
    for instr in window.iter().rev() {
        if let Opcode::LdcString(s) = &instr.op {
            if s.contains(':') {
                return s.clone();
            } else if !s.is_empty() && !s.contains('/') && !s.starts_with('L') {
                return format!("{}:{}", meta.id, s);
            }
        }
    }

    String::new()
}

fn extract_block_properties(window: &[DecodedInstruction], block: &mut BlockDef) {
    for (i, instr) in window.iter().enumerate() {
        match &instr.op {
            Opcode::InvokeVirtual { name, .. } | Opcode::InvokeStatic { name, .. } => {
                let lower = name.to_lowercase();
                if lower == "strength" || lower == "hardnessandresistance" || lower == "destroytime"
                {
                    // Inspect preceding floats
                    let mut floats = Vec::new();
                    for prev in window[..i].iter().rev() {
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
                } else if lower == "luminance" || lower == "lightlevel" {
                    // Preceding int
                    for prev in window[..i].iter().rev() {
                        match &prev.op {
                            Opcode::ConstInt(n) | Opcode::LdcInt(n) => {
                                block.light_emission = (*n).clamp(0, 15) as u32;
                                break;
                            }
                            _ => {}
                        }
                    }
                } else if lower == "nonopaque" || lower == "noocclusion" {
                    block.opaque = false;
                } else if lower == "requirestool" || lower == "breakbytool" {
                    block.tool = Some(ToolType::Pickaxe);
                }
            }
            _ => {}
        }
    }
}

fn extract_item_properties(window: &[DecodedInstruction], id: &str, item: &mut ItemDef) {
    // Check if instantiated with a specific Tool class
    for instr in window {
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

    // Also check item ID for tool hints if not yet set
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

    for (i, instr) in window.iter().enumerate() {
        match &instr.op {
            Opcode::InvokeVirtual { name, .. } | Opcode::InvokeStatic { name, .. } => {
                let lower = name.to_lowercase();
                if lower == "maxcount" || lower == "stacksto" {
                    for prev in window[..i].iter().rev() {
                        match &prev.op {
                            Opcode::ConstInt(n) | Opcode::LdcInt(n) if *n > 0 => {
                                item.max_stack_size = *n as u32;
                                break;
                            }
                            _ => {}
                        }
                    }
                } else if lower == "maxdamage" || lower == "durability" {
                    for prev in window[..i].iter().rev() {
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
