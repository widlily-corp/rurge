use rurge_converter::{
    audit_compatibility, extract_assets_from_zip, parse_lang_json, parse_legacy_lang,
    parse_recipe_json, resolve_model_parents, BlockDef, ClassFile, CompatibilityStatus, ContentIR,
    ContentSchema, ItemDef, ModLoader, ModMetadata, RecipeDef, SchemaRecipe, ToolStats, ToolType,
};
use std::collections::HashMap;
use std::io::{Cursor, Write};
use zip::write::SimpleFileOptions;
use zip::ZipWriter;

struct TestClassBuilder {
    cp_entries: Vec<Vec<u8>>,
    this_class: u16,
    super_class: u16,
    methods: Vec<Vec<u8>>,
}

impl TestClassBuilder {
    fn new(this_class_name: &str, super_class_name: &str) -> Self {
        let mut builder = Self {
            cp_entries: Vec::new(),
            this_class: 0,
            super_class: 0,
            methods: Vec::new(),
        };
        builder.this_class = builder.add_class(this_class_name);
        builder.super_class = builder.add_class(super_class_name);
        builder
    }

    fn add_utf8(&mut self, s: &str) -> u16 {
        let mut entry = Vec::new();
        entry.push(1);
        let bytes = s.as_bytes();
        entry.extend_from_slice(&(bytes.len() as u16).to_be_bytes());
        entry.extend_from_slice(bytes);
        self.cp_entries.push(entry);
        self.cp_entries.len() as u16
    }

    fn add_class(&mut self, name: &str) -> u16 {
        let name_idx = self.add_utf8(name);
        let mut entry = Vec::new();
        entry.push(7);
        entry.extend_from_slice(&name_idx.to_be_bytes());
        self.cp_entries.push(entry);
        self.cp_entries.len() as u16
    }

    fn add_method(&mut self, name: &str, descriptor: &str) {
        let name_idx = self.add_utf8(name);
        let desc_idx = self.add_utf8(descriptor);
        let mut method = Vec::new();
        method.extend_from_slice(&0x0001u16.to_be_bytes()); // ACC_PUBLIC
        method.extend_from_slice(&name_idx.to_be_bytes());
        method.extend_from_slice(&desc_idx.to_be_bytes());
        method.extend_from_slice(&0u16.to_be_bytes()); // 0 attributes
        self.methods.push(method);
    }

    fn build(&self) -> Vec<u8> {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&0xCAFEBABEu32.to_be_bytes());
        bytes.extend_from_slice(&0u16.to_be_bytes()); // minor
        bytes.extend_from_slice(&61u16.to_be_bytes()); // major (Java 17)
        let cp_count = (self.cp_entries.len() + 1) as u16;
        bytes.extend_from_slice(&cp_count.to_be_bytes());
        for entry in &self.cp_entries {
            bytes.extend_from_slice(entry);
        }
        bytes.extend_from_slice(&0x0021u16.to_be_bytes()); // access flags
        bytes.extend_from_slice(&self.this_class.to_be_bytes());
        bytes.extend_from_slice(&self.super_class.to_be_bytes());
        bytes.extend_from_slice(&0u16.to_be_bytes()); // interfaces count
        bytes.extend_from_slice(&0u16.to_be_bytes()); // fields count
        bytes.extend_from_slice(&(self.methods.len() as u16).to_be_bytes());
        for method in &self.methods {
            bytes.extend_from_slice(method);
        }
        bytes.extend_from_slice(&0u16.to_be_bytes()); // attributes count
        bytes
    }
}

#[test]
fn test_parse_crafting_shaped_recipe() {
    let json = r#"{
        "type": "minecraft:crafting_shaped",
        "pattern": [
            " C ",
            " S ",
            " S "
        ],
        "key": {
            "C": {
                "item": "minecraft:copper_ingot"
            },
            "S": {
                "item": "minecraft:stick"
            }
        },
        "result": {
            "item": "copper_plus:copper_wrench",
            "count": 1
        }
    }"#;

    let recipe = parse_recipe_json("copper_plus:copper_wrench", json)
        .expect("should parse")
        .expect("should be some");

    match recipe {
        RecipeDef::Shaped {
            id,
            pattern,
            key,
            result_item,
            result_count,
        } => {
            assert_eq!(id, "copper_plus:copper_wrench");
            assert_eq!(pattern, vec![" C ", " S ", " S "]);
            assert_eq!(
                key.get("C").map(String::as_str),
                Some("minecraft:copper_ingot")
            );
            assert_eq!(key.get("S").map(String::as_str), Some("minecraft:stick"));
            assert_eq!(result_item, "copper_plus:copper_wrench");
            assert_eq!(result_count, 1);
        }
        _ => panic!("Expected Shaped recipe"),
    }
}

