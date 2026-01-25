pub mod otel;
pub mod state;

use crate::cache::debug_log;
use crate::editors::HookCommandOutput;
use crate::error::Result;
use crate::event_bus;
use crate::events::{
    AikiEvent, AikiSessionEndedPayload, AikiTurnCompletedPayload, AikiTurnStartedPayload,
    TurnSource,
};
use crate::provenance::{AgentType, DetectionMethod};
use crate::session::AikiSession;
use chrono::Utc;
use serde::Deserialize;
use std::path::PathBuf;

/// Codex notify payload for `agent-turn-complete` event
///
/// Codex appends the JSON payload as the final CLI argument to the notify command.
/// Fields: type, thread-id, turn-id, cwd, input-messages, last-assistant-message
#[derive(Debug, Deserialize)]
pub struct NotifyPayload {
    /// Event type (always "agent-turn-complete" for now)
    #[serde(rename = "type")]
    pub event_type: String,
    /// Thread/conversation ID (same as OTel `conversation.id`)
    #[serde(rename = "thread-id")]
    pub thread_id: String,
    /// Codex's internal turn ID (intentionally unused by aiki).
    ///
    /// Aiki generates its own deterministic turn_id as `{conversation_id}:{turn_number}`
    /// for cross-agent consistency. Codex's turn-id is opaque and not guaranteed stable
    /// across restarts. We capture it here only for debugging/logging purposes.
    #[serde(rename = "turn-id")]
    #[allow(dead_code)]
    pub turn_id: Option<String>,
    /// Working directory
    pub cwd: String,
    /// The agent's complete response for this turn
    #[serde(rename = "last-assistant-message")]
    pub last_assistant_message: Option<String>,
    /// Input messages (user prompts for this turn)
    #[serde(rename = "input-messages")]
    #[allow(dead_code)]
    pub input_messages: Option<serde_json::Value>,
}

fn extract_prompt_from_input_messages(
    input_messages: Option<&serde_json::Value>,
) -> Option<String> {
    let messages = input_messages?.as_array()?;

    for message in messages.iter().rev() {
        let role = message.get("role").and_then(|v| v.as_str());
        if role != Some("user") {
            continue;
        }

        let content = message.get("content")?;
        if let Some(text) = content.as_str() {
            return Some(text.to_string());
        }

        if let Some(text) = content.get("text").and_then(|v| v.as_str()) {
            return Some(text.to_string());
        }

        if let Some(text) = content.get("content").and_then(|v| v.as_str()) {
            return Some(text.to_string());
        }

        if let Some(items) = content.as_array() {
            let mut chunks = Vec::new();
            for item in items {
                if let Some(text) = item.as_str() {
                    chunks.push(text);
                    continue;
                }
                if let Some(text) = item.get("text").and_then(|v| v.as_str()) {
                    chunks.push(text);
                    continue;
                }
                if let Some(text) = item.get("content").and_then(|v| v.as_str()) {
                    chunks.push(text);
                }
            }
            if !chunks.is_empty() {
                return Some(chunks.join("\n\n"));
            }
        }
    }

    None
}

/// Handle a Codex hook event
///
/// Entry point for `aiki hooks handle --agent codex --event <event_name>`
///
/// For Codex, the only event dispatched via notify is `agent-turn-complete`.
/// The JSON payload is passed as a CLI argument (not stdin).
/// Also performs TTL cleanup of stale Codex sessions.
pub fn handle(event_name: &str, payload_json: Option<&str>) -> Result<()> {
    debug_log(|| format!("Codex hook event: {}", event_name));

    // Clean up expired sessions (2h TTL) and emit final events
    cleanup_expired_sessions();

    match event_name {
        "agent-turn-complete" => handle_turn_complete(payload_json),
        other => {
            debug_log(|| format!("Unknown Codex event: {}", other));
            Ok(())
        }
    }
}

/// Clean up expired Codex sessions and emit final events.
///
/// For each expired session:
/// - If modified_files is non-empty, emit turn.completed with empty response
/// - Emit session.ended with reason "ttl_expired"
fn cleanup_expired_sessions() {
    let expired = state::cleanup_stale_sessions();

    for session_info in expired {
        let cwd = match session_info.cwd {
            Some(c) => c,
            None => continue, // No cwd known, can't dispatch events
        };

        let session = AikiSession::new(
            AgentType::Codex,
            &session_info.external_id,
            session_info.agent_version.as_deref(),
            DetectionMethod::Hook,
        );
        let now = Utc::now();

        // Emit final turn.completed if there are unreported modified_files
        if !session_info.modified_files.is_empty() {
            let turn_completed = AikiEvent::TurnCompleted(AikiTurnCompletedPayload {
                session: session.clone(),
                cwd: cwd.clone(),
                timestamp: now,
                turn: session_info.current_turn,
                turn_id: format!("{}:{}", session_info.external_id, session_info.current_turn),
                source: TurnSource::User,
                response: String::new(),
                modified_files: session_info
                    .modified_files
                    .into_iter()
                    .map(PathBuf::from)
                    .collect(),
            });
            if let Err(e) = event_bus::dispatch(turn_completed) {
                debug_log(|| format!("Failed to dispatch final turn.completed: {}", e));
            }
        }

        // Emit session.ended
        let session_ended = AikiEvent::SessionEnded(AikiSessionEndedPayload {
            session,
            cwd,
            timestamp: now,
            reason: "ttl_expired".to_string(),
        });
        if let Err(e) = event_bus::dispatch(session_ended) {
            debug_log(|| format!("Failed to dispatch session.ended: {}", e));
        }
    }
}

