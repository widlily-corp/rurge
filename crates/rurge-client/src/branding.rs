//! Engine branding and environment configuration for Rurge client.

/// Initializes the engine branding by setting the relevant environment variables.
pub fn init_engine_branding(version: &str) {
    let engine_name = "Rurge Engine";
    let window_title = format!("Rurge Client v{} [Multi-Core 144+ FPS]", version);

    // SAFETY: Setting environment variables at startup before background threads
    // perform concurrent getenv calls.
    unsafe {
        std::env::set_var("RURGE_VERSION", version);
        std::env::set_var("RURGE_ENGINE_NAME", engine_name);
        std::env::set_var("RURGE_WINDOW_TITLE", &window_title);
        std::env::set_var("HORIZON_TITLE", &window_title);
    }
}

/// Returns the window title configured for the client, falling back to HORIZON_TITLE or default.
pub fn get_window_title() -> String {
    std::env::var("RURGE_WINDOW_TITLE")
        .or_else(|_| std::env::var("HORIZON_TITLE"))
        .unwrap_or_else(|_| "Horizon".to_string())
}

/// Returns the engine name, falling back to "Horizon".
pub fn get_engine_name() -> String {
    std::env::var("RURGE_ENGINE_NAME").unwrap_or_else(|_| "Horizon".to_string())
}