#[test]
fn test_parse_crafting_shapeless_recipe() {
    let json = r#"{
        "type": "minecraft:crafting_shapeless",
        "ingredients": [
            { "item": "minecraft:copper_ingot" },
            { "item": "minecraft:redstone" }
        ],
        "result": {
            "item": "copper_plus:copper_wire",
            "count": 4
        }
    }"#;

    let recipe = parse_recipe_json("copper_plus:copper_wire", json)
        .expect("should parse")
        .expect("should be some");

    match recipe {
        RecipeDef::Shapeless {
            id,
            ingredients,
            result_item,
            result_count,
        } => {
            assert_eq!(id, "copper_plus:copper_wire");
            assert_eq!(
                ingredients,
                vec!["minecraft:copper_ingot", "minecraft:redstone"]
            );
            assert_eq!(result_item, "copper_plus:copper_wire");
            assert_eq!(result_count, 4);
        }
        _ => panic!("Expected Shapeless recipe"),
    }
}

#[test]
fn test_parse_smelting_and_blasting_recipe() {
    let json = r#"{
        "type": "minecraft:smelting",
        "ingredient": {
            "item": "copper_plus:raw_copper"
        },
        "result": {
            "item": "copper_plus:copper_ingot",
            "count": 1
        },
        "experience": 0.7,
        "cookingtime": 200
    }"#;

    let recipe = parse_recipe_json("copper_plus:smelt_copper", json)
        .expect("should parse")
        .expect("should be some");

    match recipe {
        RecipeDef::Smelting {
            id,
            ingredient,
            result_item,
            experience,
            cooking_time,
        } => {
            assert_eq!(id, "copper_plus:smelt_copper");
            assert_eq!(ingredient, "copper_plus:raw_copper");
            assert_eq!(result_item, "copper_plus:copper_ingot");
            assert!((experience - 0.7).abs() < 1e-4);
            assert_eq!(cooking_time, 200);
        }
        _ => panic!("Expected Smelting recipe"),
    }
}

#[test]
fn test_parse_lang_json_and_legacy() {
    let json = r#"{"block.mod.ore": "Ruby Ore", "item.mod.gem": "Ruby"}"#;
    let map = parse_lang_json(json).expect("valid JSON lang");
    assert_eq!(map.get("block.mod.ore").unwrap(), "Ruby Ore");
    assert_eq!(map.get("item.mod.gem").unwrap(), "Ruby");

    let legacy = "tile.ore.name = Ruby Ore\n# comment\nitem.gem.name = Ruby\n";
    let legacy_map = parse_legacy_lang(legacy);
    assert_eq!(legacy_map.get("tile.ore.name").unwrap(), "Ruby Ore");
    assert_eq!(legacy_map.get("item.gem.name").unwrap(), "Ruby");
}

