use sha1::{Digest, Sha1};

pub fn compute_sha1(data: &[u8]) -> String {
    let mut hasher = Sha1::new();
    hasher.update(data);
    format!("{:x}", hasher.finalize())
}

pub fn verify_sha1(data: &[u8], expected_hex: &str) -> bool {
    compute_sha1(data).eq_ignore_ascii_case(expected_hex)
}
