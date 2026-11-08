use cargo_rurge::commands::run::{resolve_client_package, RunArgs};

#[test]
fn test_resolve_client_package_default() {
    let args = RunArgs::default();
    assert_eq!(resolve_client_package(&args), "rurge-client");
}

#[test]
fn test_resolve_client_package_release() {
    let args = RunArgs {
        release: true,
        ..Default::default()
    };
    assert_eq!(resolve_client_package(&args), "rurge-client");
}
