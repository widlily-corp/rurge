//! Multi-loader pattern matchers for Minecraft `MobEffect` / `StatusEffect` registration.
//!
//! Supports Forge/NeoForge `DeferredRegister<MobEffect>`, Fabric `Registry.register(Registries.STATUS_EFFECT, ...)`,
//! category resolution (`BENEFICIAL`, `HARMFUL`, `NEUTRAL`), and RGB color constant extraction.

use super::{DecodedInstruction, Opcode};
use crate::bytecode::ClassFile;
use crate::ir::{ContentIR, MobEffectDef};
use crate::manifest::ModMetadata;
use std::collections::HashMap;

/// Determines effect category (`beneficial`, `harmful`, `neutral`) from explicit or heuristic sources.
pub fn determine_effect_category(
    explicit: Option<&str>,
    name: &str,
    class_name: Option<&str>,
) -> &'static str {
    if let Some(cat) = explicit {
        return match cat.to_ascii_lowercase().as_str() {
            "harmful" => "harmful",
            "neutral" => "neutral",
            _ => "beneficial",
        };
    }

    let mut combined = name.to_lowercase();
    if let Some(cls) = class_name {
        combined.push(' ');
        combined.push_str(&cls.to_lowercase());
    }

    if combined.contains("harm")
        || combined.contains("frost")
        || combined.contains("poison")
        || combined.contains("wither")
        || combined.contains("slow")
        || combined.contains("weak")
        || combined.contains("blind")
        || combined.contains("curse")
        || combined.contains("decay")
        || combined.contains("burn")
        || combined.contains("bleed")
        || combined.contains("paraly")
        || combined.contains("stun")
        || combined.contains("freeze")
    {
        "harmful"
    } else {
        "beneficial"
    }
}

/// Scans classes for Forge and Fabric mob effect registrations and populates `ir.mob_effects`.
pub fn scan_effects(classes: &[ClassFile], meta: &ModMetadata, ir: &mut ContentIR) {
    // Phase 1: Scan effect constructors to index custom MobEffect subclasses (category & color)
    let mut effect_class_defaults: HashMap<String, (String, u32)> = HashMap::new();
    for class in classes {
        inspect_effect_subclass_constructors(class, &mut effect_class_defaults);
    }

    // Phase 2: Discover DeferredRegister instances dedicated to MobEffect
    let mut registers: HashMap<(String, String), String> = HashMap::new();
    for class in classes {
        let class_name = class.class_name();
        for method in &class.methods {
            if let Some(code_info) = &method.code {
                let instructions =
                    super::decode_method_instructions(&code_info.bytecode, &class.constant_pool);
                discover_effect_registers(&instructions, class_name, meta, &mut registers);
            }
        }
    }

    // Phase 3: Scan methods for .register(...) calls on effects
    for class in classes {
        let class_name = class.class_name();
        for method in &class.methods {
            if let Some(code_info) = &method.code {
                let instructions =
                    super::decode_method_instructions(&code_info.bytecode, &class.constant_pool);
                scan_method_effect_registrations(
                    &instructions,
                    class_name,
                    meta,
                    &registers,
                    &effect_class_defaults,
                    ir,
                );
            }
        }
    }

    // Phase 4: Scan lambda methods for inline suppliers refining category & color
    for class in classes {
        for method in &class.methods {
            if let Some(code_info) = &method.code {
                let instructions =
                    super::decode_method_instructions(&code_info.bytecode, &class.constant_pool);
                refine_effects_from_lambdas(&instructions, &effect_class_defaults, ir);
            }
        }
    }
}

/// Alias for `scan_effects`.
pub use scan_effects as scan_effect_classes;

fn inspect_effect_subclass_constructors(
    class: &ClassFile,
    defaults: &mut HashMap<String, (String, u32)>,
) {
    let class_name = class.class_name();
    for method in &class.methods {
        if method.name != "<init>" {
            continue;
        }
        if let Some(code_info) = &method.code {
            let instructions =
                super::decode_method_instructions(&code_info.bytecode, &class.constant_pool);

            let mut category = None;
            let mut color = None;

            for instr in &instructions {
                match &instr.op {
                    Opcode::GetStatic { name, .. } => {
                        if name == "BENEFICIAL" || name == "HARMFUL" || name == "NEUTRAL" {
                            category = Some(name.to_ascii_lowercase());
                        }
                    }
                    Opcode::LdcInt(c) | Opcode::ConstInt(c)
                        if *c > 0 && *c <= 0xFFFFFF && color.is_none() =>
                    {
                        color = Some(*c as u32);
                    }
                    _ => {}
                }
            }

            if let Some(cat) = category {
                let col = color.unwrap_or(0xFFFFFF);
                defaults.insert(class_name.to_string(), (cat, col));
            }
        }
    }
}

