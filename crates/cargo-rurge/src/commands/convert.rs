use clap::Args;
use rurge_converter::{convert_jar_to_rmod, inspect_jar, CompatibilityStatus, ModLoader};
use std::path::PathBuf;

use crate::CargoRurgeError;

/// Arguments for `cargo rurge convert`.
#[derive(Args, Debug, Clone, Default)]
pub struct ConvertArgs {
    /// Path to legacy Minecraft mod .jar archive
    pub jar_path: PathBuf,

    /// Destination path for generated .rmod archive
    #[arg(short, long)]
    pub output: Option<PathBuf>,

    /// Inspect mod contents and compatibility without generating package
    #[arg(long)]
    pub inspect: bool,
}

/// Executes the JAR-to-RMOD conversion or inspection CLI command.
pub fn execute(args: ConvertArgs) -> Result<(), CargoRurgeError> {
    let jar_display = args.jar_path.display();
    println!("[Rurge Converter] Analyzing: {jar_display}");

    if args.inspect {
        let inspection = inspect_jar(&args.jar_path)?;
        let loader_str = format_loader(inspection.metadata.loader);
        println!(
            "[✓] Detected: {} - {} v{}",
            loader_str, inspection.metadata.name, inspection.metadata.version
        );
        println!(
            "[✓] Extracted: {} blocks, {} items, {} recipes",
            inspection.blocks_count, inspection.items_count, inspection.recipes_count
        );
        println!(
            "[✓] Normalized: {} textures, {} models, {} lang entries",
            inspection.textures_count, inspection.models_count, inspection.lang_keys_count
        );
        let status_str = format_status(inspection.compatibility.status);
        println!("[✓] Compatibility: {status_str}");

        for warning in &inspection.compatibility.warnings {
            println!("  [!] Warning: {warning}");
        }
        for unsupported in &inspection.compatibility.unsupported_features {
            println!("  [!] Unsupported feature: {unsupported}");
        }
        println!("[✓] Inspection complete (no package generated)");
    } else {
        let summary = convert_jar_to_rmod(&args.jar_path, args.output.as_deref())?;
        let loader_str = format_loader(summary.metadata.loader);
        println!(
            "[✓] Detected: {} - {} v{}",
            loader_str, summary.metadata.name, summary.metadata.version
        );
        println!(
            "[✓] Extracted: {} blocks, {} items, {} recipes",
            summary.blocks_count, summary.items_count, summary.recipes_count
        );
        println!(
            "[✓] Normalized: {} textures, {} models, {} lang entries",
            summary.textures_count, summary.models_count, summary.lang_keys_count
        );
        let status_str = format_status(summary.compatibility.status);
        println!("[✓] Compatibility: {status_str}");

        for warning in &summary.compatibility.warnings {
            println!("  [!] Warning: {warning}");
        }
        println!("[✓] Generated: {}", summary.output_path.display());
    }

    Ok(())
}

fn format_loader(loader: ModLoader) -> &'static str {
    match loader {
        ModLoader::Fabric => "Fabric",
        ModLoader::Quilt => "Quilt",
        ModLoader::Forge => "Forge",
        ModLoader::NeoForge => "NeoForge",
        ModLoader::LegacyForge => "Legacy Forge",
        ModLoader::Unknown => "Unknown",
    }
}

fn format_status(status: CompatibilityStatus) -> &'static str {
    match status {
        CompatibilityStatus::Full => "FULL",
        CompatibilityStatus::Partial => "PARTIAL",
        CompatibilityStatus::Skipped => "SKIPPED",
    }
}
