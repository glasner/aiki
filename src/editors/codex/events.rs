use serde::Deserialize;
use std::path::PathBuf;

use super::session::create_session;
use crate::agents::AgentType;
use crate::cache::debug_log;
use crate::error::Result;
use crate::events::{
    AikiChangeCompletedPayload, AikiEvent, AikiSessionClearedPayload, AikiSessionResumedPayload,
    AikiSessionStartPayload, AikiShellPermissionAskedPayload, AikiTurnCompletedPayload,
    AikiTurnStartedPayload, ChangeOperation, DeleteOperation, MoveOperation, TokenUsage, Turn,
    WriteOperation,
};
use crate::editors::transcript::{TranscriptEntry, TurnTranscript};
use crate::history;
use crate::error::AikiError;
use crate::jj::{jj_cmd, JJ_READONLY_ARGS};
use crate::provenance::record::ProvenanceRecord;
use crate::session::turn_state::generate_turn_id;

// ============================================================================
// Hook Payload Structures (matches Codex native hooks API)
// ============================================================================

/// Codex hook event - discriminated by hook_event_name
#[derive(Deserialize, Debug)]
#[serde(tag = "hook_event_name", deny_unknown_fields)]
enum CodexEvent {
    #[serde(rename = "SessionStart")]
    SessionStart {
        #[serde(flatten)]
        payload: SessionStartPayload,
    },
    #[serde(rename = "UserPromptSubmit")]
    UserPromptSubmit {
        #[serde(flatten)]
        payload: UserPromptSubmitPayload,
    },
    #[serde(rename = "PreToolUse")]
    PreToolUse {
        #[serde(flatten)]
        payload: PreToolUsePayload,
    },
    #[serde(rename = "Stop")]
    Stop {
        #[serde(flatten)]
        payload: StopPayload,
    },
}

/// SessionStart hook payload
///
/// Codex provides a `source` field indicating how the session started:
/// - "startup" - New session started
/// - "resume" - Session resumed
/// - "clear" - Session after clear
/// No "compact" variant — Codex doesn't have PreCompact.
#[derive(Deserialize, Debug, Clone, Copy)]
enum PermissionMode {
    #[serde(rename = "default")]
    Default,
    #[serde(rename = "acceptEdits")]
    AcceptEdits,
    #[serde(rename = "plan")]
    Plan,
    #[serde(rename = "dontAsk")]
    DontAsk,
    #[serde(rename = "bypassPermissions")]
    BypassPermissions,
}

#[derive(Deserialize, Debug, Clone, Copy)]
enum SessionStartSource {
    #[serde(rename = "startup")]
    Startup,
    #[serde(rename = "resume")]
    Resume,
    #[serde(rename = "clear")]
    Clear,
}

#[derive(Deserialize, Debug)]
#[serde(deny_unknown_fields)]
struct SessionStartPayload {
    session_id: String,
    cwd: String,
    source: SessionStartSource,
    #[allow(dead_code)]
    model: String,
    #[allow(dead_code)]
    permission_mode: PermissionMode,
    #[allow(dead_code)]
    transcript_path: Option<String>,
}

/// UserPromptSubmit hook payload
#[derive(Deserialize, Debug)]
#[serde(deny_unknown_fields)]
struct UserPromptSubmitPayload {
    session_id: String,
    cwd: String,
    prompt: String,
    #[allow(dead_code)]
    turn_id: String,
    #[allow(dead_code)]
    model: String,
    #[allow(dead_code)]
    permission_mode: PermissionMode,
    #[allow(dead_code)]
    transcript_path: Option<String>,
}

#[derive(Deserialize, Debug)]
#[serde(deny_unknown_fields)]
struct PreToolUseToolInput {
    command: String,
}

