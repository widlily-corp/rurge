use parking_lot::RwLock;
use serde::Serialize;

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
    records: RwLock<Vec<BlameRecord>>,
}

impl Blamer {
    /// Creates a new empty `Blamer`.
    pub fn new() -> Self {
        Self {
            records: RwLock::new(Vec::new()),
        }
    }

    /// Records a mod panic incident.
    pub fn record_panic(&self, record: BlameRecord) {
        self.records.write().push(record);
    }

    /// Returns a copy of all recorded panic incidents.
    pub fn get_recent_panics(&self) -> Vec<BlameRecord> {
        self.records.read().clone()
    }

    /// Returns the number of panics attributed to a given mod ID.
    pub fn panic_count_for_mod(&self, mod_id: &str) -> usize {
        self.records
            .read()
            .iter()
            .filter(|r| r.mod_id == mod_id)
            .count()
    }

    /// Clears all recorded panic incidents.
    pub fn clear(&self) {
        self.records.write().clear();
    }
}

impl Default for Blamer {
    fn default() -> Self {
        Self::new()
    }
}