fn discover_effect_registers(
    instructions: &[DecodedInstruction],
    enclosing_class: &str,
    meta: &ModMetadata,
    registers: &mut HashMap<(String, String), String>,
) {
    for (i, instr) in instructions.iter().enumerate() {
        let is_deferred_create = match &instr.op {
            Opcode::InvokeStatic {
                class_name, name, ..
            } => {
                class_name.contains("DeferredRegister")
                    && (name == "create" || name == "createEffects")
            }
            _ => false,
        };

        if !is_deferred_create {
            continue;
        }

        let mut is_effect = false;
        let mut modid = meta.id.clone();

        // Check backwards for Registries.MOB_EFFECT or ForgeRegistries.MOB_EFFECTS / POTIONS
        for prev in instructions[..i].iter().rev() {
            match &prev.op {
                Opcode::LdcString(s) => {
                    if !s.is_empty() && !s.contains('/') {
                        modid = s.clone();
                    }
                }
                Opcode::GetStatic {
                    name,
                    descriptor,
                    class_name,
                } if name == "MOB_EFFECTS"
                    || name == "MOB_EFFECT"
                    || name == "STATUS_EFFECT"
                    || name == "POTIONS"
                    || descriptor.contains("MobEffect")
                    || descriptor.contains("StatusEffect")
                    || class_name.contains("MobEffect")
                    || class_name.contains("StatusEffect") =>
                {
                    is_effect = true;
                }
                _ => {}
            }
        }

        if !is_effect {
            continue;
        }

        // Look forwards for putstatic to store the DeferredRegister field
        for next in instructions[i + 1..].iter() {
            match &next.op {
                Opcode::PutStatic {
                    class_name: f_class,
                    name: f_name,
                    ..
                } => {
                    registers.insert((f_class.clone(), f_name.clone()), modid.clone());
                    break;
                }
                Opcode::InvokeStatic { .. } | Opcode::InvokeVirtual { .. } => break,
                _ => {}
            }
        }

        registers.insert((enclosing_class.to_string(), "MOB_EFFECTS".to_string()), modid.clone());
        registers.insert((enclosing_class.to_string(), "EFFECTS".to_string()), modid);
    }
}

fn scan_method_effect_registrations(
    instructions: &[DecodedInstruction],
    enclosing_class: &str,
    meta: &ModMetadata,
    registers: &HashMap<(String, String), String>,
    effect_class_defaults: &HashMap<String, (String, u32)>,
    ir: &mut ContentIR,
) {
    let mut last_split = 0;

    for (i, instr) in instructions.iter().enumerate() {
        if is_effect_register_call(instr, enclosing_class, registers, instructions, i) {
            let window = &instructions[last_split..=i];
            process_effect_registration_window(window, meta, registers, effect_class_defaults, ir);
            last_split = i + 1;
        } else if matches!(&instr.op, Opcode::InvokeStatic { class_name, name, .. } if class_name.contains("DeferredRegister") && (name == "create" || name == "createEffects")) {
            last_split = i + 1;
        }
    }
}

