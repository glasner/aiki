use anyhow::{Context, Result};
use chrono::Utc;
use serde::Deserialize;
use std::io::{self, Read};
use std::path::PathBuf;
use std::thread;

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

/// Handle the record-change command (called by AI editor hooks)
pub fn record_change(agent_type: AgentType) -> Result<()> {
    eprintln!("=== AIKI HOOK CALLED ===");
    eprintln!("Agent type: {:?}", agent_type);

    // Read JSON from stdin
    let mut stdin = io::stdin();
    let mut buffer = String::new();
    stdin
        .read_to_string(&mut buffer)
        .context("Failed to read JSON from stdin")?;

    eprintln!("Hook input JSON length: {} bytes", buffer.len());

    // Parse hook data
    let hook_data: HookInput =
        serde_json::from_str(&buffer).context("Failed to parse hook JSON")?;

    eprintln!("Hook data parsed successfully");
    eprintln!("  Session ID: {}", hook_data.session_id);
    eprintln!("  CWD: {}", hook_data.cwd);
    eprintln!("  Tool: {}", hook_data.tool_name);
    eprintln!("  File: {}", hook_data.tool_input.file_path);

    // Get change_id from the working copy
    eprintln!("Getting working copy change ID...");
    let change_id = get_working_copy_change_id(&hook_data.cwd)
        .context("Failed to get working copy change ID. Is the repository initialized with JJ?")?;
    eprintln!("  Change ID: {}", change_id);

    // Build provenance record with change_id
    eprintln!("Building provenance record...");
    let provenance = ProvenanceRecord {
        id: None,
        agent: AgentInfo {
            agent_type,
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
        jj_change_id: Some(change_id.clone()),
        jj_operation_id: None, // Will be filled by op_heads watcher
    };

    // Write to database
    let db_path = PathBuf::from(&hook_data.cwd)
        .join(".aiki")
        .join("provenance")
        .join("attribution.db");
    eprintln!("Database path: {}", db_path.display());

    eprintln!("Opening database...");
    let db = ProvenanceDatabase::open(&db_path)?;

    eprintln!("Inserting provenance record...");
    let provenance_id = db.insert_provenance(&provenance)?;
    eprintln!("  Provenance ID: {}", provenance_id);

    // Spawn background thread to set the change description
    // This keeps the hook response time under 25ms
    let repo_path = hook_data.cwd.clone();
    let change_id_for_thread = change_id.clone();

    eprintln!("Spawning background thread to set change description...");
    thread::spawn(move || {
        eprintln!("Background thread: Setting change description...");
        // Set the change description to link it to the provenance record
        if let Err(e) = set_change_description(&repo_path, &change_id_for_thread, provenance_id) {
            eprintln!(
                "Warning: Failed to set change description in background: {}",
                e
            );
        } else {
            eprintln!("Background thread: Successfully set change description");
        }
    });

    eprintln!("=== AIKI HOOK COMPLETED SUCCESSFULLY ===");
    // Return immediately - the hook is now fast!
    Ok(())
}

/// Get the working copy change ID from JJ using jj-lib
///
/// The change_id is a stable identifier that persists across rewrites,
/// unlike commit_id which changes every time the commit content changes.
///
/// NOTE: This reads the current working copy change without snapshotting.
/// In the typical Claude Code workflow, files are already tracked by git/jj,
/// so the working copy should reflect recent changes.
fn get_working_copy_change_id(repo_path: &str) -> Result<String> {
    use jj_lib::object_id::ObjectId;
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
    .context("Failed to load JJ workspace. Is the repository initialized with JJ?")?;

    // Load repo at head
    let repo = workspace
        .repo_loader()
        .load_at_head()
        .context("Failed to load repository at head operation")?;

    // Get working copy commit
    let workspace_id = workspace.workspace_name();
    let wc_commit_id = repo
        .view()
        .get_wc_commit_id(workspace_id)
        .context("No working copy commit found for workspace")?;

    // Load the commit to get its change_id
    let commit = repo
        .store()
        .get_commit(wc_commit_id)
        .context("Failed to load working copy commit")?;

    // Return the change_id (stable identifier)
    Ok(commit.change_id().hex())
}

/// Set the description on a change to link it to a provenance record
///
/// This updates the change description to "aiki:{provenance_id}" which allows
/// us to track which provenance record corresponds to which JJ change.
/// Since change_id is stable, this link persists even as the commit is rewritten.
fn set_change_description(repo_path: &str, change_id_str: &str, provenance_id: i64) -> Result<()> {
    use jj_lib::backend::ChangeId;
    use jj_lib::object_id::ObjectId;
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

    // Parse change ID from hex string
    let change_id_bytes = hex::decode(change_id_str).context("Invalid change ID: not valid hex")?;
    let change_id = ChangeId::new(change_id_bytes);

    // Load repo at head
    let repo = workspace
        .repo_loader()
        .load_at_head()
        .context("Failed to load repository at head")?;

    // Start transaction
    let mut tx = repo.start_transaction();

    // Find the commit with this change_id
    // Get the working copy commit (it should have this change_id)
    let workspace_id = workspace.workspace_name();
    let wc_commit_id = tx
        .repo()
        .view()
        .get_wc_commit_id(workspace_id)
        .context("No working copy commit found")?;

    let commit = tx
        .repo()
        .store()
        .get_commit(wc_commit_id)
        .context("Failed to load working copy commit")?;

    // Verify this is the right change
    if commit.change_id() != &change_id {
        anyhow::bail!(
            "Change ID mismatch: expected {}, got {}",
            change_id_str,
            commit.change_id().hex()
        );
    }

    // Rewrite commit with new description
    let description = format!("aiki:{}", provenance_id);
    let new_commit = tx
        .repo_mut()
        .rewrite_commit(&commit)
        .set_description(description)
        .write()
        .context("Failed to write rewritten commit")?;

    // Rebase descendants if any
    let num_rebased = tx.repo_mut().rebase_descendants()?;
    if num_rebased > 0 {
        eprintln!("Rebased {} descendant commits", num_rebased);
    }

    // Update working copy pointer
    tx.repo_mut()
        .set_wc_commit(workspace_id.into(), new_commit.id().clone())?;

    // Commit transaction
    tx.commit(format!("aiki: link provenance {}", provenance_id))
        .context("Failed to commit transaction")?;

    Ok(())
}
