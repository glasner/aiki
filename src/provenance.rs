use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Information about the AI agent that made a change
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentInfo {
    pub agent_type: AgentType,
    pub version: Option<String>,
    pub detected_at: DateTime<Utc>,
    pub confidence: AttributionConfidence,
    pub detection_method: DetectionMethod,
}

/// Type of AI agent
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AgentType {
    ClaudeCode,
    Unknown,
}

/// Confidence level of the attribution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AttributionConfidence {
    /// 100% - Hook-based detection
    High,
    /// 70-80% - lsof or directory check (Phase 3)
    Medium,
    /// 40-60% - Heuristic (Phase 3)
    Low,
    /// No detection succeeded
    Unknown,
}

/// Method used to detect the AI agent
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DetectionMethod {
    /// Claude Code PostToolUse hook
    Hook,
    /// Fallback (Phase 3)
    Unknown,
}

/// A complete provenance record for a change
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProvenanceRecord {
    /// Database ID (auto-generated, None for new records)
    pub id: Option<i64>,
    /// Information about the agent that made the change
    pub agent: AgentInfo,
    /// Path to the file that was changed
    pub file_path: PathBuf,
    /// Claude Code session ID
    pub session_id: String,
    /// Tool name used (e.g., "Edit" or "Write")
    pub tool_name: String,
    /// When the change was made
    pub timestamp: DateTime<Utc>,
    /// Details about what changed
    pub change_summary: Option<ChangeSummary>,
    /// JJ commit ID from snapshot
    pub jj_commit_id: Option<String>,
    /// JJ operation ID from op_heads watcher (filled later)
    pub jj_operation_id: Option<String>,
}

/// Summary of what changed in a file
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChangeSummary {
    /// The old content (for Edit operations)
    pub old_string: Option<String>,
    /// The new content (for Edit and Write operations)
    pub new_string: Option<String>,
}
