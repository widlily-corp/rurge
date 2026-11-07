use crate::sha1_check::verify_sha1;
use rayon::prelude::*;
use std::fs;
use std::io::Read;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

#[derive(Debug, Clone)]
pub struct DownloadJob {
    pub url: String,
    pub target_path: PathBuf,
    pub expected_sha1: String,
    pub size: u64,
}

pub struct DownloadStats {
    pub completed_files: usize,
    pub total_files: usize,
    pub total_bytes: u64,
}

pub fn download_batch_parallel(
    jobs: &[DownloadJob],
    max_retries: u32,
    on_progress: Option<Arc<dyn Fn(usize, usize) + Send + Sync>>,
) -> Result<DownloadStats, String> {
    let completed = AtomicUsize::new(0);
    let total = jobs.len();
    let total_bytes: u64 = jobs.iter().map(|j| j.size).sum();

    jobs.par_iter().try_for_each(|job| -> Result<(), String> {
        // If file already exists and has matching hash, skip
        if job.target_path.exists() {
            if let Ok(existing_bytes) = fs::read(&job.target_path) {
                if verify_sha1(&existing_bytes, &job.expected_sha1) {
                    let done = completed.fetch_add(1, Ordering::Relaxed) + 1;
                    if let Some(ref cb) = on_progress {
                        cb(done, total);
                    }
                    return Ok(());
                }
            }
        }

        // Retry loop
        let mut attempts = 0;
        let mut last_err = String::new();
        while attempts < max_retries {
            attempts += 1;
            match ureq::get(&job.url).timeout(Duration::from_secs(30)).call() {
                Ok(response) => {
                    let mut bytes = Vec::with_capacity(job.size as usize);
                    if let Err(e) = response.into_reader().read_to_end(&mut bytes) {
                        last_err = format!("Read error: {e}");
                        continue;
                    }

                    if !verify_sha1(&bytes, &job.expected_sha1) {
                        last_err = format!("SHA1 mismatch for {}", job.url);
                        continue;
                    }

                    if let Some(parent) = job.target_path.parent() {
                        let _ = fs::create_dir_all(parent);
                    }

                    if let Err(e) = fs::write(&job.target_path, &bytes) {
                        last_err = format!("Write error: {e}");
                        continue;
                    }

                    let done = completed.fetch_add(1, Ordering::Relaxed) + 1;
                    if let Some(ref cb) = on_progress {
                        cb(done, total);
                    }
                    return Ok(());
                }
                Err(e) => {
                    last_err = format!("HTTP error: {e}");
                    std::thread::sleep(Duration::from_millis(150 * attempts as u64));
                }
            }
        }

        Err(format!(
            "Failed to download {} after {} attempts: {}",
            job.url, max_retries, last_err
        ))
    })?;

    Ok(DownloadStats {
        completed_files: completed.load(Ordering::Relaxed),
        total_files: total,
        total_bytes,
    })
}