#[test]
fn test_model_parent_recursion_safety() {
    let mut models = HashMap::new();

    // Model A -> Model B -> Model A (circular cycle of length 2)
    models.insert(
        "mod:block/a".to_string(),
        br#"{"parent": "mod:block/b"}"#.to_vec(),
    );
    models.insert(
        "mod:block/b".to_string(),
        br#"{"parent": "mod:block/a"}"#.to_vec(),
    );

    let res = resolve_model_parents("mod:block/a", &models, 16);
    assert!(
        res.is_err(),
        "Circular model references must return an error and not loop infinitely"
    );

    // Deep chain > 16
    let mut deep_models = HashMap::new();
    for i in 0..20 {
        let current = format!("mod:block/node_{i}");
        let parent = format!("mod:block/node_{}", i + 1);
        deep_models.insert(
            current,
            format!(r#"{{"parent": "{parent}"}}"#).into_bytes(),
        );
    }
    let res_deep = resolve_model_parents("mod:block/node_0", &deep_models, 16);
    assert!(
        res_deep.is_err(),
        "Chains deeper than max_depth (16) must safely abort with an error"
    );

    // Valid hierarchy: Leaf -> Mid -> Root
    let mut valid_models = HashMap::new();
    valid_models.insert(
        "mod:block/fancy".to_string(),
        br#"{"parent": "mod:block/base"}"#.to_vec(),
    );
    valid_models.insert(
        "mod:block/base".to_string(),
        br#"{"parent": "minecraft:block/cube_all"}"#.to_vec(),
    );

    let chain = resolve_model_parents("mod:block/fancy", &valid_models, 16)
        .expect("Valid chain must resolve");
    assert_eq!(
        chain,
        vec![
            "mod:block/fancy",
            "mod:block/base",
            "minecraft:block/cube_all"
        ]
    );
}

#[test]
fn test_extract_assets_from_zip() {
    let mut buf = Vec::new();
    {
        let mut zip = ZipWriter::new(Cursor::new(&mut buf));
        let options = SimpleFileOptions::default();

        zip.start_file("assets/copper_plus/blockstates/copper_block.json", options)
            .unwrap();
        zip.write_all(br#"{"variants": {"": {"model": "copper_plus:block/copper_block"}}}"#)
            .unwrap();

        zip.start_file(
            "assets/copper_plus/models/block/copper_block.json",
            options,
        )
        .unwrap();
        zip.write_all(br#"{"parent": "minecraft:block/cube_all"}"#)
            .unwrap();

        zip.start_file("assets/copper_plus/textures/block/copper_block.png", options)
            .unwrap();
        zip.write_all(&[0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A])
            .unwrap();

        zip.start_file("assets/copper_plus/lang/en_us.json", options)
            .unwrap();
        zip.write_all(br#"{"block.copper_plus.copper_block": "Copper Block"}"#)
            .unwrap();

        zip.start_file("assets/copper_plus/lang/legacy.lang", options)
            .unwrap();
        zip.write_all(b"item.copper_wrench.name=Copper Wrench\n# comment\n")
            .unwrap();

        zip.start_file("data/copper_plus/recipes/copper_wrench.json", options)
            .unwrap();
        zip.write_all(
            br#"{
            "type": "minecraft:crafting_shaped",
            "pattern": [" C ", " S ", " S "],
            "key": {
                "C": {"item": "minecraft:copper_ingot"},
                "S": {"item": "minecraft:stick"}
            },
            "result": {"item": "copper_plus:copper_wrench", "count": 1}
        }"#,
        )
        .unwrap();

        zip.finish().unwrap();
    }

    let mut archive = zip::ZipArchive::new(Cursor::new(&buf)).unwrap();
    let extracted = extract_assets_from_zip(&mut archive, Some("copper_plus")).unwrap();

    assert_eq!(extracted.blockstates.len(), 1);
    assert_eq!(extracted.models.len(), 1);
    assert_eq!(extracted.textures.len(), 1);
    assert_eq!(extracted.lang.len(), 2);
    assert_eq!(
        extracted.lang.get("block.copper_plus.copper_block"),
        Some(&"Copper Block".to_string())
    );
    assert_eq!(
        extracted.lang.get("item.copper_wrench.name"),
        Some(&"Copper Wrench".to_string())
    );
    assert_eq!(extracted.recipes.len(), 1);

    // Merge into IR
    let mut ir = ContentIR::new(ModMetadata {
        id: "copper_plus".into(),
        name: "Copper Plus".into(),
        version: "1.0.0".into(),
        authors: vec!["Author".into()],
        description: "Test mod".into(),
        loader: ModLoader::Fabric,
    });
    ir.add_block(BlockDef::new("copper_block"));
    ir.add_item(ItemDef::new("copper_wrench"));

    extracted.merge_into(&mut ir);
    assert_eq!(ir.recipes.len(), 1);
    assert_eq!(ir.lang.len(), 2);
    assert_eq!(
        ir.get_block("copper_block").unwrap().model,
        Some("copper_plus:block/copper_block".into())
    );
}

#[test]
fn test_content_schema_serialization_matches_spec() {
    let mut ir = ContentIR::new(ModMetadata {
        id: "copper_plus".into(),
        name: "Copper Plus".into(),
        version: "1.0.0".into(),
        authors: vec![],
        description: "".into(),
        loader: ModLoader::Fabric,
    });

    let mut block = BlockDef::new("copper_block_fancy");
    block.hardness = 3.0;
    block.resistance = 6.0;
    block.tool = Some(ToolType::Pickaxe);
    block.harvest_level = 1;
    block.sound_group = "metal".to_string();
    block.light_emission = 0;
    block.opaque = true;
    block.model = Some("copper_plus:block/copper_block_fancy".to_string());
    ir.add_block(block);

    let mut item = ItemDef::new("copper_wrench");
    item.max_stack_size = 1;
    item.durability = 250;
    item.tool = Some(ToolStats {
        tool_type: ToolType::Pickaxe,
        speed: 6.0,
        attack_damage: 3.0,
        mining_level: 1,
    });
    item.model = Some("copper_plus:item/copper_wrench".to_string());
    ir.add_item(item);

    let mut key = HashMap::new();
    key.insert("C".to_string(), "minecraft:copper_ingot".to_string());
    key.insert("S".to_string(), "minecraft:stick".to_string());
    ir.add_recipe(RecipeDef::Shaped {
        id: "copper_plus:copper_wrench".to_string(),
        pattern: vec![" C ".into(), " S ".into(), " S ".into()],
        key,
        result_item: "copper_plus:copper_wrench".to_string(),
        result_count: 1,
    });
    ir.add_recipe(RecipeDef::Shapeless {
        id: "copper_plus:copper_nuggets".to_string(),
        ingredients: vec!["copper_plus:copper_ingot".to_string()],
        result_item: "copper_plus:copper_nugget".to_string(),
        result_count: 9,
    });
    ir.add_recipe(RecipeDef::Smelting {
        id: "copper_plus:smelt_ingot".to_string(),
        ingredient: "copper_plus:raw_copper".to_string(),
        result_item: "copper_plus:copper_ingot".to_string(),
        experience: 0.7,
        cooking_time: 200,
    });

    let schema = ContentSchema::from_ir(&ir);
    let json = schema.to_json_pretty().expect("serialization should succeed");

    assert!(json.contains(r#""schema_version": "1.0""#));
    assert!(json.contains(r#""mod_id": "copper_plus""#));
    assert!(json.contains(r#""copper_block_fancy""#));
    assert!(json.contains(r#""copper_wrench""#));
    assert!(json.contains(r#""crafting_shaped""#));
    assert!(json.contains(r#""crafting_shapeless""#));
    assert!(json.contains(r#""smelting""#));

    // Verify roundtrip deserialization
    let deserialized = ContentSchema::from_json_str(&json).expect("deserialization should succeed");
    assert_eq!(deserialized.mod_id, "copper_plus");
    assert_eq!(deserialized.blocks.len(), 1);
    assert_eq!(deserialized.items.len(), 1);
    assert_eq!(deserialized.recipes.len(), 3);
    assert_eq!(deserialized.blocks[0].hardness, 3.0);
    assert_eq!(deserialized.blocks[0].tool, Some(ToolType::Pickaxe));
    assert_eq!(
        deserialized.items[0].tool.as_ref().unwrap().tool_type,
        ToolType::Pickaxe
    );

    let compact = schema.to_json_string().unwrap();
    assert!(compact.contains(r#""schema_version":"1.0""#));

    // Check recipe types deserialized correctly
    let has_shapeless = deserialized
        .recipes
        .iter()
        .any(|r| matches!(r, SchemaRecipe::Shapeless { .. }));
    let has_smelting = deserialized
        .recipes
        .iter()
        .any(|r| matches!(r, SchemaRecipe::Smelting { .. }));
    assert!(has_shapeless);
    assert!(has_smelting);
}

#[test]
fn test_compatibility_auditor_pure_content_mod() {
    let mut ir = ContentIR::new(ModMetadata {
        id: "pure_content".into(),
        name: "Pure Content".into(),
        version: "1.0".into(),
        authors: vec![],
        description: "".into(),
        loader: ModLoader::Fabric,
    });
    ir.add_block(BlockDef::new("ruby_ore"));
    ir.add_item(ItemDef::new("ruby"));

    let classes: Vec<ClassFile> = Vec::new();
    let entries = vec![
        "fabric.mod.json".to_string(),
        "assets/pure_content/models/item/ruby.json".to_string(),
    ];

    let report = audit_compatibility(ModLoader::Fabric, &classes, &entries, &ir);
    assert_eq!(report.status, CompatibilityStatus::Full);
    assert!(report.unsupported_features.is_empty());
    assert!(!report.features_supported.is_empty());

    let report_json = report.to_json_pretty().unwrap();
    assert!(report_json.contains(r#""status": "Full""#));
}

#[test]
fn test_compatibility_auditor_content_with_mixins_is_partial() {
    let mut ir = ContentIR::new(ModMetadata {
        id: "hybrid_mod".into(),
        name: "Hybrid Mod".into(),
        version: "1.0".into(),
        authors: vec![],
        description: "".into(),
        loader: ModLoader::Fabric,
    });
    ir.add_block(BlockDef::new("custom_chest"));

    let classes: Vec<ClassFile> = Vec::new();
    let entries = vec![
        "fabric.mod.json".to_string(),
        "hybrid.mixins.json".to_string(),
    ];

    let report = audit_compatibility(ModLoader::Fabric, &classes, &entries, &ir);
    assert_eq!(report.status, CompatibilityStatus::Partial);
    assert!(!report.warnings.is_empty());
    assert!(report
        .warnings
        .iter()
        .any(|w| w.to_lowercase().contains("mixin")));
}

#[test]
fn test_compatibility_auditor_pure_engine_patcher_is_skipped() {
    let ir = ContentIR::new(ModMetadata {
        id: "sodium".into(),
        name: "Sodium".into(),
        version: "0.5.0".into(),
        authors: vec![],
        description: "Rendering engine".into(),
        loader: ModLoader::Fabric,
    });

    let classes: Vec<ClassFile> = Vec::new();
    let entries = vec![
        "fabric.mod.json".to_string(),
        "sodium.mixins.json".to_string(),
    ];

    let report = audit_compatibility(ModLoader::Fabric, &classes, &entries, &ir);
    assert_eq!(report.status, CompatibilityStatus::Skipped);
    assert!(!report.unsupported_features.is_empty());
    assert!(report.features_supported.is_empty());
}

#[test]
fn test_compatibility_auditor_bytecode_mixin_and_asm_detection() {
    let mut mixin_builder =
        TestClassBuilder::new("com/example/MixinPlayer", "java/lang/Object");
    mixin_builder.add_utf8("org/spongepowered/asm/mixin/Mixin");
    let mixin_class = ClassFile::parse(&mixin_builder.build()).unwrap();

    let mut asm_builder =
        TestClassBuilder::new("com/example/Transformer", "java/lang/Object");
    asm_builder.add_utf8("org/objectweb/asm/ClassVisitor");
    let asm_class = ClassFile::parse(&asm_builder.build()).unwrap();

    let mut ticker_builder = TestClassBuilder::new(
        "com/example/FurnaceBlockEntity",
        "net/minecraft/world/level/block/entity/BlockEntity",
    );
    ticker_builder.add_method("tick", "()V");
    let ticker_class = ClassFile::parse(&ticker_builder.build()).unwrap();

    let classes = vec![mixin_class, asm_class, ticker_class];
    let entries = vec!["fabric.mod.json".to_string()];

    let mut ir = ContentIR::new(ModMetadata {
        id: "advanced_mod".into(),
        name: "Advanced Mod".into(),
        version: "1.0".into(),
        authors: vec![],
        description: "".into(),
        loader: ModLoader::Fabric,
    });
    ir.add_block(BlockDef::new("advanced_furnace"));

    let report = audit_compatibility(ModLoader::Fabric, &classes, &entries, &ir);
    assert_eq!(report.status, CompatibilityStatus::Partial);
    assert!(report
        .unsupported_features
        .iter()
        .any(|f| f.contains("Mixins")));
    assert!(report
        .unsupported_features
        .iter()
        .any(|f| f.contains("ASM")));
    assert!(report
        .warnings
        .iter()
        .any(|w| w.contains("ticking BlockEntity")));
}
