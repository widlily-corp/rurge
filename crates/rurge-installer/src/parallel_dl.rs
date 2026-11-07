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
    pub failed_files: usize,
}

pub fn download_batch_parallel(
    jobs: &[DownloadJob],
    max_retries: u32,
    on_progress: Option<Arc<dyn Fn(usize, usize) + Send + Sync>>,
) -> Result<DownloadStats, String> {
    let completed = AtomicUsize::new(0);
    let failed = AtomicUsize::new(0);
    let total = jobs.len();
    let total_bytes: u64 = jobs.iter().map(|j| j.size).sum();

    // Shared agent with connection pooling and keep-alive reuse to prevent TLS connection limits
    let agent = ureq::builder()
        .timeout_connect(Duration::from_secs(10))
        .timeout_read(Duration::from_secs(30))
        .max_idle_connections(256)
        .max_idle_connections_per_host(64)
        .build();

    jobs.par_iter().for_each(|job| {
        // Fast-path: if file already exists with identical size, check SHA-1
        if job.target_path.exists()
            && let Ok(meta) = fs::metadata(&job.target_path)
            && meta.len() == job.size
            && let Ok(existing_bytes) = fs::read(&job.target_path)
            && verify_sha1(&existing_bytes, &job.expected_sha1)
        {
            let done = completed.fetch_add(1, Ordering::Relaxed) + 1;
            if let Some(ref cb) = on_progress {
                cb(done, total);
            }
            return;
        }

        // Retry loop with exponential backoff & keep-alive connection reuse
        let mut attempts = 0;
        let mut success = false;
        while attempts < max_retries {
            attempts += 1;
            match agent.get(&job.url).call() {
                Ok(response) => {
                    let mut bytes = Vec::with_capacity(job.size as usize);
                    if response.into_reader().read_to_end(&mut bytes).is_err() {
                        std::thread::sleep(Duration::from_millis(150 * attempts as u64));
                        continue;
                    }

                    if !verify_sha1(&bytes, &job.expected_sha1) {
                        std::thread::sleep(Duration::from_millis(150 * attempts as u64));
                        continue;
                    }

                    if let Some(parent) = job.target_path.parent() {
                        let _ = fs::create_dir_all(parent);
                    }

                    if fs::write(&job.target_path, &bytes).is_err() {
                        std::thread::sleep(Duration::from_millis(150 * attempts as u64));
                        continue;
                    }

                    let done = completed.fetch_add(1, Ordering::Relaxed) + 1;
                    if let Some(ref cb) = on_progress {
                        cb(done, total);
                    }
                    success = true;
                    break;
                }
                Err(_) => {
                    // Exponential backoff
                    std::thread::sleep(Duration::from_millis(200 * attempts as u64));
                }
            }
        }

        if !success {
            failed.fetch_add(1, Ordering::Relaxed);
        }
    });

    let failed_count = failed.load(Ordering::Relaxed);
    if failed_count > 0 {
        return Err(format!("{failed_count} files failed to download after {max_retries} retries"));
    }

    Ok(DownloadStats {
        completed_files: completed.load(Ordering::Relaxed),
        total_files: total,
        total_bytes,
        failed_files: 0,
    })
}
