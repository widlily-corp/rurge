use std::collections::BTreeMap;
use serde::{de::DeserializeOwned, Deserialize, Serialize};

/// Errors that can occur during mod data persistence or NBT conversion.
#[derive(Debug, thiserror::Error)]
pub enum PersistenceError {
    /// Serialization or deserialization error.
    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    /// File system or I/O failure.
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    /// Corrupted or invalid NBT compound format.
    #[error("Invalid data format: {0}")]
    InvalidFormat(String),
}

/// NBT compound structure representation for mod and vanilla save isolation.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct NbtCompound {
    /// Stored tag values keyed by tag name.
    pub values: BTreeMap<String, serde_json::Value>,
}

impl NbtCompound {
    /// Creates a new empty `NbtCompound`.
    pub fn new() -> Self {
        Self::default()
    }

    /// Inserts a tag value.
    pub fn insert(&mut self, key: impl Into<String>, value: serde_json::Value) {
        self.values.insert(key.into(), value);
    }

    /// Gets a tag value by key name.
    pub fn get(&self, key: &str) -> Option<&serde_json::Value> {
        self.values.get(key)
    }
}

/// Isolates custom mod state inside the `"rurge:mods"` namespace within world/chunk NBT.
///
/// Prevents save corruption when loading chunks into vanilla Anvil saves.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct WorldDataAttachment {
    /// Maps mod_id -> (key -> JSON Value)
    data: BTreeMap<String, BTreeMap<String, serde_json::Value>>,
}

impl WorldDataAttachment {
    /// The NBT tag key reserved exclusively for Rurge mod persistence.
    pub const NAMESPACE: &'static str = "rurge:mods";

    /// Creates a new empty `WorldDataAttachment`.
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets a serializable value for a given mod and key.
    pub fn set<T: Serialize>(
        &mut self,
        mod_id: &str,
        key: &str,
        value: &T,
    ) -> Result<(), PersistenceError> {
        let val = serde_json::to_value(value)?;
        self.data
            .entry(mod_id.to_string())
            .or_default()
            .insert(key.to_string(), val);
        Ok(())
    }

    /// Retrieves and deserializes a value for a given mod and key.
    pub fn get<T: DeserializeOwned>(
        &self,
        mod_id: &str,
        key: &str,
    ) -> Result<Option<T>, PersistenceError> {
        if let Some(mod_data) = self.data.get(mod_id) {
            if let Some(val) = mod_data.get(key) {
                let deserialized = serde_json::from_value(val.clone())?;
                return Ok(Some(deserialized));
            }
        }
        Ok(None)
    }

    /// Serializes all mod data under the root compound's `"rurge:mods"` key.
    ///
    /// Preserves all existing vanilla keys without modification.
    pub fn save_to_nbt(&self, root: &mut NbtCompound) -> Result<(), PersistenceError> {
        let val = serde_json::to_value(&self.data)?;
        root.values.insert(Self::NAMESPACE.to_string(), val);
        Ok(())
    }

    /// Deserializes mod data from the root compound's `"rurge:mods"` key.
    ///
    /// Silently ignores vanilla keys and unknown data.
    pub fn load_from_nbt(&mut self, root: &NbtCompound) -> Result<(), PersistenceError> {
        if let Some(val) = root.values.get(Self::NAMESPACE) {
            let map: BTreeMap<String, BTreeMap<String, serde_json::Value>> =
                serde_json::from_value(val.clone())?;
            self.data = map;
        }
        Ok(())
    }

    /// Convenience alias for `save_to_nbt`.
    pub fn save_mod_nbt(&self, chunk_tag: &mut NbtCompound) -> Result<(), PersistenceError> {
        self.save_to_nbt(chunk_tag)
    }

    /// Convenience alias for `load_from_nbt`.
    pub fn load_mod_nbt(&mut self, chunk_tag: &NbtCompound) -> Result<(), PersistenceError> {
        self.load_from_nbt(chunk_tag)
    }
}
