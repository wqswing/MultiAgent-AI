use serde::{Deserialize, Serialize};

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