/// PreToolUse hook payload
#[derive(Deserialize, Debug)]
#[serde(deny_unknown_fields)]
struct PreToolUsePayload {
    session_id: String,
    cwd: String,
    #[allow(dead_code)]
    tool_name: String,
    #[allow(dead_code)]
    tool_input: PreToolUseToolInput,
    #[allow(dead_code)]
    tool_use_id: String,
    #[allow(dead_code)]
    turn_id: String,
    #[allow(dead_code)]
    model: String,
    #[allow(dead_code)]
    permission_mode: PermissionMode,
    #[allow(dead_code)]
    transcript_path: Option<String>,
}

/// Stop hook payload
///
/// Unlike Claude Code, Codex carries `last_assistant_message` directly
/// in the payload — no transcript parsing needed.
#[derive(Deserialize, Debug)]
#[serde(deny_unknown_fields)]
struct StopPayload {
    session_id: String,
    cwd: String,
    last_assistant_message: Option<String>,
    #[allow(dead_code)]
    stop_hook_active: bool,
    #[allow(dead_code)]
    turn_id: String,
    model: String,
    #[allow(dead_code)]
    permission_mode: PermissionMode,
    transcript_path: Option<String>,
}

// ============================================================================
// Event Building
// ============================================================================

/// Build AikiEvent from Codex event read from stdin
pub struct BuiltCodexEvents {
    pub supplemental_events: Vec<AikiEvent>,
    pub primary_event: AikiEvent,
}

pub fn build_aiki_event_from_stdin() -> Result<BuiltCodexEvents> {
    let event: CodexEvent = super::super::read_stdin_json()?;
    build_aiki_event_from_parsed(event)
}

pub(crate) fn build_aiki_event_from_json_str(json: &str) -> Result<BuiltCodexEvents> {
    let event: CodexEvent = serde_json::from_str(json).map_err(anyhow::Error::from)?;
    build_aiki_event_from_parsed(event)
}

fn build_aiki_event_from_parsed(event: CodexEvent) -> Result<BuiltCodexEvents> {
    let built = match event {
        CodexEvent::SessionStart { payload } => BuiltCodexEvents {
            supplemental_events: vec![],
            primary_event: build_session_started_event(payload),
        },
        CodexEvent::UserPromptSubmit { payload } => BuiltCodexEvents {
            supplemental_events: vec![],
            primary_event: build_turn_started_event(payload),
        },
        CodexEvent::PreToolUse { payload } => BuiltCodexEvents {
            supplemental_events: vec![],
            primary_event: build_shell_permission_asked_event(payload),
        },
        CodexEvent::Stop { payload } => build_stop_events(payload),
    };

    Ok(built)
}

/// Build session event based on SessionStart source field
///
/// Codex emits SessionStart for session lifecycle events.
/// The `source` field distinguishes them:
/// - "startup" or unknown → SessionStarted
/// - "resume" → SessionResumed
/// - "clear" → SessionCleared
/// No "compact" variant (Codex doesn't have PreCompact).
fn build_session_started_event(payload: SessionStartPayload) -> AikiEvent {
    let session = create_session(&payload.session_id, &payload.cwd);
    let cwd = PathBuf::from(&payload.cwd);
    let timestamp = chrono::Utc::now();

    match payload.source {
        SessionStartSource::Resume => AikiEvent::SessionResumed(AikiSessionResumedPayload {
            session,
            cwd,
            timestamp,
        }),
        SessionStartSource::Clear => AikiEvent::SessionCleared(AikiSessionClearedPayload {
            session,
            cwd,
            timestamp,
        }),
        SessionStartSource::Startup => AikiEvent::SessionStarted(AikiSessionStartPayload {
            session,
            cwd,
            timestamp,
        }),
    }
}

/// Build turn.started event (maps from UserPromptSubmit hook)
fn build_turn_started_event(payload: UserPromptSubmitPayload) -> AikiEvent {
    AikiEvent::TurnStarted(AikiTurnStartedPayload {
        session: create_session(&payload.session_id, &payload.cwd),
        cwd: PathBuf::from(&payload.cwd),
        timestamp: chrono::Utc::now(),
        turn: crate::events::Turn::unknown(),
        prompt: payload.prompt,
        injected_refs: vec![],
    })
}

