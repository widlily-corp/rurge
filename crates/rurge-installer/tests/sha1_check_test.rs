use rurge_installer::sha1_check::{compute_sha1, verify_sha1};

#[test]
fn test_sha1_verification() {
    let data = b"Minecraft Rust Client";
    let hash = compute_sha1(data);
    assert!(verify_sha1(data, &hash));
    assert!(!verify_sha1(data, "0000000000000000000000000000000000000000"));
}
