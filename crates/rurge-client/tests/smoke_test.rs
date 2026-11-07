#[test]
fn test_rurge_client_metadata() {
    assert_eq!(rurge_client::version(), env!("CARGO_PKG_VERSION"));
}