fn is_effect_register_call(
    instr: &DecodedInstruction,
    enclosing_class: &str,
    registers: &HashMap<(String, String), String>,
    all_instructions: &[DecodedInstruction],
    current_idx: usize,
) -> bool {
    match &instr.op {
        Opcode::InvokeVirtual {
            class_name, name, ..
        }
        | Opcode::InvokeInterface {
            class_name, name, ..
        } => {
            if name == "register" && class_name.contains("DeferredRegister") {
                let window_start = current_idx.saturating_sub(15);
                for prev in &all_instructions[window_start..current_idx] {
                    if matches!(&prev.op, Opcode::GetStatic { class_name: f_class, name: f_name, .. } if registers.contains_key(&(f_class.clone(), f_name.clone())) || f_name == "MOB_EFFECTS" || f_name == "EFFECTS" || f_name == "STATUS_EFFECTS") {
                        return true;
                    }
                }
            }
            false
        }
        Opcode::InvokeStatic {
            class_name,
            name,
            descriptor,
        } => {
            // Fabric Registry.register(Registries.STATUS_EFFECT, ...)
            if name == "register"
                && (class_name.contains("registry/Registry")
                    || class_name.contains("registry/Registries")
                    || class_name.ends_with("/Registry")
                    || class_name.ends_with("/Registries"))
            {
                let window_start = current_idx.saturating_sub(15);
                for prev in &all_instructions[window_start..current_idx] {
                    if matches!(&prev.op, Opcode::GetStatic { name: f_name, descriptor: f_desc, .. } if f_name == "STATUS_EFFECT" || f_name == "MOB_EFFECT" || f_name == "POTION" || f_desc.contains("MobEffect") || f_desc.contains("StatusEffect")) {
                        return true;
                    }
                }
                return false;
            }

            // Custom static helper register("effect_name", ...)
            if (name == "register" || name == "registerEffect")
                && (descriptor.contains("MobEffect")
                    || descriptor.contains("StatusEffect")
                    || class_name == enclosing_class)
            {
                let window_start = current_idx.saturating_sub(10);
                for prev in &all_instructions[window_start..current_idx] {
                    if let Opcode::LdcString(_) = &prev.op {
                        return true;
                    }
                }
            }
            false
        }
        _ => false,
    }
}

fn process_effect_registration_window(
    window: &[DecodedInstruction],
    meta: &ModMetadata,
    registers: &HashMap<(String, String), String>,
    effect_class_defaults: &HashMap<String, (String, u32)>,
    ir: &mut ContentIR,
) {
    let mut effect_name = None;
    let mut effect_class = None;
    let mut explicit_category = None;
    let mut explicit_color = None;
    let mut modid = meta.id.clone();

    // First pass: scan backwards from the registration call for entry name and modid
    for instr in window.iter().rev() {
        match &instr.op {
            Opcode::GetStatic {
                class_name, name, ..
            } => {
                if let Some(discovered_modid) = registers.get(&(class_name.clone(), name.clone())) {
                    modid = discovered_modid.clone();
                }
            }
            Opcode::LdcString(s)
                if effect_name.is_none()
                    && ((!s.is_empty() && !s.contains('/') && !s.contains(' '))
                        || s.contains(':')) =>
            {
                effect_name = Some(s.clone());
            }
            _ => {}
        }
    }

    // Scan forward for class, category, and color
    for instr in window {
        match &instr.op {
            Opcode::LdcClass(cls) | Opcode::New(cls) => {
                if !cls.contains("MobEffect")
                    && !cls.contains("StatusEffect")
                    && !cls.contains("Identifier")
                    && !cls.contains("ResourceLocation")
                    && !cls.contains("Supplier")
                    && effect_class.is_none()
                {
                    effect_class = Some(cls.clone());
                }
            }
            Opcode::GetStatic { name, .. } => {
                if name == "BENEFICIAL" || name == "HARMFUL" || name == "NEUTRAL" {
                    explicit_category = Some(name.to_ascii_lowercase());
                }
            }
            Opcode::LdcInt(c) | Opcode::ConstInt(c)
                if *c > 0 && *c <= 0xFFFFFF && explicit_color.is_none() =>
            {
                explicit_color = Some(*c as u32);
            }
            _ => {}
        }
    }

    let name = match effect_name {
        Some(n) => n,
        None => return,
    };

    let full_id = if name.contains(':') {
        name.clone()
    } else {
        format!("{}:{}", modid, name)
    };

    // If category or color are missing, consult subclass defaults
    if let Some((cat, col)) = effect_class
        .as_ref()
        .and_then(|cls| effect_class_defaults.get(cls))
    {
        if explicit_category.is_none() {
            explicit_category = Some(cat.clone());
        }
        if explicit_color.is_none() {
            explicit_color = Some(*col);
        }
    }

    let category = determine_effect_category(
        explicit_category.as_deref(),
        &name,
        effect_class.as_deref(),
    );
    let color = explicit_color.unwrap_or(0xFFFFFF);

    ir.add_mob_effect(MobEffectDef::new(full_id, category, color));
}

