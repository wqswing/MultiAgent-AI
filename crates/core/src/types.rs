//! Core type definitions for MutilAgent.
//!
//! This module contains all the fundamental data structures used across
//! the multi-agent system.

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

// =============================================================================
// Request Types
// =============================================================================

/// Content type for multi-modal input.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "data")]
pub enum RequestContent {
    /// Plain text content.
    Text(String),

    /// Audio content (will be Whisper transcribed).
    Audio {
        /// Reference to audio file in L3.
        ref_id: RefId,
        /// Optional transcription if already processed.
        transcription: Option<String>,
    },

    /// Image content (will be Vision parsed).
    Image {
        /// Reference to image file in L3.
        ref_id: RefId,
        /// Optional description if already processed.
        description: Option<String>,
    },

    /// System event (webhook payload).
    SystemEvent {
        /// Event type identifier.
        event_type: String,
        /// Event payload.
        payload: serde_json::Value,
    },
}

/// Normalized input request after multi-modal processing.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NormalizedRequest {
    /// Unique trace ID for this request.
    pub trace_id: String,

    /// Normalized content (always text after processing).
    pub content: String,

    /// Original content type.
    pub original_content: RequestContent,

    /// References to artifacts in L3.
    pub refs: Vec<RefId>,

    /// Request metadata.
    pub metadata: RequestMetadata,
}

/// Metadata associated with a request.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RequestMetadata {
    /// User identifier.
    pub user_id: Option<String>,

    /// Session identifier for stateful conversations.
    pub session_id: Option<String>,

    /// Custom key-value metadata.
    pub custom: std::collections::HashMap<String, String>,
}

impl NormalizedRequest {
    /// Create a new NormalizedRequest with text content.
    pub fn text(content: impl Into<String>) -> Self {
        let content = content.into();
        Self {
            trace_id: Uuid::new_v4().to_string(),
            content: content.clone(),
            original_content: RequestContent::Text(content),
            refs: Vec::new(),
            metadata: RequestMetadata::default(),
        }
    }

    /// Add a reference to the request.
    pub fn with_ref(mut self, ref_id: RefId) -> Self {
        self.refs.push(ref_id);
        self
    }

    /// Add metadata to the request.
    pub fn with_metadata(mut self, metadata: RequestMetadata) -> Self {
        self.metadata = metadata;
        self
    }
}

// =============================================================================
// Intent Types (L0 Router Output)
// =============================================================================

/// User intent classification result from L0.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "payload")]
pub enum UserIntent {
    /// Fast path: direct tool invocation without L1 overhead.
    #[serde(rename = "fast_action")]
    FastAction {
        /// Name of the tool to invoke.
        tool_name: String,
        /// Arguments for the tool.
        args: serde_json::Value,
    },

    /// Slow path: start L1 Controller for complex reasoning.
    #[serde(rename = "complex_mission")]
    ComplexMission {
        /// High-level goal extracted from the request.
        goal: String,
        /// Summarized context from L0 preprocessing.
        context_summary: String,
        /// Visual references (image RefIds).
        visual_refs: Vec<String>,
    },
}

// =============================================================================
// Tool Types (L2)
// =============================================================================

/// Output from a tool execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolOutput {
    /// Whether the tool execution was successful.
    pub success: bool,

    /// Output content (may be a RefId for large outputs).
    pub content: String,

    /// Optional structured data.
    pub data: Option<serde_json::Value>,

    /// References created during execution.
    pub created_refs: Vec<RefId>,
}

impl ToolOutput {
    /// Create a successful text output.
    pub fn text(content: impl Into<String>) -> Self {
        Self {
            success: true,
            content: content.into(),
            data: None,
            created_refs: Vec::new(),
        }
    }

    /// Create a successful output with structured data.
    pub fn with_data(mut self, data: serde_json::Value) -> Self {
        self.data = Some(data);
        self
    }

