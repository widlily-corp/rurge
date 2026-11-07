use rurge_installer::deployer::deploy_default_configs;
use rurge_installer::mojang_cdn::{download_file_verified, VersionManifest, VersionPackage, MANIFEST_URL};
use rurge_installer::unpacker::unpack_assets_from_jar_bytes;
use std::env;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("========================================================");
    println!("     Rurge Standalone One-Click Minecraft Installer      ");
    println!("========================================================");

    let target_dir = env::current_dir()?;
    let assets_dir = target_dir.join("assets");

    // 1. Check if assets already exist
    if assets_dir.join("minecraft").exists() {
        println!("[1/3] Assets already present in {}", assets_dir.display());
    } else {
        println!("[1/3] Fetching official version manifest from Mojang CDN...");
        let resp = ureq::get(MANIFEST_URL).call()?;
        let manifest: VersionManifest = resp.into_json()?;
        let target_version = &manifest.latest.release;
        println!("      Latest release version: {}", target_version);

        let version_entry = manifest
            .versions
            .iter()
            .find(|v| &v.id == target_version)
            .ok_or_else(|| format!("Target version {} not found in manifest", target_version))?;

        println!("[2/3] Fetching version package metadata...");
        let pkg_resp = ureq::get(&version_entry.url).call()?;
        let pkg: VersionPackage = pkg_resp.into_json()?;

        println!(
            "      Downloading client.jar ({} bytes, sha1: {})...",
            pkg.downloads.client.size,
            &pkg.downloads.client.sha1[..8]
        );
        let jar_bytes = download_file_verified(&pkg.downloads.client.url, &pkg.downloads.client.sha1)?;

        println!("[3/3] Unpacking textures, models, and lang files into {}...", assets_dir.display());
        let count = unpack_assets_from_jar_bytes(&jar_bytes, &assets_dir)?;
        println!("      Extracted {} asset files successfully.", count);
    }

    // 2. Deploy default graphics configs and folders
    println!("Deploying default graphics configuration and shaderpacks...");
    deploy_default_configs(&target_dir)?;

    println!("========================================================");
    println!("  Installation Complete! Ready to launch rurge-client. ");
    println!("========================================================");
    Ok(())
}