fn refine_effects_from_lambdas(
    instructions: &[DecodedInstruction],
    effect_class_defaults: &HashMap<String, (String, u32)>,
    ir: &mut ContentIR,
) {
    let mut category = None;
    let mut color = None;
    let mut target_class = None;

    for instr in instructions {
        match &instr.op {
            Opcode::GetStatic { name, .. } => {
                if name == "BENEFICIAL" || name == "HARMFUL" || name == "NEUTRAL" {
                    category = Some(name.to_ascii_lowercase());
                }
            }
            Opcode::LdcInt(c) | Opcode::ConstInt(c)
                if *c > 0 && *c <= 0xFFFFFF && color.is_none() =>
            {
                color = Some(*c as u32);
            }
            Opcode::New(cls) => {
                target_class = Some(cls.clone());
            }
            _ => {}
        }
    }

    if let Some((cat, col)) = target_class
        .as_ref()
        .and_then(|cls| effect_class_defaults.get(cls))
    {
        if category.is_none() {
            category = Some(cat.clone());
        }
        if color.is_none() {
            color = Some(*col);
        }
    }

    // If this lambda produced a category or color, we can update any effect matching target class or name
    if let (Some(cat), Some(col)) = (category, color) {
        for effect in &mut ir.mob_effects {
            if effect.color == 0xFFFFFF {
                effect.category = cat.clone();
                effect.color = col;
                break;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bytecode::{
        AttributeInfo, ClassFile, CodeInfo, ConstantPool, ConstantPoolEntry, MethodInfo,
    };
    use crate::manifest::ModLoader;

    fn mock_meta() -> ModMetadata {
        ModMetadata {
            id: "ebwizardry".to_string(),
            name: "Electroblob's Wizardry".to_string(),
            version: "4.3.9".to_string(),
            description: "Spellcasting and wizardry".to_string(),
            authors: vec!["Electroblob".to_string()],
            loader: ModLoader::Forge,
        }
    }

    fn push_utf8(cp: &mut ConstantPool, s: &str) -> u16 {
        cp.push(ConstantPoolEntry::Utf8(s.to_string()));
        (cp.len() - 1) as u16
    }

    fn push_string(cp: &mut ConstantPool, s: &str) -> u16 {
        let utf8_idx = push_utf8(cp, s);
        cp.push(ConstantPoolEntry::StringRef {
            string_index: utf8_idx,
        });
        (cp.len() - 1) as u16
    }

    fn push_class(cp: &mut ConstantPool, name: &str) -> u16 {
        let utf8_idx = push_utf8(cp, name);
        cp.push(ConstantPoolEntry::Class {
            name_index: utf8_idx,
        });
        (cp.len() - 1) as u16
    }

    fn push_name_and_type(cp: &mut ConstantPool, name: &str, desc: &str) -> u16 {
        let n_idx = push_utf8(cp, name);
        let d_idx = push_utf8(cp, desc);
        cp.push(ConstantPoolEntry::NameAndType {
            name_index: n_idx,
            descriptor_index: d_idx,
        });
        (cp.len() - 1) as u16
    }

    fn push_method_ref(cp: &mut ConstantPool, class_idx: u16, nat_idx: u16) -> u16 {
        cp.push(ConstantPoolEntry::Methodref {
            class_index: class_idx,
            name_and_type_index: nat_idx,
        });
        (cp.len() - 1) as u16
    }

    fn push_field_ref(cp: &mut ConstantPool, class_idx: u16, nat_idx: u16) -> u16 {
        cp.push(ConstantPoolEntry::Fieldref {
            class_index: class_idx,
            name_and_type_index: nat_idx,
        });
        (cp.len() - 1) as u16
    }

    #[test]
    fn test_determine_effect_category_heuristics() {
        assert_eq!(
            determine_effect_category(None, "frost", None),
            "harmful"
        );
        assert_eq!(
            determine_effect_category(None, "poison", None),
            "harmful"
        );
        assert_eq!(
            determine_effect_category(None, "fireskin", None),
            "beneficial"
        );
        assert_eq!(
            determine_effect_category(Some("NEUTRAL"), "fireskin", None),
            "neutral"
        );
        assert_eq!(
            determine_effect_category(Some("HARMFUL"), "speed", None),
            "harmful"
        );
    }

    #[test]
    fn test_scan_forge_mob_effect_registration() {
        let mut cp = ConstantPool::new();
        let utf_clinit = push_utf8(&mut cp, "<clinit>");
        let utf_void = push_utf8(&mut cp, "()V");
        let utf_code = push_utf8(&mut cp, "Code");

        let cls_def_reg = push_class(&mut cp, "net/minecraftforge/registrations/DeferredRegister");
        let nat_create = push_name_and_type(
            &mut cp,
            "create",
            "(Lnet/minecraft/resources/ResourceKey;Ljava/lang/String;)Lnet/minecraftforge/registrations/DeferredRegister;",
        );
        let mref_create = push_method_ref(&mut cp, cls_def_reg, nat_create);

        let cls_forge_reg = push_class(&mut cp, "net/minecraftforge/registries/ForgeRegistries");
        let nat_effects = push_name_and_type(
            &mut cp,
            "MOB_EFFECTS",
            "Lnet/minecraftforge/registries/IForgeRegistry;",
        );
        let fref_effects = push_field_ref(&mut cp, cls_forge_reg, nat_effects);

        let str_modid = push_string(&mut cp, "ebwizardry");

        let cls_mod = push_class(&mut cp, "com/example/ModEffects");
        let nat_reg_field = push_name_and_type(
            &mut cp,
            "MOB_EFFECTS",
            "Lnet/minecraftforge/registrations/DeferredRegister;",
        );
        let fref_reg_field = push_field_ref(&mut cp, cls_mod, nat_reg_field);

        let str_frost = push_string(&mut cp, "frost");
        let nat_register = push_name_and_type(
            &mut cp,
            "register",
            "(Ljava/lang/String;Ljava/util/function/Supplier;)Lnet/minecraftforge/registrations/RegistryObject;",
        );
        let mref_register = push_method_ref(&mut cp, cls_def_reg, nat_register);

        // Constant pool integer for color 0x3399FF (3381759)
        cp.push(ConstantPoolEntry::Integer(0x3399FF));
        let int_color = (cp.len() - 1) as u16;

        let cls_category = push_class(&mut cp, "net/minecraft/world/effect/MobEffectCategory");
        let nat_harmful = push_name_and_type(
            &mut cp,
            "HARMFUL",
            "Lnet/minecraft/world/effect/MobEffectCategory;",
        );
        let fref_harmful = push_field_ref(&mut cp, cls_category, nat_harmful);

        let mut bytecode = Vec::new();
        // getstatic ForgeRegistries.MOB_EFFECTS
        bytecode.extend_from_slice(&[0xB2, (fref_effects >> 8) as u8, (fref_effects & 0xFF) as u8]);
        // ldc str_modid
        bytecode.extend_from_slice(&[0x12, str_modid as u8]);
        // invokestatic DeferredRegister.create
        bytecode.extend_from_slice(&[0xB8, (mref_create >> 8) as u8, (mref_create & 0xFF) as u8]);
        // putstatic ModEffects.MOB_EFFECTS
        bytecode.extend_from_slice(&[0xB3, (fref_reg_field >> 8) as u8, (fref_reg_field & 0xFF) as u8]);

        // getstatic ModEffects.MOB_EFFECTS
        bytecode.extend_from_slice(&[0xB2, (fref_reg_field >> 8) as u8, (fref_reg_field & 0xFF) as u8]);
        // ldc str_frost
        bytecode.extend_from_slice(&[0x12, str_frost as u8]);
        // getstatic MobEffectCategory.HARMFUL
        bytecode.extend_from_slice(&[0xB2, (fref_harmful >> 8) as u8, (fref_harmful & 0xFF) as u8]);
        // ldc int_color
        bytecode.extend_from_slice(&[0x12, int_color as u8]);
        // aconst_null
        bytecode.push(0x01);
        // invokevirtual DeferredRegister.register
        bytecode.extend_from_slice(&[0xB6, (mref_register >> 8) as u8, (mref_register & 0xFF) as u8]);
        // return
        bytecode.push(0xB1);

        let method = MethodInfo {
            access_flags: 0x0008,
            name_index: utf_clinit,
            descriptor_index: utf_void,
            name: "<clinit>".to_string(),
            descriptor: "()V".to_string(),
            attributes: vec![AttributeInfo {
                name_index: utf_code,
                name: "Code".to_string(),
                data: Vec::new(),
            }],
            code: Some(CodeInfo {
                max_stack: 5,
                max_locals: 0,
                bytecode,
            }),
        };

        let class_file = ClassFile {
            minor_version: 0,
            major_version: 61,
            constant_pool: cp,
            access_flags: 0x0001,
            this_class: cls_mod,
            super_class: 0,
            interfaces: Vec::new(),
            fields: Vec::new(),
            methods: vec![method],
            attributes: Vec::new(),
        };

        let meta = mock_meta();
        let mut ir = ContentIR::new(meta.clone());
        scan_effect_classes(&[class_file], &meta, &mut ir);

        assert_eq!(ir.mob_effects.len(), 1);
        let frost = &ir.mob_effects[0];
        assert_eq!(frost.id, "ebwizardry:frost");
        assert_eq!(frost.category, "harmful");
        assert_eq!(frost.color, 0x3399FF);
    }

    #[test]
    fn test_scan_fabric_status_effect_registration() {
        let mut cp = ConstantPool::new();
        let utf_clinit = push_utf8(&mut cp, "<clinit>");
        let utf_void = push_utf8(&mut cp, "()V");
        let utf_code = push_utf8(&mut cp, "Code");

        let cls_registry = push_class(&mut cp, "net/minecraft/core/Registry");
        let nat_register = push_name_and_type(
            &mut cp,
            "register",
            "(Lnet/minecraft/core/Registry;Ljava/lang/String;Ljava/lang/Object;)Ljava/lang/Object;",
        );
        let mref_register = push_method_ref(&mut cp, cls_registry, nat_register);

        let cls_registries = push_class(&mut cp, "net/minecraft/core/registries/BuiltInRegistries");
        let nat_status_effect = push_name_and_type(
            &mut cp,
            "STATUS_EFFECT",
            "Lnet/minecraft/core/Registry;",
        );
        let fref_status_effect = push_field_ref(&mut cp, cls_registries, nat_status_effect);

        let str_fireskin = push_string(&mut cp, "fireskin");
        let cls_category = push_class(&mut cp, "net/minecraft/world/effect/MobEffectCategory");
        let nat_beneficial = push_name_and_type(
            &mut cp,
            "BENEFICIAL",
            "Lnet/minecraft/world/effect/MobEffectCategory;",
        );
        let fref_beneficial = push_field_ref(&mut cp, cls_category, nat_beneficial);

        cp.push(ConstantPoolEntry::Integer(0xFF6600));
        let int_color = (cp.len() - 1) as u16;

        let cls_mod = push_class(&mut cp, "com/example/FabricEffects");

        let mut bytecode = Vec::new();
        // getstatic BuiltInRegistries.STATUS_EFFECT
        bytecode.extend_from_slice(&[0xB2, (fref_status_effect >> 8) as u8, (fref_status_effect & 0xFF) as u8]);
        // ldc str_fireskin
        bytecode.extend_from_slice(&[0x12, str_fireskin as u8]);
        // getstatic MobEffectCategory.BENEFICIAL
        bytecode.extend_from_slice(&[0xB2, (fref_beneficial >> 8) as u8, (fref_beneficial & 0xFF) as u8]);
        // ldc int_color
        bytecode.extend_from_slice(&[0x12, int_color as u8]);
        // aconst_null
        bytecode.push(0x01);
        // invokestatic Registry.register
        bytecode.extend_from_slice(&[0xB8, (mref_register >> 8) as u8, (mref_register & 0xFF) as u8]);
        // pop
        bytecode.push(0x57);
        // return
        bytecode.push(0xB1);

        let method = MethodInfo {
            access_flags: 0x0008,
            name_index: utf_clinit,
            descriptor_index: utf_void,
            name: "<clinit>".to_string(),
            descriptor: "()V".to_string(),
            attributes: vec![AttributeInfo {
                name_index: utf_code,
                name: "Code".to_string(),
                data: Vec::new(),
            }],
            code: Some(CodeInfo {
                max_stack: 5,
                max_locals: 0,
                bytecode,
            }),
        };

        let class_file = ClassFile {
            minor_version: 0,
            major_version: 61,
            constant_pool: cp,
            access_flags: 0x0001,
            this_class: cls_mod,
            super_class: 0,
            interfaces: Vec::new(),
            fields: Vec::new(),
            methods: vec![method],
            attributes: Vec::new(),
        };

        let meta = ModMetadata {
            id: "wizardry_fabric".to_string(),
            name: "Wizardry Fabric".to_string(),
            version: "1.0.0".to_string(),
            description: "Fabric effects".to_string(),
            authors: vec!["FabricDev".to_string()],
            loader: ModLoader::Fabric,
        };

        let mut ir = ContentIR::new(meta.clone());
        scan_effect_classes(&[class_file], &meta, &mut ir);

        assert_eq!(ir.mob_effects.len(), 1);
        let fireskin = &ir.mob_effects[0];
        assert_eq!(fireskin.id, "wizardry_fabric:fireskin");
        assert_eq!(fireskin.category, "beneficial");
        assert_eq!(fireskin.color, 0xFF6600);
    }
}
