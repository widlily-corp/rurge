use rurge_converter::{
    ContentSchema, SchemaBlock, SchemaItem, SchemaRecipe, SchemaRecipeResult,
    SchemaToolStats, ToolType,
};
use rurge_runtime::content_schema::ContentSchemaExecutor;
use rurge_runtime::engine::RurgeEngine;
use std::collections::HashMap;

fn create_sample_schema(mod_id: &str) -> ContentSchema {
    let block = SchemaBlock {
        id: "fancy_copper_block".to_string(),
        hardness: 3.0,
        resistance: 6.0,
        tool: Some(ToolType::Pickaxe),
        harvest_level: 1,
        sound_group: "metal".to_string(),
        light_emission: 0,
        opaque: true,
        model: Some(format!("{mod_id}:block/fancy_copper_block")),
        triggers: Vec::new(),
    };

    let item = SchemaItem {
        id: "copper_wrench".to_string(),
        max_stack_size: 1,
        durability: 250,
        tool: Some(SchemaToolStats {
            tool_type: ToolType::Pickaxe,
            speed: 6.0,
            attack_damage: 3.0,
            mining_level: 1,
        }),
        food: None,
        rarity: "common".to_string(),
        model: Some(format!("{mod_id}:item/copper_wrench")),
        actions: Vec::new(),
    };

    let mut key = HashMap::new();
    key.insert("C".to_string(), "minecraft:copper_ingot".to_string());
    key.insert("S".to_string(), "minecraft:stick".to_string());

    let shaped_recipe = SchemaRecipe::Shaped {
        id: Some(format!("{mod_id}:copper_wrench_craft")),
        pattern: vec![" C ".into(), " S ".into(), " S ".into()],
        key,
        result: SchemaRecipeResult {
            item: format!("{mod_id}:copper_wrench"),
            count: 1,
        },
    };

    let shapeless_recipe = SchemaRecipe::Shapeless {
        id: Some(format!("{mod_id}:copper_nuggets_craft")),
        ingredients: vec![format!("{mod_id}:copper_wrench")],
        result: SchemaRecipeResult {
            item: "minecraft:copper_ingot".to_string(),
            count: 1,
        },
    };

    ContentSchema {
        schema_version: "1.0".to_string(),
        mod_id: mod_id.to_string(),
        blocks: vec![block],
        items: vec![item],
        recipes: vec![shaped_recipe, shapeless_recipe],
        item_actions: Vec::new(),
        block_triggers: Vec::new(),
    }
}

#[test]
fn test_schema_executor_empty() {
    let executor = ContentSchemaExecutor::new();
    assert_eq!(executor.block_count(), 0);
    assert_eq!(executor.item_count(), 0);
    assert_eq!(executor.recipe_count(), 0);
    assert!(executor.all_blocks().is_empty());
    assert!(executor.all_items().is_empty());
    assert!(executor.recipes().is_empty());
    assert_eq!(executor.get_block("unknown:block"), None);
    assert_eq!(executor.get_item("unknown:item"), None);
}

#[test]
fn test_register_schema_and_query() {
    let executor = ContentSchemaExecutor::new();
    let schema = create_sample_schema("copper_plus");

    assert!(executor.register_schema("copper_plus", &schema).is_ok());

    assert_eq!(executor.block_count(), 1);
    assert_eq!(executor.item_count(), 1);
    assert_eq!(executor.recipe_count(), 2);

    // Query full ID
    let block = executor.get_block("copper_plus:fancy_copper_block");
    assert!(block.is_some());
    let b = block.unwrap();
    assert_eq!(b.id, "copper_plus:fancy_copper_block");
    assert_eq!(b.hardness, 3.0);
    assert_eq!(b.tool, Some(ToolType::Pickaxe));

    // Query short ID fallback
    let block_short = executor.get_block("fancy_copper_block");
    assert!(block_short.is_some());
    assert_eq!(block_short.unwrap().id, "copper_plus:fancy_copper_block");

    // Query full item ID
    let item = executor.get_item("copper_plus:copper_wrench");
    assert!(item.is_some());
    let it = item.unwrap();
    assert_eq!(it.id, "copper_plus:copper_wrench");
    assert_eq!(it.durability, 250);
    assert_eq!(it.tool.as_ref().unwrap().speed, 6.0);

    // Query short item ID fallback
    let item_short = executor.get_item("copper_wrench");
    assert!(item_short.is_some());
    assert_eq!(item_short.unwrap().id, "copper_plus:copper_wrench");

    // All blocks and items
    let blocks = executor.all_blocks();
    assert_eq!(blocks.len(), 1);
    assert_eq!(blocks[0].id, "copper_plus:fancy_copper_block");

    let items = executor.all_items();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].id, "copper_plus:copper_wrench");

    let recipes = executor.recipes();
    assert_eq!(recipes.len(), 2);

    // Origin tracking
    assert_eq!(
        executor.origin_for("copper_plus:fancy_copper_block").as_deref(),
        Some("copper_plus")
    );
    assert_eq!(
        executor.origin_for("copper_plus:copper_wrench").as_deref(),
        Some("copper_plus")
    );
}

#[test]
fn test_schema_multi_mod_isolation() {
    let executor = ContentSchemaExecutor::new();
    let schema_a = create_sample_schema("mod_a");
    let schema_b = create_sample_schema("mod_b");

    assert!(executor.register_schema("mod_a", &schema_a).is_ok());
    assert!(executor.register_schema("mod_b", &schema_b).is_ok());

    assert_eq!(executor.block_count(), 2);
    assert_eq!(executor.item_count(), 2);
    assert_eq!(executor.recipe_count(), 4);

    let block_a = executor.get_block("mod_a:fancy_copper_block");
    let block_b = executor.get_block("mod_b:fancy_copper_block");
    assert!(block_a.is_some());
    assert!(block_b.is_some());
    assert_ne!(block_a.unwrap().id, block_b.unwrap().id);

    assert_eq!(executor.origin_for("mod_a:fancy_copper_block").as_deref(), Some("mod_a"));
    assert_eq!(executor.origin_for("mod_b:fancy_copper_block").as_deref(), Some("mod_b"));

    let mods = executor.loaded_mods();
    assert!(mods.contains(&"mod_a".to_string()));
    assert!(mods.contains(&"mod_b".to_string()));

    let blocks_a = executor.blocks_for_mod("mod_a");
    assert_eq!(blocks_a.len(), 1);
    let blocks_b = executor.blocks_for_mod("mod_b");
    assert_eq!(blocks_b.len(), 1);
}

#[test]
fn test_schema_conflict_error() {
    let executor = ContentSchemaExecutor::new();
    let schema_a = create_sample_schema("mod_a");
    assert!(executor.register_schema("mod_a", &schema_a).is_ok());

    // mod_b tries to register a block with full id already belonging to mod_a
    let mut bad_schema = create_sample_schema("mod_b");
    bad_schema.blocks[0].id = "mod_a:fancy_copper_block".to_string();

    let res = executor.register_schema("mod_b", &bad_schema);
    assert!(res.is_err());
    let err_msg = res.unwrap_err();
    assert!(err_msg.contains("already registered"));
}

#[test]
fn test_engine_content_integration() {
    let engine = RurgeEngine::new();
    let content = engine.content();
    assert_eq!(content.block_count(), 0);
    assert_eq!(content.item_count(), 0);

    let schema = create_sample_schema("engine_test_mod");
    assert!(content.register_schema("engine_test_mod", &schema).is_ok());
    assert_eq!(engine.content().block_count(), 1);
}
