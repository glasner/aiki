use serde::Deserialize;
use std::path::PathBuf;

use super::session::create_session;
use crate::error::Result;
use crate::events::{
    AikiEvent, AikiSessionClearedPayload, AikiSessionResumedPayload, AikiSessionStartPayload,
    AikiShellPermissionAskedPayload, AikiTurnCompletedPayload, AikiTurnStartedPayload,
    TokenUsage,
};

// ============================================================================
// Hook Payload Structures (matches Codex native hooks API)
// ============================================================================

/// Codex hook event - discriminated by hook_event_name
#[derive(Deserialize, Debug)]
#[serde(tag = "hook_event_name")]
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
#[derive(Deserialize, Debug)]
struct SessionStartPayload {
    session_id: String,
    cwd: String,
    #[serde(default = "default_session_source")]
    source: String,
    #[allow(dead_code)]
    #[serde(default)]
    model: Option<String>,
    #[allow(dead_code)]
    #[serde(default)]
    permission_mode: Option<String>,
    #[allow(dead_code)]
    #[serde(default)]
    transcript_path: Option<String>,
}

fn default_session_source() -> String {
    "startup".to_string()
}

/// UserPromptSubmit hook payload
#[derive(Deserialize, Debug)]
struct UserPromptSubmitPayload {
    session_id: String,
    cwd: String,
    #[serde(default)]
    prompt: String,
    #[allow(dead_code)]
    #[serde(default)]
    turn_id: Option<String>,
    #[allow(dead_code)]
    #[serde(default)]
    model: Option<String>,
    #[allow(dead_code)]
    #[serde(default)]
    permission_mode: Option<String>,
    #[allow(dead_code)]
    #[serde(default)]
    transcript_path: Option<String>,
}

/// PreToolUse hook payload
#[derive(Deserialize, Debug)]
struct PreToolUsePayload {
    session_id: String,
    cwd: String,
    #[allow(dead_code)]
    tool_name: String,
    #[allow(dead_code)]
    #[serde(default)]
    tool_input: Option<serde_json::Value>,
    #[allow(dead_code)]
    #[serde(default)]
    tool_use_id: Option<String>,
    #[allow(dead_code)]
    #[serde(default)]
    turn_id: Option<String>,
    #[allow(dead_code)]
    #[serde(default)]
    model: Option<String>,
    #[allow(dead_code)]
    #[serde(default)]
    permission_mode: Option<String>,
    #[allow(dead_code)]
    #[serde(default)]
    transcript_path: Option<String>,
}

/// Stop hook payload
///
/// Unlike Claude Code, Codex carries `last_assistant_message` directly
/// in the payload — no transcript parsing needed.
#[derive(Deserialize, Debug)]
struct StopPayload {
    session_id: String,
    cwd: String,
    #[serde(default)]
    last_assistant_message: Option<String>,
    #[allow(dead_code)]
    #[serde(default)]
    stop_hook_active: Option<bool>,
    #[allow(dead_code)]
    #[serde(default)]
    turn_id: Option<String>,
    #[serde(default)]
    model: Option<String>,
    #[allow(dead_code)]
    #[serde(default)]
    permission_mode: Option<String>,
    #[serde(default)]
    transcript_path: Option<String>,
}

// ============================================================================
// Event Building
// ============================================================================

