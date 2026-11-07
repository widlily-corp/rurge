use rurge_installer::deployer::deploy_default_configs;
use rurge_installer::mojang_cdn::{
    build_asset_download_jobs, download_file_verified, AssetIndex, VersionManifest, VersionPackage,
    MANIFEST_URL,
};
use rurge_installer::parallel_dl::download_batch_parallel;
use rurge_installer::unpacker::unpack_assets_from_jar_bytes;
use std::env;
use std::sync::Arc;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("========================================================");
    println!("     Rurge Standalone One-Click Minecraft Installer      ");
    println!("========================================================");

    let target_dir = env::current_dir()?;
    let assets_dir = target_dir.join("assets");

    // 1. Check if assets already exist
    if assets_dir.join("minecraft").exists() {
        println!("[1/4] Assets already present in {}", assets_dir.display());
    } else {
        println!("[1/4] Fetching official version manifest from Mojang CDN...");
        let resp = ureq::get(MANIFEST_URL).call()?;
        let manifest: VersionManifest = resp.into_json()?;
        let target_version = &manifest.latest.release;
        println!("      Latest release version: {}", target_version);

        let version_entry = manifest
            .versions
            .iter()
            .find(|v| &v.id == target_version)
            .ok_or_else(|| format!("Target version {} not found in manifest", target_version))?;

        println!("[2/4] Fetching version package metadata...");
        let pkg_resp = ureq::get(&version_entry.url).call()?;
        let pkg: VersionPackage = pkg_resp.into_json()?;

        println!(
            "      Downloading client.jar ({} bytes, sha1: {})...",
            pkg.downloads.client.size,
            &pkg.downloads.client.sha1[..8]
        );
        let jar_bytes = download_file_verified(&pkg.downloads.client.url, &pkg.downloads.client.sha1)?;

        println!("[3/4] Unpacking textures, models, and lang files into {}...", assets_dir.display());
        let count = unpack_assets_from_jar_bytes(&jar_bytes, &assets_dir)?;
        println!("      Extracted {} asset files successfully.", count);

        // 2. Fetch asset index for sounds, music, and font objects and download via Rayon
        println!("[4/4] Fetching asset index and synchronizing sound/font assets in parallel...");
        match download_file_verified(&pkg.asset_index.url, &pkg.asset_index.sha1) {
            Ok(index_bytes) => {
                if let Ok(asset_index) = serde_json::from_slice::<AssetIndex>(&index_bytes) {
                    let jobs = build_asset_download_jobs(&asset_index, &assets_dir);
                    println!("      Found {} asset objects to synchronize in parallel.", jobs.len());
                    let stats = download_batch_parallel(
                        &jobs,
                        3,
                        Some(Arc::new(|done, total| {
                            if done % 200 == 0 || done == total {
                                println!("      Progress: [{done}/{total}] assets verified/downloaded...");
                            }
                        })),
                    );
                    match stats {
                        Ok(s) => println!("      Synchronized {} assets ({:.2} MB) in parallel.", s.completed_files, s.total_bytes as f64 / (1024.0 * 1024.0)),
                        Err(e) => println!("      Asset synchronization warning: {e}"),
                    }
                }
            }
            Err(e) => println!("      Failed to download asset index: {e}"),
        }
    }

    // 3. Deploy default graphics configs and folders
    println!("Deploying default graphics configuration and shaderpacks...");
    deploy_default_configs(&target_dir)?;

    println!("========================================================");
    println!("  Installation Complete! Ready to launch rurge-client. ");
    println!("========================================================");
    Ok(())
}
