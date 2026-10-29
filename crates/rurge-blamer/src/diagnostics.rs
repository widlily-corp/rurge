use std::collections::{HashMap, VecDeque};
use parking_lot::RwLock;
use serde::Serialize;

/// Maximum number of recent panic records retained in memory to prevent OOM.
pub const MAX_RECENT_PANICS: usize = 256;

/// Information about a single caught panic attributed to a specific mod.
#[derive(Debug, Clone, Serialize)]
pub struct BlameRecord {
    pub mod_id: String,
    pub hook_name: String,
    pub timestamp: u64,
    pub panic_message: String,
    pub backtrace: String,
}

/// Thread-safe registry that collects blame records and tracks mod panic counts.
pub struct Blamer {
    records: RwLock<VecDeque<BlameRecord>>,
    panic_counts: RwLock<HashMap<String, usize>>,
}

impl Blamer {
    /// Creates a new empty `Blamer`.
    pub fn new() -> Self {
        Self {
            records: RwLock::new(VecDeque::with_capacity(MAX_RECENT_PANICS)),
            panic_counts: RwLock::new(HashMap::new()),
        }
    }

    /// Records a mod panic incident with ring-buffer eviction to protect against OOM.
    pub fn record_panic(&self, record: BlameRecord) {
        *self.panic_counts.write().entry(record.mod_id.clone()).or_insert(0) += 1;

        let mut lock = self.records.write();
        if lock.len() >= MAX_RECENT_PANICS {
            lock.pop_front();
        }
        lock.push_back(record);
    }

    /// Returns a copy of recent recorded panic incidents.
    pub fn get_recent_panics(&self) -> Vec<BlameRecord> {
        self.records.read().iter().cloned().collect()
    }

    /// Returns the cumulative number of panics attributed to a given mod ID in O(1) time.
    pub fn panic_count_for_mod(&self, mod_id: &str) -> usize {
        self.panic_counts
            .read()
            .get(mod_id)
            .copied()
            .unwrap_or(0)
    }

    /// Clears all recorded panic incidents and counters.
    pub fn clear(&self) {
        self.records.write().clear();
        self.panic_counts.write().clear();
    }
}

impl Default for Blamer {
    fn default() -> Self {
        Self::new()
    }
}
