use std::path::Path;
use rurge_converter::convert_jar_to_rmod;

fn main() {
    let jar = Path::new("mods/ebwizardry-1.0.2-alpha.jar");
    let rmod = Path::new("mods/ebwizardry-1.0.2-alpha.rmod");
    println!("Converting {} -> {}", jar.display(), rmod.display());
    match convert_jar_to_rmod(jar, Some(rmod)) {
        Ok(summary) => {
            println!(
                "Conversion successful: {} blocks, {} items, {} recipes",
                summary.blocks_count, summary.items_count, summary.recipes_count
            );
            let hmod = Path::new("mods/ebwizardry-1.0.2-alpha.hmod");
            let _ = std::fs::copy(rmod, hmod);
            if Path::new("horizon-mods").exists() {
                let _ = std::fs::copy(rmod, "horizon-mods/ebwizardry-1.0.2-alpha.hmod");
                let _ = std::fs::copy(rmod, "horizon-mods/ebwizardry-1.0.2-alpha.rmod");
            }
            println!("Synchronized .hmod and .rmod in mods/ and horizon-mods/");
        }
        Err(e) => eprintln!("Error converting: {e}"),
    }
}
