use abi_stable::sabi_trait;
use crate::context::RurgeContext;
use crate::metadata::ModMetadata;

/// Core plugin trait that all Rurge native mods must implement.
///
/// This trait uses `abi_stable::sabi_trait` to produce an ABI-stable trait object
/// `RurgePlugin_TO` that can safely be invoked across dynamic library boundaries.
#[sabi_trait]
pub trait RurgePlugin: Send + Sync {
    /// Returns the metadata describing this mod.
    fn metadata(&self) -> ModMetadata;

    /// Invoked when the mod is loaded and the engine initializes.
    fn on_initialize(&mut self, ctx: &mut RurgeContext);

    /// Invoked when the game or engine is shutting down.
    fn on_shutdown(&mut self);
}

/// Macro to export a Rurge plugin from a dynamic library.
///
/// Exports the `_rurge_plugin_create` symbol with C ABI.
#[macro_export]
macro_rules! export_rurge_plugin {
    ($plugin_type:ty) => {
        #[unsafe(no_mangle)]
        pub extern "C" fn _rurge_plugin_create(
        ) -> $crate::plugin::RurgePlugin_TO<'static, $crate::abi_stable::std_types::RBox<()>> {
            $crate::plugin::RurgePlugin_TO::from_value(
                <$plugin_type as ::core::default::Default>::default(),
                $crate::abi_stable::erased_types::TD_Opaque,
            )
        }
    };
    ($plugin_type:ty, $constructor:expr) => {
        #[unsafe(no_mangle)]
        pub extern "C" fn _rurge_plugin_create(
        ) -> $crate::plugin::RurgePlugin_TO<'static, $crate::abi_stable::std_types::RBox<()>> {
            let plugin: $plugin_type = ($constructor)();
            $crate::plugin::RurgePlugin_TO::from_value(
                plugin,
                $crate::abi_stable::erased_types::TD_Opaque,
            )
        }
    };
}
