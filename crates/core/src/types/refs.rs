use serde::{Deserialize, Serialize};
use uuid::Uuid;

// =============================================================================
// Reference Types (L3 Pass-by-Reference)
// =============================================================================

/// Reference ID for artifacts stored in L3.
/// Used to implement pass-by-reference for large content.
#[derive(Debug, Clone, Hash, Eq, PartialEq, Serialize, Deserialize)]
pub struct RefId(pub String);

impl RefId {
    /// Create a new randomly generated RefId.
    pub fn new() -> Self {
        Self(Uuid::new_v4().to_string())
    }

    /// Create a RefId from a string.
    pub fn from_string(s: impl Into<String>) -> Self {
        Self(s.into())
    }

    /// Get the inner string value.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Default for RefId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for RefId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}
