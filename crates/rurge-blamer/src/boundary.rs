use crate::diagnostics::{BlameRecord, Blamer};

/// Error returned when a mod hook panics inside `safe_invoke`.
#[derive(Debug, thiserror::Error, Clone)]
#[error("Mod '{mod_id}' panicked in hook '{hook}': {message}")]
pub struct PanicError {
    pub mod_id: String,
    pub hook: &'static str,
    pub message: String,
}

/// Threshold of panics per mod before expensive stack unwinding backtrace is throttled to save FPS.
pub const BACKTRACE_THROTTLE_THRESHOLD: usize = 5;

/// Safely executes a closure representing a mod hook within a panic boundary (`catch_unwind`).
///
/// If a panic occurs, the payload and backtrace are captured, attributed to `mod_id`,
/// recorded in `blamer`, and returned as `Err(PanicError)`.
pub fn safe_invoke<F, R>(
    mod_id: &str,
    hook: &'static str,
    blamer: &Blamer,
    f: F,
) -> Result<R, PanicError>
where
    F: FnOnce() -> R + std::panic::UnwindSafe,
{
    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(f)) {
        Ok(res) => Ok(res),
        Err(err) => {
            let message = if let Some(s) = err.downcast_ref::<&str>() {
                (*s).to_string()
            } else if let Some(s) = err.downcast_ref::<String>() {
                s.clone()
            } else {
                "Box<dyn Any>".to_string()
            };

            let current_count = blamer.panic_count_for_mod(mod_id);
            let backtrace_str = if current_count < BACKTRACE_THROTTLE_THRESHOLD {
                let bt = backtrace::Backtrace::new();
                format!("{bt:?}")
            } else {
                format!("<Backtrace throttled: mod '{mod_id}' exceeded {BACKTRACE_THROTTLE_THRESHOLD} panic incidents; suppressing backtrace capture to preserve framerate>")
            };

            let timestamp = chrono::Utc::now().timestamp().max(0) as u64;

            let record = BlameRecord {
                mod_id: mod_id.to_string(),
                hook_name: hook.to_string(),
                timestamp,
                panic_message: message.clone(),
                backtrace: backtrace_str,
            };

            blamer.record_panic(record);

            Err(PanicError {
                mod_id: mod_id.to_string(),
                hook,
                message,
            })
        }
    }
}