/// Build shell.permission_asked event (Codex currently only has Bash tool)
fn build_shell_permission_asked_event(payload: PreToolUsePayload) -> AikiEvent {
    AikiEvent::ShellPermissionAsked(AikiShellPermissionAskedPayload {
        session: create_session(&payload.session_id, &payload.cwd),
        cwd: PathBuf::from(&payload.cwd),
        timestamp: chrono::Utc::now(),
        command: payload.tool_input.command,
    })
}

/// Build turn.completed event (maps from Stop hook)
///
/// Codex carries `last_assistant_message` and `model` directly in the payload,
/// so those take precedence over transcript data. Token usage comes from the
/// transcript via the shared `TurnTranscript` aggregation.
fn build_turn_completed_event(payload: StopPayload) -> AikiEvent {
    let transcript = payload
        .transcript_path
        .as_deref()
        .map(|p| TurnTranscript::parse(p, parse_transcript_lines))
        .unwrap_or_default();

    AikiEvent::TurnCompleted(AikiTurnCompletedPayload {
        session: create_session(&payload.session_id, &payload.cwd),
        cwd: PathBuf::from(&payload.cwd),
        timestamp: chrono::Utc::now(),
        turn: crate::events::Turn::unknown(),
        response: payload.last_assistant_message.unwrap_or(transcript.response),
        modified_files: vec![],
        tasks: Default::default(),
        tokens: transcript.tokens,
        model: Some(payload.model).or(transcript.model),
    })
}

fn build_stop_events(payload: StopPayload) -> BuiltCodexEvents {
    let supplemental_events = build_change_completed_fallback_events(&payload);
    let primary_event = build_turn_completed_event(payload);

    BuiltCodexEvents {
        supplemental_events,
        primary_event,
    }
}

fn build_change_completed_fallback_events(payload: &StopPayload) -> Vec<AikiEvent> {
    let session = create_session(&payload.session_id, &payload.cwd);
    let cwd = PathBuf::from(&payload.cwd);
    let turn = current_turn_for_session(session.uuid());

    if turn.is_known() && current_change_already_attributed(&cwd, session.uuid(), &turn) {
        return vec![];
    }

    let operations = match current_change_operations(&cwd) {
        Ok(ops) => ops,
        Err(err) => {
            debug_log(|| format!("Codex stop fallback skipped (diff failed): {}", err));
            return vec![];
        }
    };

    if operations.is_empty() {
        return vec![];
    }

    let timestamp = chrono::Utc::now();

    operations
        .into_iter()
        .map(|operation| {
            AikiEvent::ChangeCompleted(AikiChangeCompletedPayload {
                session: session.clone(),
                cwd: cwd.clone(),
                timestamp,
                tool_name: "codex-stop-fallback".to_string(),
                success: true,
                turn: turn.clone(),
                operation,
            })
        })
        .collect()
}

fn current_turn_for_session(session_uuid: &str) -> Turn {
    match history::get_current_turn_info(&crate::global::global_aiki_dir(), session_uuid) {
        Ok((turn_number, source)) if turn_number > 0 => Turn::new(
            turn_number,
            generate_turn_id(session_uuid, turn_number),
            source.to_string(),
        ),
        Ok(_) => Turn::unknown(),
        Err(err) => {
            debug_log(|| {
                format!(
                    "Codex stop fallback turn lookup failed for {}: {}",
                    session_uuid, err
                )
            });
            Turn::unknown()
        }
    }
}

fn current_change_already_attributed(cwd: &std::path::Path, session_uuid: &str, turn: &Turn) -> bool {
    if !turn.is_known() {
        return false;
    }

    let description = match current_change_description(cwd) {
        Ok(description) => description,
        Err(err) => {
            debug_log(|| format!("Codex stop fallback description read failed: {}", err));
            return false;
        }
    };

    description_has_codex_turn_metadata(&description, session_uuid, &turn.id)
}

