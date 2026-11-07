use crate::sha1_check::verify_sha1;
use serde::Deserialize;
use std::io::Read;

pub const MANIFEST_URL: &str = "https://piston-meta.mojang.com/mc/game/version_manifest_v2.json";
pub const RESOURCES_URL: &str = "https://resources.download.minecraft.net";

#[derive(Debug, Deserialize)]
pub struct VersionManifest {
    pub latest: LatestVersions,
    pub versions: Vec<VersionEntry>,
}

#[derive(Debug, Deserialize)]
pub struct LatestVersions {
    pub release: String,
    pub snapshot: String,
}

#[derive(Debug, Deserialize)]
pub struct VersionEntry {
    pub id: String,
    #[serde(rename = "type")]
    pub version_type: String,
    pub url: String,
    pub time: String,
    #[serde(rename = "releaseTime")]
    pub release_time: String,
    pub sha1: String,
}

#[derive(Debug, Deserialize)]
pub struct VersionPackage {
    pub id: String,
    #[serde(rename = "assetIndex")]
    pub asset_index: AssetIndexRef,
    pub downloads: Downloads,
}

#[derive(Debug, Deserialize)]
pub struct AssetIndexRef {
    pub id: String,
    pub sha1: String,
    pub size: u64,
    pub url: String,
    #[serde(rename = "totalSize")]
    pub total_size: u64,
}

#[derive(Debug, Deserialize)]
pub struct Downloads {
    pub client: DownloadEntry,
    pub server: Option<DownloadEntry>,
}

#[derive(Debug, Deserialize)]
pub struct DownloadEntry {
    pub sha1: String,
    pub size: u64,
    pub url: String,
}

#[derive(Debug, Deserialize)]
pub struct AssetIndex {
    pub objects: std::collections::HashMap<String, AssetObject>,
}

#[derive(Debug, Deserialize)]
pub struct AssetObject {
    pub hash: String,
    pub size: u64,
}

/// Builds parallel download jobs for all assets indexed in `AssetIndex`.
pub fn build_asset_download_jobs(
    index: &AssetIndex,
    assets_dir: &std::path::Path,
) -> Vec<crate::parallel_dl::DownloadJob> {
    let objects_dir = assets_dir.join("objects");
    let mut jobs = Vec::with_capacity(index.objects.len());
    for obj in index.objects.values() {
        if obj.hash.len() >= 2 {
            let prefix = &obj.hash[..2];
            let target_path = objects_dir.join(prefix).join(&obj.hash);
            let url = format!("{RESOURCES_URL}/{prefix}/{}", obj.hash);
            jobs.push(crate::parallel_dl::DownloadJob {
                url,
                target_path,
                expected_sha1: obj.hash.clone(),
                size: obj.size,
            });
        }
    }
    jobs
}

pub fn download_file_verified(url: &str, expected_sha1: &str) -> Result<Vec<u8>, String> {
    let resp = ureq::get(url)
        .call()
        .map_err(|e| format!("Failed to download {url}: {e}"))?;

    let mut bytes = Vec::new();
    resp.into_reader()
        .read_to_end(&mut bytes)
        .map_err(|e| format!("Failed to read response body: {e}"))?;

    if !verify_sha1(&bytes, expected_sha1) {
        return Err(format!(
            "SHA-1 mismatch for {url}: expected {expected_sha1}"
        ));
    }

    Ok(bytes)
}
