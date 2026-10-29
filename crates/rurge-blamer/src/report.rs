use std::collections::BTreeMap;
use std::fmt::Write;
use std::path::Path;

use crate::diagnostics::Blamer;

/// Generates a human-readable crash report summarizing all panics recorded by `blamer`.
pub fn generate_crash_report(blamer: &Blamer, system_info: Option<&str>) -> String {
    let panics = blamer.get_recent_panics();
    let mut report = String::new();

    let _ = writeln!(report, "================================================================================");
    let _ = writeln!(report, "                         RURGE CRASH & BLAME REPORT                             ");
    let _ = writeln!(report, "================================================================================");

    if let Some(sys) = system_info {
        let _ = writeln!(report, "System Info: {}", sys);
    }

    let now = chrono::Utc::now();
    let _ = writeln!(report, "Generated At: {}", now.to_rfc3339());
    let _ = writeln!(report, "Total Blamed Panics: {}", panics.len());
    let _ = writeln!(report, "--------------------------------------------------------------------------------");

    let mut mod_counts: BTreeMap<&str, usize> = BTreeMap::new();
    for p in &panics {
        *mod_counts.entry(&p.mod_id).or_insert(0) += 1;
    }

    let _ = writeln!(report, "Blamed Mods Summary:");
    if mod_counts.is_empty() {
        let _ = writeln!(report, "  No mod panics recorded.");
    } else {
        for (mod_id, count) in &mod_counts {
            let _ = writeln!(report, "  - Mod '{}': {} panic(s)", mod_id, count);
        }
    }
    let _ = writeln!(report, "================================================================================");
    let _ = writeln!(report, "CRASH INCIDENTS:");
    let _ = writeln!(report, "================================================================================");

    for (idx, record) in panics.iter().enumerate() {
        let time_str = chrono::DateTime::from_timestamp(record.timestamp as i64, 0)
            .map(|dt| dt.to_rfc3339())
            .unwrap_or_else(|| format!("timestamp: {}", record.timestamp));

        let _ = writeln!(report, "\n--- [Incident #{}] -----------------------------------------------------------", idx + 1);
        let _ = writeln!(report, "Blamed Mod ID : {}", record.mod_id);
        let _ = writeln!(report, "Hook Name     : {}", record.hook_name);
        let _ = writeln!(report, "Timestamp     : {} (unix: {})", time_str, record.timestamp);
        let _ = writeln!(report, "Panic Reason  : {}", record.panic_message);
        let _ = writeln!(report, "\nStack Trace:\n{}", record.backtrace);
    }

    report
}

/// Writes the generated crash report to the specified file path.
pub fn write_crash_report_to_file(
    blamer: &Blamer,
    path: &Path,
    system_info: Option<&str>,
) -> std::io::Result<()> {
    let content = generate_crash_report(blamer, system_info);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, content)
}
