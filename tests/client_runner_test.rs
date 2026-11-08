use rurge_runtime::engine::RurgeEngine;
use rurge_horizon::hooks::RurgeHorizonHooks;
use std::sync::Arc;
#[allow(unused_imports)]
use horizon_client::mod_api::Hooks;

#[test]
fn test_rurge_horizon_bootstrap_smoke() {
    let engine = Arc::new(RurgeEngine::new());
    let hooks = RurgeHorizonHooks::new(engine.clone());
    let mods: horizon_client::mod_api::Mods = Arc::new([Box::new(hooks)]);
    assert_eq!(mods.len(), 1);
    assert!(mods[0].render().is_some());
}

#[test]
fn test_ebwizardry_mod_loading_and_content_queries() {
    let rmod_path = std::path::Path::new("mods/ebwizardry-1.0.2-alpha.rmod");
    if !rmod_path.exists() {
        return;
    }

    let mut engine = RurgeEngine::new();
    engine.initialize().expect("Engine should initialize and load mods/");

    let content = engine.content();
    println!("Loaded blocks: {}", content.block_count());
    println!("Loaded items: {}", content.item_count());
    println!("Loaded mods: {:?}", content.loaded_mods());

    assert!(content.block_count() >= 54, "Should load at least 54 blocks");
    assert!(content.item_count() >= 185, "Should load at least 185 items");
    assert!(content.loaded_mods().contains(&"ebwizardry".to_string()));

    let engine_arc = Arc::new(engine);
    let hooks = RurgeHorizonHooks::new(engine_arc);
    let mut native_mods = Vec::new();
    hooks.populate_native_mods(&mut native_mods);

    let eb_entry = native_mods
        .iter()
        .find(|m| m.id.contains("ebwizardry"))
        .expect("ebwizardry mod must appear in native_mods list");
    println!("NativeMod title: {}", eb_entry.title);
    println!("NativeMod description: {}", eb_entry.description);
    assert!(eb_entry.title.contains("ebwizardry"));
    assert!(eb_entry.description.contains("Blocks: 54"));
    assert!(eb_entry.description.contains("Items: 1421"));
}