fn current_change_description(cwd: &std::path::Path) -> Result<String> {
    let output = jj_cmd()
        .current_dir(cwd)
        .args(["log", "-r", "@", "-T", "description"])
        .args(JJ_READONLY_ARGS)
        .output()
        .map_err(|e| AikiError::JjCommandFailed(format!("Failed to execute jj log: {}", e)))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(AikiError::JjCommandFailed(format!(
            "jj log failed: {}",
            stderr
        )));
    }

    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

fn description_has_codex_turn_metadata(description: &str, session_uuid: &str, turn_id: &str) -> bool {
    match ProvenanceRecord::from_description(description) {
        Ok(Some(record)) => {
            record.agent.agent_type == AgentType::Codex
                && record.session_id == session_uuid
                && record.turn_id == turn_id
        }
        _ => false,
    }
}

fn current_change_operations(cwd: &std::path::Path) -> Result<Vec<ChangeOperation>> {
    let workspace = crate::jj::JJWorkspace::find(cwd)?;
    let output = jj_cmd()
        .arg("diff")
        .arg("-r")
        .arg("@")
        .arg("--summary")
        .current_dir(workspace.workspace_root())
        .output()
        .map_err(|e| AikiError::JjCommandFailed(format!("Failed to execute jj diff: {}", e)))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(AikiError::JjCommandFailed(format!(
            "jj diff failed: {}",
            stderr
        )));
    }

    Ok(parse_summary_operations(&String::from_utf8_lossy(&output.stdout)))
}

fn parse_summary_operations(summary: &str) -> Vec<ChangeOperation> {
    let mut operations = Vec::new();

    for line in summary.lines() {
        let line = line.trim();
        if line.len() < 3 {
            continue;
        }

        if let Some(path) = line.strip_prefix("A ").or_else(|| line.strip_prefix("M ")) {
            let path = path.trim();
            if !path.is_empty() {
                operations.push(ChangeOperation::Write(WriteOperation {
                    file_paths: vec![path.to_string()],
                    edit_details: vec![],
                }));
            }
            continue;
        }

        if let Some(path) = line.strip_prefix("D ") {
            let path = path.trim();
            if !path.is_empty() {
                operations.push(ChangeOperation::Delete(DeleteOperation {
                    file_paths: vec![path.to_string()],
                }));
            }
            continue;
        }

        if let Some((source, destination)) =
            line.strip_prefix("R ").and_then(parse_move_summary_line)
        {
            operations.push(ChangeOperation::Move(MoveOperation {
                file_paths: vec![destination.clone()],
                source_paths: vec![source],
                destination_paths: vec![destination],
            }));
        }
    }

    operations
}

fn parse_move_summary_line(content: &str) -> Option<(String, String)> {
    let inner = content.trim().strip_prefix('{')?.strip_suffix('}')?;
    let mut parts = inner.split(" => ");
    let source = parts.next()?.trim();
    let destination = parts.next()?.trim();
    if source.is_empty() || destination.is_empty() {
        return None;
    }
    Some((source.to_string(), destination.to_string()))
}

// ============================================================================
// Token Usage Parsing
// ============================================================================

/// Token usage counts from `last_token_usage` / `total_token_usage` in Codex
/// session JSONL `event_msg` events with `payload.type == "token_count"`.
#[derive(Deserialize, Debug, Clone)]
struct CodexTokenUsageDetail {
    #[serde(default)]
    input_tokens: u64,
    #[serde(default)]
    output_tokens: u64,
    #[serde(default)]
    cached_input_tokens: u64,
}

/// The `info` object inside a `token_count` payload.
#[derive(Deserialize, Debug, Clone)]
struct CodexTokenCountInfo {
    last_token_usage: CodexTokenUsageDetail,
}

/// Payload of a `token_count` event_msg.
#[derive(Deserialize, Debug, Clone)]
struct CodexTokenCountPayload {
    /// `null` on the initial event before any API call completes.
    info: Option<CodexTokenCountInfo>,
}

/// Top-level JSONL line: `{"type":"event_msg","payload":{...}}`
#[derive(Deserialize, Debug, Clone)]
struct CodexEventMsg {
    payload: CodexTokenCountPayload,
}