/// Handle the `agent-turn-complete` notify event
///
/// Reads session state (accumulated by OTel), emits `turn.completed` with
/// the response text and modified_files. Emits `turn.started` if OTel
/// hasn't dispatched it for the current turn.
fn handle_turn_complete(payload_json: Option<&str>) -> Result<()> {
    let json = match payload_json {
        Some(j) => j,
        None => {
            debug_log(|| "No payload JSON for agent-turn-complete".to_string());
            return Ok(());
        }
    };

    let payload: NotifyPayload = match serde_json::from_str(json) {
        Ok(p) => p,
        Err(e) => {
            debug_log(|| format!("Failed to parse Codex notify payload: {}", e));
            return Ok(());
        }
    };

    debug_log(|| {
        format!(
            "Codex turn complete: thread={}, cwd={}",
            payload.thread_id, payload.cwd
        )
    });

    // Read session state (accumulated by OTel receiver)
    let codex_state = state::read_state(&payload.thread_id);

    let (
        current_turn,
        modified_files,
        agent_version,
        last_turn_started,
        agent_pid,
    ): (u32, Vec<String>, Option<String>, u32, Option<u32>) = match &codex_state {
        Some(s) => (
            s.current_turn,
            s.modified_files.iter().cloned().collect(),
            s.agent_version.clone(),
            s.last_turn_started,
            s.agent_pid,
        ),
        None => {
            // No OTel state exists - notify arrived without prior OTel events
            debug_log(|| {
                format!(
                    "No OTel session state for thread {}. Notify without OTel is a no-op.",
                    payload.thread_id
                )
            });
            return Ok(());
        }
        };

    // Update session state: set cwd (normalizing any relative paths) and touch
    state::update_state(&payload.thread_id, |s| {
        s.set_cwd(PathBuf::from(&payload.cwd));
        if let Some(pid) = agent_pid {
            s.set_agent_pid(pid);
        }
        s.touch();
    });

    // Build AikiSession for event dispatch
    let session = AikiSession::for_hook(
        AgentType::Codex,
        &payload.thread_id,
        agent_version.as_deref(),
    );
    if agent_pid.is_none() {
        if let Some(pid) = session.parent_pid() {
            state::update_state(&payload.thread_id, |s| {
                s.set_agent_pid(pid);
                s.touch();
            });
        }
    }
    let cwd = PathBuf::from(&payload.cwd);
    let now = Utc::now();

    // Codex notify arrives after the prompt is sent; we still emit turn.started
    // to keep turn state and prompt history consistent with other agents.
    if last_turn_started < current_turn {
        let prompt = extract_prompt_from_input_messages(payload.input_messages.as_ref())
            .unwrap_or_default();
        let turn_started = AikiEvent::TurnStarted(AikiTurnStartedPayload {
            session: session.clone(),
            cwd: cwd.clone(),
            timestamp: now,
            turn: 0,
            turn_id: String::new(),
            source: TurnSource::User,
            prompt,
            injected_refs: vec![],
        });
        if let Err(e) = event_bus::dispatch(turn_started) {
            debug_log(|| format!("Failed to dispatch turn.started for Codex: {}", e));
        } else {
            state::update_state(&payload.thread_id, |s| {
                s.mark_turn_started(current_turn);
                s.touch();
            });
        }
    }

    // Emit turn.completed
    let turn_completed = AikiEvent::TurnCompleted(AikiTurnCompletedPayload {
        session,
        cwd,
        timestamp: now,
        turn: current_turn,
        turn_id: format!("{}:{}", payload.thread_id, current_turn),
        source: TurnSource::User,
        response: payload.last_assistant_message.unwrap_or_default(),
        modified_files: modified_files.into_iter().map(PathBuf::from).collect(),
    });
    if let Err(e) = event_bus::dispatch(turn_completed) {
        debug_log(|| format!("Failed to dispatch turn.completed for Codex: {}", e));
    }

    Ok(())
}

/// Format a Codex hook result for output
///
/// Codex notify is fire-and-forget, so we don't output anything.
/// This is a no-op that returns a success exit code.
#[must_use]
pub fn format_output() -> HookCommandOutput {
    HookCommandOutput::new(None, 0)
}

#[cfg(test)]
mod tests {
    use super::extract_prompt_from_input_messages;
    use serde_json::json;

    #[test]
    fn test_extract_prompt_from_input_messages_string() {
        let input = json!([
            {"role": "system", "content": "sys"},
            {"role": "user", "content": "Fix the login bug"},
            {"role": "assistant", "content": "Sure"},
            {"role": "user", "content": "Add tests too"}
        ]);

        assert_eq!(
            extract_prompt_from_input_messages(Some(&input)),
            Some("Add tests too".to_string())
        );
    }

    #[test]
    fn test_extract_prompt_from_input_messages_array() {
        let input = json!([
            {"role": "user", "content": [
                {"type": "text", "text": "Part 1"},
                {"type": "text", "text": "Part 2"}
            ]}
        ]);

        assert_eq!(
            extract_prompt_from_input_messages(Some(&input)),
            Some("Part 1\n\nPart 2".to_string())
        );
    }
}
