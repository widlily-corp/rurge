use rurge_converter::ir::ItemDef;
use std::collections::BTreeMap;

#[test]
fn test_canonical_spell_book_has_custom_data_components() {
    let mut item = ItemDef::new("ebwizardry:spell_book");
    let mut custom_data = BTreeMap::new();
    custom_data.insert("spell".to_string(), serde_json::json!("ebwizardry:fireball"));
    custom_data.insert("element".to_string(), serde_json::json!("fire"));
    custom_data.insert("tier".to_string(), serde_json::json!("apprentice"));

    item = item.with_component("minecraft:custom_data", serde_json::json!(custom_data));
    assert!(item.components.contains_key("minecraft:custom_data"));
    let cd = &item.components["minecraft:custom_data"];
    assert_eq!(cd["spell"], "ebwizardry:fireball");
    assert_eq!(cd["element"], "fire");
    assert_eq!(cd["tier"], "apprentice");
}

#[test]
fn test_canonical_magic_wand_has_mana_components() {
    let mut wand = ItemDef::new("ebwizardry:magic_wand");
    let mut custom_data = BTreeMap::new();
    custom_data.insert("mana".to_string(), serde_json::json!(1400));
    custom_data.insert("max_mana".to_string(), serde_json::json!(1400));
    custom_data.insert("tier".to_string(), serde_json::json!("master"));

    wand = wand.with_component("minecraft:custom_data", serde_json::json!(custom_data));
    assert_eq!(wand.components["minecraft:custom_data"]["mana"], 1400);
}
