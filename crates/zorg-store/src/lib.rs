//! Storage and indexing boundary for Zorg.

use zorg_core::{ZorgError, ZorgResult};

/// Store handle placeholder.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct Store {
    root: String,
}

impl Store {
    /// Opens a store rooted at the supplied corpus path.
    pub fn open(root: impl Into<String>) -> ZorgResult<Self> {
        let root = root.into();
        if root.is_empty() {
            return Err(ZorgError::Unsupported("store root must not be empty"));
        }

        Err(ZorgError::Unsupported(
            "zorg-store is a foundation stub; indexing implementation is pending",
        ))
    }

    /// Returns the configured root path text.
    #[must_use]
    pub fn root(&self) -> &str {
        &self.root
    }
}