/// Parse Codex JSONL content into transcript entries.
///
/// Extracts `token_count` events, each representing one API call's usage.
fn parse_transcript_lines(content: &str) -> Vec<TranscriptEntry> {
    let mut entries = Vec::new();

    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        // Quick check before full parse
        if !line.contains("\"token_count\"") {
            continue;
        }
        if let Ok(val) = serde_json::from_str::<serde_json::Value>(line) {
            let is_token_count = val
                .get("payload")
                .and_then(|p| p.get("type"))
                .and_then(|t| t.as_str())
                == Some("token_count");
            if is_token_count {
                if let Ok(msg) = serde_json::from_value::<CodexEventMsg>(val) {
                    if let Some(info) = msg.payload.info {
                        let u = info.last_token_usage;
                        entries.push(TranscriptEntry {
                            response: None,
                            model: None,
                            tokens: Some(TokenUsage {
                                input: u.input_tokens,
                                output: u.output_tokens,
                                cache_read: u.cached_input_tokens,
                                cache_created: 0,
                            }),
                        });
                    }
                }
            }
        }
    }

    entries
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_session_start(source: &str) -> SessionStartPayload {
        SessionStartPayload {
            session_id: "test-session-123".to_string(),
            cwd: "/tmp/test".to_string(),
            source: match source {
                "resume" => SessionStartSource::Resume,
                "clear" => SessionStartSource::Clear,
                _ => SessionStartSource::Startup,
            },
            model: "o3".to_string(),
            permission_mode: PermissionMode::Default,
            transcript_path: None,
        }
    }

    #[test]
    fn test_session_start_startup_maps_to_session_started() {
        let event = build_session_started_event(make_session_start("startup"));
        assert!(
            matches!(event, AikiEvent::SessionStarted(_)),
            "SessionStart(source=startup) should map to SessionStarted"
        );
    }

    #[test]
    fn test_session_start_resume_maps_to_session_resumed() {
        let event = build_session_started_event(make_session_start("resume"));
        match event {
            AikiEvent::SessionResumed(payload) => {
                assert_eq!(payload.session.external_id(), "test-session-123");
                assert_eq!(payload.cwd, PathBuf::from("/tmp/test"));
            }
            _ => panic!("SessionStart(source=resume) should map to SessionResumed"),
        }
    }

    #[test]
    fn test_session_start_clear_maps_to_session_cleared() {
        let event = build_session_started_event(make_session_start("clear"));
        match event {
            AikiEvent::SessionCleared(payload) => {
                assert_eq!(payload.session.external_id(), "test-session-123");
                assert_eq!(payload.cwd, PathBuf::from("/tmp/test"));
            }
            _ => panic!("SessionStart(source=clear) should map to SessionCleared"),
        }
    }

    #[test]
    fn test_session_start_deserialization_with_source() {
        let json = r#"{"hook_event_name":"SessionStart","session_id":"abc","cwd":"/tmp","source":"resume","model":"o3","permission_mode":"default","transcript_path":null}"#;
        let event: CodexEvent = serde_json::from_str(json).unwrap();
        match event {
            CodexEvent::SessionStart { payload } => {
                assert!(matches!(payload.source, SessionStartSource::Resume));
            }
            _ => panic!("Expected SessionStart variant"),
        }
    }

    #[test]
    fn test_session_start_deserialization_requires_source() {
        let json = r#"{"hook_event_name":"SessionStart","session_id":"abc","cwd":"/tmp","model":"o3","permission_mode":"default","transcript_path":null}"#;
        assert!(serde_json::from_str::<CodexEvent>(json).is_err());
    }

    #[test]
    fn test_user_prompt_submit_deserialization() {
        let json = r#"{"hook_event_name":"UserPromptSubmit","session_id":"abc","cwd":"/tmp","prompt":"Fix the bug","turn_id":"turn-1","model":"o3","permission_mode":"default","transcript_path":null}"#;
        let event: CodexEvent = serde_json::from_str(json).unwrap();
        match event {
            CodexEvent::UserPromptSubmit { payload } => {
                assert_eq!(payload.prompt, "Fix the bug");
                assert_eq!(payload.session_id, "abc");
            }
            _ => panic!("Expected UserPromptSubmit variant"),
        }
    }

    #[test]
    fn test_pre_tool_use_deserialization() {
        let json = r#"{"hook_event_name":"PreToolUse","session_id":"abc","cwd":"/tmp","tool_name":"Bash","tool_input":{"command":"cargo test"},"tool_use_id":"tool-xyz","turn_id":"turn-1","model":"o3","permission_mode":"default","transcript_path":null}"#;
        let event: CodexEvent = serde_json::from_str(json).unwrap();
        match event {
            CodexEvent::PreToolUse { payload } => {
                assert_eq!(payload.tool_name, "Bash");
                assert_eq!(payload.tool_input.command, "cargo test");
            }
            _ => panic!("Expected PreToolUse variant"),
        }
    }

    #[test]
    fn test_stop_deserialization() {
        let json = r#"{"hook_event_name":"Stop","session_id":"abc","cwd":"/tmp","last_assistant_message":"Done fixing","stop_hook_active":true,"turn_id":"turn-1","model":"o3","permission_mode":"default","transcript_path":null}"#;
        let event: CodexEvent = serde_json::from_str(json).unwrap();
        match event {
            CodexEvent::Stop { payload } => {
                assert_eq!(
                    payload.last_assistant_message,
                    Some("Done fixing".to_string())
                );
            }
            _ => panic!("Expected Stop variant"),
        }
    }

    #[test]
    fn test_turn_started_event_uses_prompt() {
        let payload = UserPromptSubmitPayload {
            session_id: "test-session".to_string(),
            cwd: "/tmp/test".to_string(),
            prompt: "Fix the login bug".to_string(),
            turn_id: "turn-1".to_string(),
            model: "o3".to_string(),
            permission_mode: PermissionMode::Default,
            transcript_path: None,
        };
        let event = build_turn_started_event(payload);
        match event {
            AikiEvent::TurnStarted(p) => {
                assert_eq!(p.prompt, "Fix the login bug");
            }
            _ => panic!("Expected TurnStarted"),
        }
    }

    #[test]
    fn test_shell_permission_extracts_command() {
        let payload = PreToolUsePayload {
            session_id: "test-session".to_string(),
            cwd: "/tmp/test".to_string(),
            tool_name: "Bash".to_string(),
            tool_input: PreToolUseToolInput {
                command: "cargo test".to_string(),
            },
            tool_use_id: "tool-1".to_string(),
            turn_id: "turn-1".to_string(),
            model: "o3".to_string(),
            permission_mode: PermissionMode::Default,
            transcript_path: None,
        };
        let event = build_shell_permission_asked_event(payload);
        match event {
            AikiEvent::ShellPermissionAsked(p) => {
                assert_eq!(p.command, "cargo test");
            }
            _ => panic!("Expected ShellPermissionAsked"),
        }
    }

    #[test]
    fn test_turn_completed_uses_last_assistant_message() {
        let payload = StopPayload {
            session_id: "test-session".to_string(),
            cwd: "/tmp/test".to_string(),
            last_assistant_message: Some("I fixed the bug".to_string()),
            stop_hook_active: true,
            turn_id: "turn-1".to_string(),
            model: "o3".to_string(),
            permission_mode: PermissionMode::Default,
            transcript_path: None,
        };
        let event = build_turn_completed_event(payload);
        match event {
            AikiEvent::TurnCompleted(p) => {
                assert_eq!(p.response, "I fixed the bug");
            }
            _ => panic!("Expected TurnCompleted"),
        }
    }

    #[test]
    fn test_turn_completed_empty_message() {
        let payload = StopPayload {
            session_id: "test-session".to_string(),
            cwd: "/tmp/test".to_string(),
            last_assistant_message: None,
            stop_hook_active: true,
            turn_id: "turn-1".to_string(),
            model: "o3".to_string(),
            permission_mode: PermissionMode::Default,
            transcript_path: None,
        };
        let event = build_turn_completed_event(payload);
        match event {
            AikiEvent::TurnCompleted(p) => {
                assert_eq!(p.response, "");
            }
            _ => panic!("Expected TurnCompleted"),
        }
    }

    #[test]
    fn test_turn_completed_extracts_model() {
        let payload = StopPayload {
            session_id: "test-session".to_string(),
            cwd: "/tmp/test".to_string(),
            last_assistant_message: None,
            stop_hook_active: true,
            turn_id: "turn-1".to_string(),
            model: "o3".to_string(),
            permission_mode: PermissionMode::Default,
            transcript_path: None,
        };
        let event = build_turn_completed_event(payload);
        match event {
            AikiEvent::TurnCompleted(p) => {
                assert_eq!(p.model, Some("o3".to_string()));
            }
            _ => panic!("Expected TurnCompleted"),
        }
    }

    #[test]
    fn test_user_prompt_submit_rejects_unknown_fields() {
        let json = r#"{"hook_event_name":"UserPromptSubmit","session_id":"abc","cwd":"/tmp","prompt":"Fix the bug","turn_id":"turn-1","model":"o3","permission_mode":"default","transcript_path":null,"extra":"nope"}"#;
        assert!(serde_json::from_str::<CodexEvent>(json).is_err());
    }

    #[test]
    fn test_pre_tool_use_requires_command_string() {
        let json = r#"{"hook_event_name":"PreToolUse","session_id":"abc","cwd":"/tmp","tool_name":"Bash","tool_input":{},"tool_use_id":"tool-xyz","turn_id":"turn-1","model":"o3","permission_mode":"default","transcript_path":null}"#;
        assert!(serde_json::from_str::<CodexEvent>(json).is_err());
    }

    use crate::editors::transcript::TurnTranscript;

    /// Helper: parse content and aggregate via TurnTranscript
    fn parse_and_aggregate(content: &str) -> TurnTranscript {
        TurnTranscript::from_entries(parse_transcript_lines(content))
    }

    #[test]
    fn test_parse_token_usage_single_event() {
        let content = r#"{"type":"event_msg","payload":{"type":"agent_message","message":"hello"}}
{"type":"event_msg","payload":{"type":"token_count","info":{"last_token_usage":{"input_tokens":1000,"output_tokens":500,"cached_input_tokens":200,"reasoning_output_tokens":50,"total_tokens":1750},"total_token_usage":{"input_tokens":1000,"output_tokens":500,"cached_input_tokens":200,"reasoning_output_tokens":50,"total_tokens":1750},"model_context_window":258400},"rate_limits":null}}
"#;
        let extract = parse_and_aggregate(content);
        let usage = extract.tokens.unwrap();
        assert_eq!(usage.input, 1000);
        assert_eq!(usage.output, 500);
        assert_eq!(usage.cache_read, 200);
        assert_eq!(usage.cache_created, 0);
    }

    #[test]
    fn test_parse_token_usage_sums_all_events() {
        // Multiple API calls in one turn (tool-use loop) — sum all last_token_usage entries
        let content = r#"{"type":"event_msg","payload":{"type":"token_count","info":{"last_token_usage":{"input_tokens":1000,"output_tokens":500,"cached_input_tokens":200,"reasoning_output_tokens":50,"total_tokens":1750},"total_token_usage":{"input_tokens":1000,"output_tokens":500,"cached_input_tokens":200,"reasoning_output_tokens":50,"total_tokens":1750},"model_context_window":258400},"rate_limits":null}}
{"type":"event_msg","payload":{"type":"agent_message","message":"working..."}}
{"type":"event_msg","payload":{"type":"token_count","info":{"last_token_usage":{"input_tokens":24014,"output_tokens":98,"cached_input_tokens":23808,"reasoning_output_tokens":13,"total_tokens":24112},"total_token_usage":{"input_tokens":47759,"output_tokens":249,"cached_input_tokens":27264,"reasoning_output_tokens":79,"total_tokens":48008},"model_context_window":258400},"rate_limits":null}}
"#;
        let extract = parse_and_aggregate(content);
        let usage = extract.tokens.unwrap();
        // Sum of both last_token_usage entries
        assert_eq!(usage.input, 1000 + 24014);
        assert_eq!(usage.output, 500 + 98);
        assert_eq!(usage.cache_read, 200 + 23808);
        assert_eq!(usage.cache_created, 0);
    }

    #[test]
    fn test_parse_token_usage_skips_null_info() {
        // First token_count event has info: null, second has data
        let content = r#"{"type":"event_msg","payload":{"type":"token_count","info":null,"rate_limits":null}}
{"type":"event_msg","payload":{"type":"token_count","info":{"last_token_usage":{"input_tokens":5000,"output_tokens":300,"cached_input_tokens":1000,"reasoning_output_tokens":20,"total_tokens":5300},"total_token_usage":{"input_tokens":5000,"output_tokens":300,"cached_input_tokens":1000,"reasoning_output_tokens":20,"total_tokens":5300},"model_context_window":258400},"rate_limits":null}}
"#;
        let extract = parse_and_aggregate(content);
        let usage = extract.tokens.unwrap();
        assert_eq!(usage.input, 5000);
        assert_eq!(usage.output, 300);
        assert_eq!(usage.cache_read, 1000);
    }

    #[test]
    fn test_parse_token_usage_no_events() {
        let content = r#"{"type":"event_msg","payload":{"type":"agent_message","message":"hello"}}
{"type":"event_msg","payload":{"type":"agent_message","message":"world"}}
"#;
        assert!(parse_and_aggregate(content).tokens.is_none());
    }

    #[test]
    fn test_parse_token_usage_empty_content() {
        assert!(parse_and_aggregate("").tokens.is_none());
    }

    #[test]
    fn test_parse_token_usage_only_null_info() {
        // Only token_count events with info: null — should return no tokens
        let content = r#"{"type":"event_msg","payload":{"type":"token_count","info":null,"rate_limits":null}}
"#;
        assert!(parse_and_aggregate(content).tokens.is_none());
    }

    #[test]
    fn test_parse_summary_operations_maps_write_delete_and_move() {
        let summary = "\
M src/main.rs
A src/new.rs
D src/old.rs
R {src/from.rs => src/to.rs}
";

        let operations = parse_summary_operations(summary);
        assert_eq!(operations.len(), 4);

        match &operations[0] {
            ChangeOperation::Write(op) => assert_eq!(op.file_paths, vec!["src/main.rs"]),
            _ => panic!("Expected write operation for modified file"),
        }

        match &operations[1] {
            ChangeOperation::Write(op) => assert_eq!(op.file_paths, vec!["src/new.rs"]),
            _ => panic!("Expected write operation for added file"),
        }

        match &operations[2] {
            ChangeOperation::Delete(op) => assert_eq!(op.file_paths, vec!["src/old.rs"]),
            _ => panic!("Expected delete operation"),
        }

        match &operations[3] {
            ChangeOperation::Move(op) => {
                assert_eq!(op.source_paths, vec!["src/from.rs"]);
                assert_eq!(op.destination_paths, vec!["src/to.rs"]);
            }
            _ => panic!("Expected move operation"),
        }
    }

    #[test]
    fn test_description_has_codex_turn_metadata_matches_current_turn() {
        let description = "\
[aiki]
author=codex
author_type=agent
session=550e8400-e29b-41d4-a716-446655440000
tool=apply_patch
confidence=High
method=Hook
turn=2
turn_id=turn-2
turn_source=user
[/aiki]
";

        assert!(description_has_codex_turn_metadata(
            description,
            "550e8400-e29b-41d4-a716-446655440000",
            "turn-2"
        ));
        assert!(!description_has_codex_turn_metadata(
            description,
            "550e8400-e29b-41d4-a716-446655440000",
            "turn-3"
        ));
    }
}
