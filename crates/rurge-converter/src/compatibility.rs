//! Compatibility auditor detecting Mixins, ASM, and dynamic logic.

use crate::bytecode::{ClassFile, ConstantPoolEntry};
use crate::error::Result;
use crate::ir::ContentIR;
use crate::manifest::ModLoader;
use serde::{Deserialize, Serialize};

/// High-level compatibility classification for a converted mod.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum CompatibilityStatus {
    Full,
    Partial,
    Skipped,
}

/// Detailed compatibility audit report for a converted mod archive.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CompatibilityReport {
    pub status: CompatibilityStatus,
    pub loader: ModLoader,
    pub features_supported: Vec<String>,
    pub unsupported_features: Vec<String>,
    pub warnings: Vec<String>,
    pub summary: String,
}

impl CompatibilityReport {
    /// Serializes report into compact JSON string.
    pub fn to_json_string(&self) -> Result<String> {
        serde_json::to_string(self).map_err(Into::into)
    }

    /// Serializes report into pretty human-readable JSON string.
    pub fn to_json_pretty(&self) -> Result<String> {
        serde_json::to_string_pretty(self).map_err(Into::into)
    }
}

/// Inspects classfiles, archive entries, and extracted content to produce a `CompatibilityReport`.
pub fn audit_compatibility(
    loader: ModLoader,
    classes: &[ClassFile],
    zip_entry_names: &[String],
    content: &ContentIR,
) -> CompatibilityReport {
    let mixins = detect_mixins(classes, zip_entry_names);
    let asm = detect_asm(classes);
    let ticking_be = detect_ticking_block_entities(classes);

    let mut features_supported = Vec::new();
    let mut unsupported_features = Vec::new();
    let mut warnings = Vec::new();

    if !content.blocks.is_empty() {
        features_supported.push(format!("Blocks ({})", content.blocks.len()));
    }
    if !content.items.is_empty() {
        features_supported.push(format!("Items ({})", content.items.len()));
    }
    if !content.recipes.is_empty() {
        features_supported.push(format!("Recipes ({})", content.recipes.len()));
    }
    if !content.lang.is_empty() {
        features_supported.push(format!("Localization ({} keys)", content.lang.len()));
    }
    if zip_entry_names
        .iter()
        .any(|e| e.starts_with("assets/") && (e.ends_with(".json") || e.ends_with(".png")))
    {
        features_supported.push("Assets (Textures & Models)".to_string());
    }

    if !mixins.is_empty() {
        unsupported_features.push(format!("Mixins ({} indicators)", mixins.len()));
        warnings.push(format!(
            "Mod contains Mixins ({:?}) that cannot be executed in native Rust engine. Statically registered content was extracted.",
            mixins.iter().take(3).cloned().collect::<Vec<_>>()
        ));
    }

    if !asm.is_empty() {
        unsupported_features.push(format!("ASM Bytecode Transformers ({} classes)", asm.len()));
        warnings.push("Mod uses low-level ASM bytecode modification which is unsupported in native runtime.".to_string());
    }

    if !ticking_be.is_empty() {
        warnings.push(format!(
            "Mod defines {} ticking BlockEntity classes; dynamic tick loops are not evaluated in static converter.",
            ticking_be.len()
        ));
    }

    let has_content =
        !content.blocks.is_empty() || !content.items.is_empty() || !content.recipes.is_empty();
    let has_unsupported = !mixins.is_empty() || !asm.is_empty();

    let (status, summary) = if !has_content && has_unsupported {
        (
            CompatibilityStatus::Skipped,
            "Engine patcher or code-only mod with 0 declarative content items/blocks. Skipped by converter.".to_string(),
        )
    } else if has_content && (has_unsupported || !ticking_be.is_empty()) {
        (
            CompatibilityStatus::Partial,
            "Partial conversion: Declarative blocks, items, recipes, and assets extracted. Custom Java bytecode logic, mixins, or ticking block entities cannot be executed natively.".to_string(),
        )
    } else if has_content || !features_supported.is_empty() {
        (
            CompatibilityStatus::Full,
            "100% compatible declarative content mod. All blocks, items, recipes, and assets successfully extracted.".to_string(),
        )
    } else {
        (
            CompatibilityStatus::Skipped,
            "No recognized content items, blocks, recipes, or assets found in archive.".to_string(),
        )
    };

    CompatibilityReport {
        status,
        loader,
        features_supported,
        unsupported_features,
        warnings,
        summary,
    }
}

/// Detects mixin configuration files and mixin bytecode references.
fn detect_mixins(classes: &[ClassFile], zip_entry_names: &[String]) -> Vec<String> {
    let mut indicators = Vec::new();

    // 1. Check entry names for *.mixins.json or mixin configurations
    for entry in zip_entry_names {
        let lower = entry.to_lowercase();
        if lower.ends_with(".mixins.json") || lower.ends_with("mixin.json") {
            indicators.push(entry.clone());
        }
    }

    // 2. Check classes for spongepowered mixin package references
    for class in classes {
        let has_mixin_ref = class.constant_pool.iter().any(|(_, entry)| {
            if let ConstantPoolEntry::Utf8(s) = entry {
                s.contains("org/spongepowered/asm/mixin")
                    || s.contains("org.spongepowered.asm.mixin")
                    || s.contains("Lorg/spongepowered/asm/mixin")
            } else {
                false
            }
        });

        if has_mixin_ref {
            let name = class.class_name();
            indicators.push(format!("class:{}", if name.is_empty() { "unknown" } else { name }));
        }
    }

    indicators
}

/// Detects ASM bytecode transformer libraries.
fn detect_asm(classes: &[ClassFile]) -> Vec<String> {
    let mut indicators = Vec::new();

    for class in classes {
        let has_asm = class.constant_pool.iter().any(|(_, entry)| {
            if let ConstantPoolEntry::Utf8(s) = entry {
                s.contains("org/objectweb/asm")
                    || s.contains("org.objectweb.asm")
                    || s.contains("cpw/mods/fml/relauncher/IFMLLoadingPlugin")
            } else {
                false
            }
        });

        if has_asm {
            let name = class.class_name();
            indicators.push(if name.is_empty() { "unknown".to_string() } else { name.to_string() });
        }
    }

    indicators
}

/// Detects custom BlockEntity classes that implement ticking or reference tickers.
fn detect_ticking_block_entities(classes: &[ClassFile]) -> Vec<String> {
    let mut ticking = Vec::new();

    for class in classes {
        let is_block_entity = class.super_class_name().is_some_and(|s| {
            s.contains("BlockEntity")
                || s.contains("net/minecraft/world/level/block/entity/BlockEntity")
                || s.contains("net/minecraft/block/entity/BlockEntity")
        });

        let has_tick_method = class.methods.iter().any(|m| m.name == "tick");

        let references_ticker = class.constant_pool.iter().any(|(_, entry)| {
            if let ConstantPoolEntry::Utf8(s) = entry {
                s.contains("BlockEntityTicker")
                    || s.contains("tick(Lnet/minecraft/world/level/Level;")
            } else {
                false
            }
        });

        if (is_block_entity && has_tick_method) || references_ticker {
            let name = class.class_name();
            ticking.push(if name.is_empty() { "unknown".to_string() } else { name.to_string() });
        }
    }

    ticking
}
