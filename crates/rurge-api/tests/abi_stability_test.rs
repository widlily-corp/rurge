use abi_stable::{
    abi_stability::check_layout_compatibility,
    std_types::{RBox, RString},
    StableAbi,
};
use rurge_api::{
    export_rurge_plugin,
    lifecycle::{InitEvent, PostInitEvent, PreInitEvent, ServerStartingEvent, ServerStoppingEvent},
    ModMetadata, RurgeContext, RurgePlugin, RurgePlugin_TO,
};

#[derive(Default)]
struct TestMod {
    initialized: bool,
    shutdown: bool,
}

impl RurgePlugin for TestMod {
    fn metadata(&self) -> ModMetadata {
        ModMetadata::new(
            "test_mod",
            "Test Mod",
            "1.0.0",
            "A test mod for ABI stability",
            vec![RString::from("Developer")],
        )
    }

    fn on_initialize(&mut self, ctx: &mut RurgeContext) {
        assert_eq!(ctx.engine_version.as_str(), "0.1.0");
        self.initialized = true;
    }

    fn on_shutdown(&mut self) {
        self.shutdown = true;
    }
}

// Test macro export functionality
export_rurge_plugin!(TestMod);

#[test]
fn test_mod_metadata_abi_stability() {
    let layout = <ModMetadata as StableAbi>::LAYOUT;
    assert!(
        check_layout_compatibility(layout, layout).is_ok(),
        "ModMetadata layout must be compatible with itself"
    );

    let meta = ModMetadata::new(
        "example_mod",
        "Example Mod",
        "0.1.0",
        "An example mod for Horizon",
        vec![RString::from("Author A"), RString::from("Author B")],
    );

    assert_eq!(meta.id.as_str(), "example_mod");
    assert_eq!(meta.name.as_str(), "Example Mod");
    assert_eq!(meta.version.as_str(), "0.1.0");
    assert_eq!(meta.description.as_str(), "An example mod for Horizon");
    assert_eq!(meta.authors.len(), 2);
    assert_eq!(meta.authors[0].as_str(), "Author A");
}

#[test]
fn test_rurge_context_abi_stability() {
    let layout = <RurgeContext as StableAbi>::LAYOUT;
    assert!(
        check_layout_compatibility(layout, layout).is_ok(),
        "RurgeContext layout must be compatible with itself"
    );

    let ctx = RurgeContext::new("0.1.0", "1.20.4", "mods", "config");
    assert_eq!(ctx.engine_version.as_str(), "0.1.0");
    assert_eq!(ctx.game_version.as_str(), "1.20.4");
    assert_eq!(ctx.mod_dir.as_str(), "mods");
    assert_eq!(ctx.config_dir.as_str(), "config");
}

#[test]
fn test_plugin_trait_object_abi_stability() {
    type PluginTO = RurgePlugin_TO<'static, RBox<()>>;
    let layout = <PluginTO as StableAbi>::LAYOUT;
    assert!(
        check_layout_compatibility(layout, layout).is_ok(),
        "RurgePlugin_TO layout must be compatible with itself"
    );

    // Instantiate plugin through trait object
    let mut plugin_to = _rurge_plugin_create();
    let meta = plugin_to.metadata();
    assert_eq!(meta.id.as_str(), "test_mod");

    let mut ctx = RurgeContext::new("0.1.0", "1.20.4", "mods", "config");
    plugin_to.on_initialize(&mut ctx);
    plugin_to.on_shutdown();
}

#[test]
fn test_lifecycle_events_abi_stability() {
    assert!(check_layout_compatibility(
        <PreInitEvent as StableAbi>::LAYOUT,
        <PreInitEvent as StableAbi>::LAYOUT
    )
    .is_ok());

    assert!(check_layout_compatibility(
        <InitEvent as StableAbi>::LAYOUT,
        <InitEvent as StableAbi>::LAYOUT
    )
    .is_ok());

    assert!(check_layout_compatibility(
        <PostInitEvent as StableAbi>::LAYOUT,
        <PostInitEvent as StableAbi>::LAYOUT
    )
    .is_ok());

    assert!(check_layout_compatibility(
        <ServerStartingEvent as StableAbi>::LAYOUT,
        <ServerStartingEvent as StableAbi>::LAYOUT
    )
    .is_ok());

    assert!(check_layout_compatibility(
        <ServerStoppingEvent as StableAbi>::LAYOUT,
        <ServerStoppingEvent as StableAbi>::LAYOUT
    )
    .is_ok());
}
