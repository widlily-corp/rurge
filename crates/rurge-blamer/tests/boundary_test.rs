use rurge_blamer::{generate_crash_report, safe_invoke, write_crash_report_to_file, Blamer};

#[test]
fn test_safe_invoke_success() {
    let blamer = Blamer::new();
    let result = safe_invoke("my_mod", "on_init", &blamer, || 42);

    assert_eq!(result.unwrap(), 42);
    assert_eq!(blamer.panic_count_for_mod("my_mod"), 0);
    assert!(blamer.get_recent_panics().is_empty());
}

#[test]
fn test_safe_invoke_panic_catch_str() {
    let blamer = Blamer::new();
    let result = safe_invoke("faulty_mod", "on_tick", &blamer, || {
        panic!("something went wrong");
    });

    assert!(result.is_err());
    let err = result.unwrap_err();
    assert_eq!(err.mod_id, "faulty_mod");
    assert_eq!(err.hook, "on_tick");
    assert_eq!(err.message, "something went wrong");

    assert_eq!(blamer.panic_count_for_mod("faulty_mod"), 1);
    let panics = blamer.get_recent_panics();
    assert_eq!(panics.len(), 1);
    assert_eq!(panics[0].mod_id, "faulty_mod");
    assert_eq!(panics[0].hook_name, "on_tick");
    assert_eq!(panics[0].panic_message, "something went wrong");
    assert!(!panics[0].backtrace.is_empty());
    assert!(panics[0].timestamp > 0);
}

#[test]
fn test_safe_invoke_panic_catch_string() {
    let blamer = Blamer::new();
    let result = safe_invoke("string_panicker", "on_render", &blamer, || {
        panic!("dynamic error: {}", 404);
    });

    let err = result.unwrap_err();
    assert_eq!(err.message, "dynamic error: 404");
    assert_eq!(blamer.panic_count_for_mod("string_panicker"), 1);
}

#[test]
fn test_safe_invoke_panic_catch_non_str() {
    let blamer = Blamer::new();
    let result = safe_invoke("weird_mod", "on_event", &blamer, || {
        std::panic::panic_any(999);
    });

    let err = result.unwrap_err();
    assert!(err.message.contains("Box<dyn Any>"));
    assert_eq!(blamer.panic_count_for_mod("weird_mod"), 1);
}

#[test]
fn test_panic_counting_and_clear() {
    let blamer = Blamer::new();

    let _ = safe_invoke("mod_a", "hook1", &blamer, || panic!("fail 1"));
    let _ = safe_invoke("mod_a", "hook2", &blamer, || panic!("fail 2"));
    let _ = safe_invoke("mod_b", "hook1", &blamer, || panic!("fail 3"));

    assert_eq!(blamer.panic_count_for_mod("mod_a"), 2);
    assert_eq!(blamer.panic_count_for_mod("mod_b"), 1);
    assert_eq!(blamer.panic_count_for_mod("mod_c"), 0);
    assert_eq!(blamer.get_recent_panics().len(), 3);

    blamer.clear();
    assert_eq!(blamer.panic_count_for_mod("mod_a"), 0);
    assert_eq!(blamer.get_recent_panics().len(), 0);
}

#[test]
fn test_crash_report_generation() {
    let blamer = Blamer::new();
    let _ = safe_invoke("crash_mod", "on_load", &blamer, || {
        panic!("fatal crash during asset loading");
    });

    let report = generate_crash_report(&blamer, Some("Windows 11 / x86_64"));
    assert!(report.contains("RURGE CRASH & BLAME REPORT"));
    assert!(report.contains("Windows 11 / x86_64"));
    assert!(report.contains("crash_mod"));
    assert!(report.contains("on_load"));
    assert!(report.contains("fatal crash during asset loading"));
    assert!(report.contains("Stack Trace:"));
}

#[test]
fn test_write_crash_report_to_file() {
    let blamer = Blamer::new();
    let _ = safe_invoke("file_crash_mod", "on_init", &blamer, || {
        panic!("io failure");
    });

    let temp_dir = std::env::temp_dir().join("rurge_blamer_test");
    let report_path = temp_dir.join("crash_report.txt");

    let result = write_crash_report_to_file(&blamer, &report_path, Some("Test OS"));
    assert!(result.is_ok());

    let content = std::fs::read_to_string(&report_path).expect("Failed to read report file");
    assert!(content.contains("file_crash_mod"));
    assert!(content.contains("io failure"));

    let _ = std::fs::remove_file(&report_path);
    let _ = std::fs::remove_dir(&temp_dir);
}


#[test]
fn test_panic_throttling_and_max_capacity() {
    let blamer = Blamer::new();

    // Trigger 300 panics for spam_mod
    for i in 0..300 {
        let _ = safe_invoke("spam_mod", "tick", &blamer, || {
            panic!("spam error {i}");
        });
    }

    assert_eq!(blamer.panic_count_for_mod("spam_mod"), 300);

    let recent = blamer.get_recent_panics();
    // Verify ring buffer caps at 256 records
    assert_eq!(recent.len(), 256);

    // Verify backtrace was throttled after threshold
    let last_record = recent.last().unwrap();
    assert!(last_record.backtrace.contains("Backtrace throttled"));
}
