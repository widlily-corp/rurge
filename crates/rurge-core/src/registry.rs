use std::{
    collections::HashMap,
    fmt,
    str::FromStr,
};
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// A namespaced resource identifier (e.g. `"minecraft:diamond"` or `"mod_id:custom_item"`).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Identifier {
    pub namespace: String,
    pub path: String,
}

impl Identifier {
    /// Creates a new `Identifier` from namespace and path components.
    pub fn new(namespace: impl Into<String>, path: impl Into<String>) -> Self {
        Self {
            namespace: namespace.into(),
            path: path.into(),
        }
    }

    /// Parses an identifier from a `"namespace:path"` string slice.
    pub fn parse(s: &str) -> Result<Self, ParseIdentifierError> {
        s.parse()
    }
}

impl fmt::Display for Identifier {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}", self.namespace, self.path)
    }
}

/// Errors occurring when parsing an identifier from a string.
#[derive(Debug, Error, PartialEq, Eq, Clone)]
pub enum ParseIdentifierError {
    #[error("missing separator ':' in identifier string: '{0}'")]
    MissingSeparator(String),
    #[error("empty namespace in identifier string: '{0}'")]
    EmptyNamespace(String),
    #[error("empty path in identifier string: '{0}'")]
    EmptyPath(String),
}

impl FromStr for Identifier {
    type Err = ParseIdentifierError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let (ns, path) = s
            .split_once(':')
            .ok_or_else(|| ParseIdentifierError::MissingSeparator(s.to_string()))?;

        if ns.is_empty() {
            return Err(ParseIdentifierError::EmptyNamespace(s.to_string()));
        }

        if path.is_empty() {
            return Err(ParseIdentifierError::EmptyPath(s.to_string()));
        }

        Ok(Self {
            namespace: ns.to_string(),
            path: path.to_string(),
        })
    }
}

/// Errors produced during registry operations.
#[derive(Debug, Error, PartialEq, Eq, Clone)]
pub enum RegistryError {
    #[error("duplicate identifier: {0}")]
    DuplicateIdentifier(Identifier),
    #[error("raw ID overflow: registry reached maximum capacity")]
    IdOverflow,
}

/// A content registry mapping between string `Identifier`s, dense sequential `u32` raw IDs, and content values `T`.
pub struct Registry<T> {
    name: String,
    entries: Vec<T>,
    id_to_raw: HashMap<Identifier, u32>,
    raw_to_id: Vec<Identifier>,
}

impl<T> Registry<T> {
    /// Creates a new, empty registry with the specified descriptive name.
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            entries: Vec::new(),
            id_to_raw: HashMap::new(),
            raw_to_id: Vec::new(),
        }
    }

    /// Returns the descriptive name of this registry.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Registers a new entry with an identifier, assigning a dense sequential `u32` raw ID.
    ///
    /// Returns `Err(RegistryError::DuplicateIdentifier)` if the identifier is already registered.
    pub fn register(&mut self, id: Identifier, entry: T) -> Result<u32, RegistryError> {
        if self.id_to_raw.contains_key(&id) {
            return Err(RegistryError::DuplicateIdentifier(id));
        }

        let raw = u32::try_from(self.entries.len()).map_err(|_| RegistryError::IdOverflow)?;

        self.id_to_raw.insert(id.clone(), raw);
        self.raw_to_id.push(id);
        self.entries.push(entry);

        Ok(raw)
    }

    /// Retrieves a reference to an entry by its `Identifier`.
    pub fn get_by_id(&self, id: &Identifier) -> Option<&T> {
        let &raw = self.id_to_raw.get(id)?;
        self.entries.get(raw as usize)
    }

    /// Retrieves a mutable reference to an entry by its `Identifier`.
    pub fn get_by_id_mut(&mut self, id: &Identifier) -> Option<&mut T> {
        let &raw = self.id_to_raw.get(id)?;
        self.entries.get_mut(raw as usize)
    }

    /// Retrieves a reference to an entry by its dense `u32` raw ID.
    pub fn get_by_raw_id(&self, raw: u32) -> Option<&T> {
        self.entries.get(raw as usize)
    }

    /// Retrieves a mutable reference to an entry by its dense `u32` raw ID.
    pub fn get_by_raw_id_mut(&mut self, raw: u32) -> Option<&mut T> {
        self.entries.get_mut(raw as usize)
    }

    /// Retrieves the `Identifier` associated with a dense `u32` raw ID.
    pub fn get_id_by_raw_id(&self, raw: u32) -> Option<&Identifier> {
        self.raw_to_id.get(raw as usize)
    }

    /// Retrieves the raw `u32` ID for an `Identifier`, if registered.
    pub fn get_raw_id(&self, id: &Identifier) -> Option<u32> {
        self.id_to_raw.get(id).copied()
    }

    /// Checks if an identifier is registered.
    pub fn contains(&self, id: &Identifier) -> bool {
        self.id_to_raw.contains_key(id)
    }

    /// Returns the number of registered entries.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Checks whether the registry is empty.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Iterates over all registered `(Identifier, &T)` pairs.
    pub fn iter(&self) -> impl Iterator<Item = (&Identifier, &T)> {
        self.raw_to_id.iter().zip(self.entries.iter())
    }
}

impl<T: fmt::Debug> fmt::Debug for Registry<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Registry")
            .field("name", &self.name)
            .field("len", &self.entries.len())
            .finish()
    }
}
