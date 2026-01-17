use serde::{Deserialize, Serialize};
use super::refs::RefId;

// =============================================================================
// Agent Result Types (L1 Output)
// =============================================================================

/// Result from the agent after completing a task.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "payload")]
pub enum AgentResult {
    /// Text response.
    Text(String),

    /// File artifact (code, document, etc.).
    File {
        /// Reference to the file in L3.
        ref_id: RefId,
        /// File name.
        filename: String,
        /// MIME type.
        mime_type: String,
    },

    /// Structured data response.
    Data(serde_json::Value),

    /// Interactive UI component (React/JSON).
    UiComponent {
        /// Component type.
        component_type: String,
        /// Component props/configuration.
        props: serde_json::Value,
    },

    /// Error result.
    Error {
        /// Error message.
        message: String,
        /// Error code.
        code: String,
    },
}
