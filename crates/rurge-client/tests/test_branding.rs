use rurge_client::branding::{get_engine_name, get_window_title, init_engine_branding};

#[test]
fn test_engine_branding_initialization() {
    init_engine_branding("0.1.0");

    assert_eq!(get_engine_name(), "Rurge Engine");
    assert_eq!(
        get_window_title(),
        "Rurge Client v0.1.0 [Multi-Core 144+ FPS]"
    );
    assert_eq!(std::env::var("RURGE_VERSION").unwrap(), "0.1.0");
    assert_eq!(std::env::var("RURGE_ENGINE_NAME").unwrap(), "Rurge Engine");
    assert_eq!(
        std::env::var("RURGE_WINDOW_TITLE").unwrap(),
        "Rurge Client v0.1.0 [Multi-Core 144+ FPS]"
    );
    assert_eq!(
        std::env::var("HORIZON_TITLE").unwrap(),
        "Rurge Client v0.1.0 [Multi-Core 144+ FPS]"
    );
}