/// Build AikiEvent from Codex event read from stdin
pub fn build_aiki_event_from_stdin() -> Result<AikiEvent> {
    let event: CodexEvent = super::super::read_stdin_json()?;

    let aiki_event = match event {
        CodexEvent::SessionStart { payload } => build_session_started_event(payload),
        CodexEvent::UserPromptSubmit { payload } => build_turn_started_event(payload),
        CodexEvent::PreToolUse { payload } => build_shell_permission_asked_event(payload),
        CodexEvent::Stop { payload } => build_turn_completed_event(payload),
    };

    Ok(aiki_event)
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

    match payload.source.as_str() {
        "resume" => AikiEvent::SessionResumed(AikiSessionResumedPayload {
            session,
            cwd,
            timestamp,
        }),
        "clear" => AikiEvent::SessionCleared(AikiSessionClearedPayload {
            session,
            cwd,
            timestamp,
        }),
        _ => AikiEvent::SessionStarted(AikiSessionStartPayload {
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
    let command = payload
        .tool_input
        .as_ref()
        .and_then(|v| v.get("command"))
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    AikiEvent::ShellPermissionAsked(AikiShellPermissionAskedPayload {
        session: create_session(&payload.session_id, &payload.cwd),
        cwd: PathBuf::from(&payload.cwd),
        timestamp: chrono::Utc::now(),
        command,
    })
}

/// Build turn.completed event (maps from Stop hook)
///
/// Codex carries `last_assistant_message` directly in the payload,
/// so no transcript file parsing is needed (unlike Claude Code).
/// Token usage is extracted from the session JSONL transcript file.
fn build_turn_completed_event(payload: StopPayload) -> AikiEvent {
    let tokens = payload
        .transcript_path
        .as_deref()
        .and_then(parse_token_usage_from_transcript);

    AikiEvent::TurnCompleted(AikiTurnCompletedPayload {
        session: create_session(&payload.session_id, &payload.cwd),
        cwd: PathBuf::from(&payload.cwd),
        timestamp: chrono::Utc::now(),
        turn: crate::events::Turn::unknown(),
        response: payload.last_assistant_message.unwrap_or_default(),
        modified_files: vec![],
        tasks: Default::default(),
        tokens,
        model: payload.model,
    })
}

// ============================================================================
// Token Usage Parsing
// ============================================================================

/// A `token_count` event from a Codex session JSONL file.
#[derive(Deserialize, Debug, Clone)]
struct CodexTokenCount {
    input: u64,
    output: u64,
    #[serde(default)]
    cached_input: u64,
}

/// Parse per-turn token usage from a Codex session JSONL transcript file.
///
/// Codex writes cumulative `token_count` events to the session JSONL.
/// Per-turn usage is the delta between the last two such events.
/// If there is only one event, it is used as-is (first turn).
fn parse_token_usage_from_transcript(path: &str) -> Option<TokenUsage> {
    let content = match std::fs::read_to_string(path) {
        Ok(c) => c,
        Err(_) => return None,
    };
    parse_token_usage_from_lines(&content)
}

/// Parse per-turn token usage from JSONL content (testable without filesystem).
fn parse_token_usage_from_lines(content: &str) -> Option<TokenUsage> {
    let mut prev: Option<CodexTokenCount> = None;
    let mut last: Option<CodexTokenCount> = None;

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
            if val.get("type").and_then(|t| t.as_str()) == Some("token_count") {
                if let Ok(tc) = serde_json::from_value::<CodexTokenCount>(val) {
                    prev = last;
                    last = Some(tc);
                }
            }
        }
    }

    let last = last?;
    match prev {
        None => Some(TokenUsage {
            input: last.input,
            output: last.output,
            cache_read: last.cached_input,
            cache_created: 0,
        }),
        Some(prev) => Some(TokenUsage {
            input: last.input.saturating_sub(prev.input),
            output: last.output.saturating_sub(prev.output),
            cache_read: last.cached_input.saturating_sub(prev.cached_input),
            cache_created: 0,
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_session_start(source: &str) -> SessionStartPayload {
        SessionStartPayload {
            session_id: "test-session-123".to_string(),
            cwd: "/tmp/test".to_string(),
            source: source.to_string(),
            model: None,
            permission_mode: None,
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
        assert!(
            matches!(event, AikiEvent::SessionResumed(_)),
            "SessionStart(source=resume) should map to SessionResumed"
        );
    }

    #[test]
    fn test_session_start_clear_maps_to_session_cleared() {
        let event = build_session_started_event(make_session_start("clear"));
        assert!(
            matches!(event, AikiEvent::SessionCleared(_)),
            "SessionStart(source=clear) should map to SessionCleared"
        );
    }

    #[test]
    fn test_session_start_unknown_source_maps_to_session_started() {
        let event = build_session_started_event(make_session_start("unknown"));
        assert!(
            matches!(event, AikiEvent::SessionStarted(_)),
            "SessionStart with unknown source should fall back to SessionStarted"
        );
    }

    #[test]
    fn test_session_start_deserialization_with_source() {
        let json = r#"{"hook_event_name":"SessionStart","session_id":"abc","cwd":"/tmp","source":"resume"}"#;
        let event: CodexEvent = serde_json::from_str(json).unwrap();
        match event {
            CodexEvent::SessionStart { payload } => {
                assert_eq!(payload.source, "resume");
            }
            _ => panic!("Expected SessionStart variant"),
        }
    }

    #[test]
    fn test_session_start_deserialization_defaults_to_startup() {
        let json = r#"{"hook_event_name":"SessionStart","session_id":"abc","cwd":"/tmp"}"#;
        let event: CodexEvent = serde_json::from_str(json).unwrap();
        match event {
            CodexEvent::SessionStart { payload } => {
                assert_eq!(payload.source, "startup");
            }
            _ => panic!("Expected SessionStart variant"),
        }
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
                let cmd = payload
                    .tool_input
                    .as_ref()
                    .and_then(|v| v.get("command"))
                    .and_then(|v| v.as_str());
                assert_eq!(cmd, Some("cargo test"));
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
            turn_id: None,
            model: None,
            permission_mode: None,
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
            tool_input: Some(serde_json::json!({"command": "cargo test"})),
            tool_use_id: None,
            turn_id: None,
            model: None,
            permission_mode: None,
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
            stop_hook_active: None,
            turn_id: None,
            model: None,
            permission_mode: None,
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
            stop_hook_active: None,
            turn_id: None,
            model: None,
            permission_mode: None,
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
            stop_hook_active: None,
            turn_id: None,
            model: Some("o3".to_string()),
            permission_mode: None,
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
    fn test_parse_token_usage_single_event() {
        let content = r#"{"type":"message","content":"hello"}
{"type":"token_count","input":1000,"output":500,"cached_input":200,"reasoning":50,"total":1750}
"#;
        let usage = parse_token_usage_from_lines(content).unwrap();
        assert_eq!(usage.input, 1000);
        assert_eq!(usage.output, 500);
        assert_eq!(usage.cache_read, 200);
        assert_eq!(usage.cache_created, 0);
    }

    #[test]
    fn test_parse_token_usage_delta_two_events() {
        let content = r#"{"type":"token_count","input":1000,"output":500,"cached_input":200,"reasoning":50,"total":1750}
{"type":"message","content":"working..."}
{"type":"token_count","input":2500,"output":1200,"cached_input":800,"reasoning":100,"total":4600}
"#;
        let usage = parse_token_usage_from_lines(content).unwrap();
        assert_eq!(usage.input, 1500);
        assert_eq!(usage.output, 700);
        assert_eq!(usage.cache_read, 600);
        assert_eq!(usage.cache_created, 0);
    }

    #[test]
    fn test_parse_token_usage_delta_three_events() {
        // Only the last two matter
        let content = r#"{"type":"token_count","input":500,"output":200,"cached_input":100,"reasoning":0,"total":800}
{"type":"token_count","input":1000,"output":500,"cached_input":200,"reasoning":50,"total":1750}
{"type":"token_count","input":2500,"output":1200,"cached_input":800,"reasoning":100,"total":4600}
"#;
        let usage = parse_token_usage_from_lines(content).unwrap();
        assert_eq!(usage.input, 1500);
        assert_eq!(usage.output, 700);
        assert_eq!(usage.cache_read, 600);
    }

    #[test]
    fn test_parse_token_usage_no_events() {
        let content = r#"{"type":"message","content":"hello"}
{"type":"message","content":"world"}
"#;
        assert!(parse_token_usage_from_lines(content).is_none());
    }

    #[test]
    fn test_parse_token_usage_empty_content() {
        assert!(parse_token_usage_from_lines("").is_none());
    }

    #[test]
    fn test_parse_token_usage_missing_cached_input_defaults_zero() {
        let content = r#"{"type":"token_count","input":1000,"output":500}
"#;
        let usage = parse_token_usage_from_lines(content).unwrap();
        assert_eq!(usage.input, 1000);
        assert_eq!(usage.output, 500);
        assert_eq!(usage.cache_read, 0);
    }
}
