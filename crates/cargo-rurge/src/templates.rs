use clap::ValueEnum;

/// Template flavors available for new Rurge mod projects.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum, Default)]
pub enum TemplateFlavor {
    /// HUD overlay and UI component mod template
    #[default]
    Hud,
    /// Custom block, item, and entity mod template
    Block,
    /// Custom post-processing and pipeline shader mod template
    Shader,
}

impl TemplateFlavor {
    /// Human-readable description of the flavor.
    pub fn description(&self) -> &'static str {
        match self {
            Self::Hud => "HUD overlay mod for Rurge",
            Self::Block => "Custom blocks mod for Rurge",
            Self::Shader => "Custom shaders mod for Rurge",
        }
    }
}

/// Generates the `Cargo.toml` file content for a new mod crate.
pub fn generate_cargo_toml(mod_id: &str) -> String {
    // Rust crate names conventionally use underscores instead of hyphens
    let crate_name = mod_id.replace('-', "_");
    format!(
        r#"[package]
name = "{crate_name}"
version = "0.1.0"
edition = "2024"
authors = ["Mod Author <author@example.com>"]

[lib]
crate-type = ["cdylib", "rlib"]

[dependencies]
rurge-api = "0.1.0"
abi_stable = "0.11"
"#
    )
}

/// Generates the `rurge.toml` manifest file content for a new mod.
pub fn generate_rurge_toml(mod_id: &str, flavor: TemplateFlavor) -> String {
    let description = flavor.description();
    format!(
        r#"id = "{mod_id}"
name = "{mod_id}"
version = "0.1.0"
description = "{description}"
authors = ["Mod Author <author@example.com>"]
environment = "both"

[dependencies]

[incompatibilities]
"#
    )
}

/// Generates the `src/lib.rs` file content implementing `RurgePlugin`.
pub fn generate_lib_rs(mod_id: &str, flavor: TemplateFlavor) -> String {
    let (struct_name, flavor_comment) = match flavor {
        TemplateFlavor::Hud => ("HudMod", "HUD overlay and UI rendering"),
        TemplateFlavor::Block => ("BlockMod", "Custom block and item registration"),
        TemplateFlavor::Shader => ("ShaderMod", "Shader pipeline and post-processing effects"),
    };

    let description = flavor.description();

    format!(
        r#"use abi_stable::std_types::RString;
use rurge_api::{{export_rurge_plugin, ModMetadata, RurgeContext, RurgePlugin}};

/// Main plugin structure for {mod_id} ({flavor_comment}).
#[derive(Default)]
pub struct {struct_name} {{
    // Add plugin state fields here
}}

impl RurgePlugin for {struct_name} {{
    fn metadata(&self) -> ModMetadata {{
        ModMetadata::new(
            "{mod_id}",
            "{mod_id}",
            "0.1.0",
            "{description}",
            vec![RString::from("Mod Author")],
        )
    }}

    fn on_initialize(&mut self, _ctx: &mut RurgeContext) {{
        // Initialization logic for {flavor_comment}
    }}

    fn on_shutdown(&mut self) {{
        // Resource cleanup logic
    }}
}}

// Export the plugin using C ABI symbol for dynamic loading
export_rurge_plugin!({struct_name});
"#
    )
}
