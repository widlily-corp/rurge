use std::path::Path;
use horizon_mods::{Package, Content};

fn main() {
    let p = Path::new("horizon-mods/ebwizardry-1.0.2-alpha.hmod");
    let pkg = Package::read(p).expect("package read");
    println!("Package read ok. Reading content...");
    match Content::read(p, &pkg.manifest) {
        Ok(c) => {
            println!("Content::read SUCCESS!");
            println!("  blocks: {}", c.blocks.len());
            println!("  items: {}", c.items.len());
            println!("  creative_tabs: {}", c.creative_tabs.len());
            for t in &c.creative_tabs {
                println!("    tab id: {}, title: {}, items: {}", t.id, t.title, t.items.len());
            }
        }
        Err(e) => {
            eprintln!("Content::read ERROR: {e}");
        }
    }
}