    /// Create a reference output (for large content).
    pub fn reference(ref_id: RefId, summary: impl Into<String>) -> Self {
        Self {
            success: true,
            content: format!(
                "Output saved as RefID: {}. {}",
                ref_id,
                summary.into()
            ),
            data: None,
            created_refs: vec![ref_id],
        }
    }

    /// Create a failed output.
    pub fn error(message: impl Into<String>) -> Self {
        Self {
            success: false,
            content: message.into(),
            data: None,
            created_refs: Vec::new(),
        }
    }
}

/// Tool definition for the tool registry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDefinition {
    /// Unique tool name.
    pub name: String,

    /// Human-readable description.
    pub description: String,

    /// JSON Schema for tool arguments.
    pub parameters: serde_json::Value,

    /// Whether the tool supports streaming output.
    pub supports_streaming: bool,
}

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

// =============================================================================
// Session & State Types
// =============================================================================

/// Session state for persistent conversations.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    /// Unique session ID.
    pub id: String,

    /// Current status.
    pub status: SessionStatus,

    /// Conversation history.
    pub history: Vec<HistoryEntry>,

    /// Current task state (for resurrection).
    pub task_state: Option<TaskState>,

    /// Token usage tracking.
    pub token_usage: TokenUsage,

    /// Creation timestamp.
    pub created_at: i64,

    /// Last updated timestamp.
    pub updated_at: i64,
}

/// Session status for state tracking.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SessionStatus {
    /// Session is actively processing.
    Running,
    /// Session is paused/waiting.
    Paused,
    /// Session completed successfully.
    Completed,
    /// Session failed with error.
    Failed,
}

/// Entry in conversation history.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistoryEntry {
    /// Role (user, assistant, system, tool).
    pub role: String,

    /// Content of the message.
    pub content: String,

    /// Optional tool call information.
    pub tool_call: Option<ToolCallInfo>,

    /// Timestamp.
    pub timestamp: i64,
}

/// Information about a tool call.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCallInfo {
    /// Tool name.
    pub name: String,
    /// Tool arguments.
    pub arguments: serde_json::Value,
    /// Tool result (if completed).
    pub result: Option<String>,
}

/// Task state for resurrection pattern.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskState {
    /// Current ReAct loop iteration.
    pub iteration: usize,

    /// Current goal.
    pub goal: String,

    /// Accumulated observations.
    pub observations: Vec<String>,

    /// Pending actions.
    pub pending_actions: Vec<serde_json::Value>,
}

/// Token usage tracking.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TokenUsage {
    /// Tokens used for prompts.
    pub prompt_tokens: u64,

    /// Tokens used for completions.
    pub completion_tokens: u64,

    /// Total tokens used.
    pub total_tokens: u64,

    /// Budget limit.
    pub budget_limit: u64,
}

impl TokenUsage {
    /// Create new token usage with a budget limit.
    pub fn with_budget(limit: u64) -> Self {
        Self {
            budget_limit: limit,
            ..Default::default()
        }
    }

    /// Add usage to the tracker.
    pub fn add(&mut self, prompt: u64, completion: u64) {
        self.prompt_tokens += prompt;
        self.completion_tokens += completion;
        self.total_tokens += prompt + completion;
    }

    /// Check if budget is exceeded.
    pub fn is_exceeded(&self) -> bool {
        self.total_tokens >= self.budget_limit
    }

    /// Get remaining budget.
    pub fn remaining(&self) -> u64 {
        self.budget_limit.saturating_sub(self.total_tokens)
    }
}

// =============================================================================
// Model Types (L-M)
// =============================================================================

/// Model tier for selection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ModelTier {
    /// Fast, cheap models (e.g., GPT-4o-mini).
    Fast,
    /// Balanced performance/cost models.
    Standard,
    /// High-performance models (e.g., GPT-4o, Claude-3.5-Sonnet).
    Premium,
}

/// Provider health status.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProviderHealth {
    /// Provider is healthy.
    Healthy,
    /// Provider is degraded (slow responses).
    Degraded,
    /// Provider is unhealthy (failing requests).
    Unhealthy,
    /// Provider circuit is open (temporarily blocked).
    CircuitOpen,
}
