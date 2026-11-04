//! Automatic drop-in scanner and converter for Minecraft Java JAR mods.
//!
//! Scans the `mods` folder for `.jar` files and automatically converts them to
//! `.rmod` packages, skipping files whose converted packages are already up-to-date.

use std::path::Path;
use rurge_converter::{convert_jar_to_rmod, ConversionSummary};

/// Scans `mods_dir` for `.jar` files and converts any new or outdated JARs to `.rmod`.
///
/// Returns a list of all successful conversions performed during this invocation.
pub fn convert_dropped_jars_in(mods_dir: &Path) -> Vec<ConversionSummary> {
    let mut summaries = Vec::new();
    if !mods_dir.exists() {
        return summaries;
    }

    let entries = match std::fs::read_dir(mods_dir) {
        Ok(e) => e,
        Err(err) => {
            log::warn!("Failed to read mods directory '{}': {err}", mods_dir.display());
            return summaries;
        }
    };

    for entry in entries {
        let entry = match entry {
            Ok(e) => e,
            Err(_) => continue,
        };
        let jar_path = entry.path();
        let is_jar = jar_path.is_file()
            && jar_path
                .extension()
                .and_then(|s| s.to_str())
                .map(|s| s.eq_ignore_ascii_case("jar"))
                .unwrap_or(false);

        if !is_jar {
            continue;
        }

        let rmod_path = jar_path.with_extension("rmod");

        // Idempotent caching: check if corresponding .rmod exists and is newer than or equal to JAR mtime
        if rmod_path.is_file() {
            let jar_mtime = jar_path.metadata().and_then(|m| m.modified()).ok();
            let rmod_mtime = rmod_path.metadata().and_then(|m| m.modified()).ok();

            if let (Some(j_time), Some(r_time)) = (jar_mtime, rmod_mtime)
                && r_time >= j_time
            {
                log::debug!(
                    "[Rurge] Skipping cached converted mod: {} (up-to-date)",
                    rmod_path.display()
                );
                continue;
            }
        }

        log::info!(
            "[Rurge] Auto-converting dropped JAR mod: {} -> {}",
            jar_path.display(),
            rmod_path.display()
        );

        match convert_jar_to_rmod(&jar_path, Some(&rmod_path)) {
            Ok(summary) => {
                log::info!(
                    "[Rurge] Successfully converted '{}' ({}): {} blocks, {} items, {} recipes",
                    summary.metadata.name,
                    summary.metadata.id,
                    summary.blocks_count,
                    summary.items_count,
                    summary.recipes_count
                );
                summaries.push(summary);
            }
            Err(e) => {
                log::error!(
                    "[Rurge] Failed to convert dropped JAR mod '{}': {e}",
                    jar_path.display()
                );
            }
        }
    }

    summaries
}

/// Convenience function scanning the standard `"mods"` directory in the current working directory.
pub fn convert_dropped_jars() -> Vec<ConversionSummary> {
    convert_dropped_jars_in(Path::new("mods"))
}
