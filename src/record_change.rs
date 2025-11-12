use anyhow::{Context, Result};
use chrono::Utc;
use serde::Deserialize;
use std::io::{self, Read};
use std::path::PathBuf;

use crate::db::ProvenanceDatabase;
use crate::provenance::{
    AgentInfo, AgentType, AttributionConfidence, ChangeSummary, DetectionMethod, ProvenanceRecord,
};

/// Input data structure from Claude Code hook
#[derive(Deserialize, Debug)]
struct HookInput {
    session_id: String,
    #[allow(dead_code)]
    transcript_path: Option<String>,
    cwd: String,
    #[allow(dead_code)]
    hook_event_name: String,
    tool_name: String,
    tool_input: ToolInput,
    #[allow(dead_code)]
    tool_output: Option<String>,
}

/// Tool input data from Claude Code
#[derive(Deserialize, Debug)]
struct ToolInput {
    file_path: String,
    #[serde(default)]
    old_string: Option<String>,
    #[serde(default)]
    new_string: Option<String>,
}

/// Handle the record-change command (called by Claude Code hooks)
pub fn record_change() -> Result<()> {
    // Read JSON from stdin
    let mut stdin = io::stdin();
    let mut buffer = String::new();
    stdin
        .read_to_string(&mut buffer)
        .context("Failed to read JSON from stdin")?;

    // Parse hook data
    let hook_data: HookInput =
        serde_json::from_str(&buffer).context("Failed to parse hook JSON")?;

    // Get commit_id (JJ auto-snapshots working copy during this command)
    let commit_id = get_working_copy_commit_id(&hook_data.cwd).context(
        "Failed to get working copy commit ID. Is jj installed and the repository initialized?",
    )?;

    // Build provenance record with commit_id
    let provenance = ProvenanceRecord {
        id: None,
        agent: AgentInfo {
            agent_type: AgentType::ClaudeCode,
            version: None,
            detected_at: Utc::now(),
            confidence: AttributionConfidence::High,
            detection_method: DetectionMethod::Hook,
        },
        file_path: PathBuf::from(&hook_data.tool_input.file_path),
        session_id: hook_data.session_id.clone(),
        tool_name: hook_data.tool_name.clone(),
        timestamp: Utc::now(),
        change_summary: Some(ChangeSummary {
            old_string: hook_data.tool_input.old_string.clone(),
            new_string: hook_data.tool_input.new_string.clone(),
        }),
        jj_commit_id: Some(commit_id.clone()),
        jj_operation_id: None, // Will be filled by op_heads watcher
    };

    // Write to database
    let db_path = PathBuf::from(&hook_data.cwd)
        .join(".aiki")
        .join("provenance")
        .join("attribution.db");
    let db = ProvenanceDatabase::open(&db_path)?;
    let provenance_id = db.insert_provenance(&provenance)?;

    // Link JJ operation to DB record (async, describe the specific commit)
    link_jj_operation(&hook_data.cwd, &commit_id, provenance_id)?;

    Ok(())
}

/// Get the working copy commit ID from JJ using jj-lib
///
/// JJ automatically snapshots the working copy when running commands.
fn get_working_copy_commit_id(repo_path: &str) -> Result<String> {
    use jj_lib::object_id::ObjectId;
    use jj_lib::repo::StoreFactories;
    use jj_lib::workspace::{default_working_copy_factories, Workspace};
    use std::path::Path;

    // Create settings
    let settings = crate::jj::JJWorkspace::create_user_settings()?;

    // Get default factories
    let store_factories = StoreFactories::default();
    let working_copy_factories = default_working_copy_factories();

    // Load workspace
    let workspace = Workspace::load(
        &settings,
        Path::new(repo_path),
        &store_factories,
        &working_copy_factories,
    )
    .context("Failed to load JJ workspace. Is the repository initialized with JJ?")?;

    // Load repo at head
    let repo = workspace
        .repo_loader()
        .load_at_head()
        .context("Failed to load repository at head operation")?;

    // Get working copy commit ID
    let workspace_id = workspace.workspace_name();
    let wc_commit_id = repo
        .view()
        .get_wc_commit_id(workspace_id)
        .context("No working copy commit found for workspace")?;

    // Convert to hex string
    Ok(wc_commit_id.hex())
}

/// Link the JJ operation to the database record using jj-lib
fn link_jj_operation(repo_path: &str, commit_id_str: &str, provenance_id: i64) -> Result<()> {
    use jj_lib::backend::CommitId;
    use jj_lib::repo::{Repo, StoreFactories};
    use jj_lib::workspace::{default_working_copy_factories, Workspace};
    use std::path::Path;

    // Create settings
    let settings = crate::jj::JJWorkspace::create_user_settings()?;

    // Get default factories
    let store_factories = StoreFactories::default();
    let working_copy_factories = default_working_copy_factories();

    // Load workspace
    let workspace = Workspace::load(
        &settings,
        Path::new(repo_path),
        &store_factories,
        &working_copy_factories,
    )
    .context("Failed to load JJ workspace")?;

    // Parse commit ID from hex string
    let commit_id_bytes = hex::decode(commit_id_str).context("Invalid commit ID: not valid hex")?;
    let commit_id = CommitId::new(commit_id_bytes);

    // Load repo at head
    let repo = workspace
        .repo_loader()
        .load_at_head()
        .context("Failed to load repository at head")?;

    // Start transaction
    let mut tx = repo.start_transaction();

    // Get commit to rewrite
    let commit = tx
        .repo()
        .store()
        .get_commit(&commit_id)
        .context("Failed to load commit from store")?;

    // Rewrite commit with new description
    let description = format!("aiki:{}", provenance_id);
    let _new_commit = tx
        .repo_mut()
        .rewrite_commit(&commit)
        .set_description(description)
        .write()
        .context("Failed to write rewritten commit")?;

    // Commit transaction
    tx.commit(format!("aiki: link provenance {}", provenance_id))
        .context("Failed to commit transaction")?;

    Ok(())
}
