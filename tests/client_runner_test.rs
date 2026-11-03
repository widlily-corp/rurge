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
