use abi_stable::{
    std_types::{RString, RVec},
    StableAbi,
};
use serde::{Deserialize, Serialize};

/// Metadata describing a Rurge mod plugin.
///
/// This structure is ABI-stable and safe to pass across dynamic library boundaries.
#[repr(C)]
#[derive(StableAbi, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModMetadata {
    /// Unique identifier for the mod (e.g. "industrial_rust").
    pub id: RString,
    /// Human-readable display name of the mod.
    pub name: RString,
    /// Semantic version of the mod (e.g. "1.0.0").
    pub version: RString,
    /// Short description of what the mod provides.
    pub description: RString,
    /// List of mod authors / maintainers.
    pub authors: RVec<RString>,
}

impl ModMetadata {
    /// Creates a new `ModMetadata` instance with the specified attributes.
    pub fn new(
        id: impl Into<RString>,
        name: impl Into<RString>,
        version: impl Into<RString>,
        description: impl Into<RString>,
        authors: impl IntoIterator<Item = impl Into<RString>>,
    ) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            version: version.into(),
            description: description.into(),
            authors: authors.into_iter().map(Into::into).collect(),
        }
    }
}
