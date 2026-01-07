How to Use This Document (For Coding Agents)

This document is an implementation specification, not a proposal.
	•	Sections marked REQUIRED define mandatory behavior.
	•	Sections marked OPTIONAL are extensions and should not block core implementation.
	•	Code blocks define canonical APIs and patterns.
	•	Do not infer behavior that is not explicitly specified.

⸻

Problem Statement

Aiki must reliably capture end-to-end provenance for real, interactive Claude Code sessions, including:
	•	session lifecycle
	•	file edits and writes
	•	event ordering

The system must be deterministic, CI-compatible, and extensible to other AI editors.

⸻

Hard Constraints (REQUIRED)
	•	Tests MUST use a real PTY.
	•	Tests MUST exercise the real Claude Code CLI.
	•	No mocks of JJ, hooks, filesystem, or flows.
	•	Tests MUST be deterministic.
	•	Tests MUST be safe to skip unless explicitly enabled.

⸻

System Architecture

Core Abstraction (REQUIRED)

trait AgentDriver {
    fn send_prompt(&mut self, text: &str) -> Result<()>;
    fn wait_for_edit(&mut self) -> Result<()>;
    fn wait_for_write(&mut self) -> Result<()>;
    fn wait_for_completion(&mut self) -> Result<()>;
    fn exit(self) -> Result<()>;
}

Driver Implementations
	•	ClaudeCodeDriver (rexpect-based)
	•	Future: CursorDriver, ZedDriver

Drivers MUST:
	•	be tool-agnostic
	•	rely on prompt return ("> ") as completion signal
	•	encapsulate CLI-specific UX

⸻

Determinism Rules (REQUIRED)

Prompt Rules

Prompts MUST:
	•	specify exact file paths
	•	specify exact modification type
	•	forbid refactors unless explicitly required

ANTI-PATTERN

"Improve calculator.py"

CANONICAL PATTERN

Using the Edit tool, insert exactly this text at the end of calculator.py:

def multiply(a, b):
    return a * b

Do not modify any other code. Do not refactor.


⸻

Session Execution Model

States
	•	NotStarted
	•	SessionStarted
	•	PromptSubmitted
	•	EditInProgress
	•	EditCompleted
	•	SessionEnded

Transitions
	•	start → SessionStarted
	•	send_prompt → PromptSubmitted
	•	wait_for_edit → EditCompleted
	•	exit → SessionEnded

⸻

Event Model (REQUIRED)

Events MUST be written as JSONL.

Event Schema

{
  "seq": 0,
  "event_type": "change.completed",
  "session_id": "...",
  "timestamp": "2025-01-15T10:30:00Z",
  "tool": "Edit",
  "file_paths": ["calculator.py"]
}

	•	seq MUST be monotonic and is the source of ordering truth.
	•	Timestamps are informational.

⸻

Canonical Helper APIs (REQUIRED)

These helpers MUST exist and SHOULD be reused across tests:

fn wait_for_metadata(repo_path: &Path, timeout: Duration) -> Result<AikiMetadata>;
fn wait_for_metadata_for_file(repo_path: &Path, file: &str, timeout: Duration) -> Result<AikiMetadata>;
fn verify_session_id(repo_path: &Path, expected: &str) -> Result<()>;
fn capture_file_snapshot(repo_path: &Path) -> Result<FileSnapshot>;
fn verify_only_expected_changes(before: &FileSnapshot, after: &FileSnapshot, expected: &[&str]) -> Result<()>;


⸻

Test Scenarios

Scenario 1: Deterministic Single Edit

Step	Action	Expected Result
1	start session	session.started
2	send deterministic prompt	prompt.submitted
3	wait_for_edit	file modified + provenance
4	exit	session.ended


⸻

Scenario 2: Deterministic Multi-Step Session

Step	Action	Expected Result
1	Edit file1	change.completed
2	Edit file2	change.completed
3	Write file3	change.completed
4	exit	all changes share session_id

Filesystem MUST contain only expected changes.

⸻

Scenario 3: Event Ordering Integrity

The following ordering MUST hold:

session.started
→ prompt.submitted
→ change.completed
→ response.received
→ session.ended

Violations are test failures.

⸻

Scenario 4: Error Recovery
	•	Invalid edit MUST log error
	•	Session MUST continue
	•	Subsequent valid edits MUST record provenance

⸻

Scenario 5: Human + AI Interleaving
	•	Claude edit
	•	Human edit
	•	Claude edit

aiki blame MUST show correct attribution.

⸻

File Creation Order (REQUIRED)

Order	File	Purpose
1	agent_driver.rs	core abstraction
2	helpers.rs	metadata, snapshots
3	claude_driver.rs	Claude PTY driver
4	test_basic_edit.rs	Scenario 1
5	test_multi_step.rs	Scenario 2
6	test_events.rs	Scenario 3


⸻

Test Enablement

Tests MUST be gated behind:

CLAUDE_INTEGRATION_TEST=1

CI SHOULD run these tests only via manual or scheduled workflows.

⸻

CI Execution (OPTIONAL)
	•	acceptEdits mode MUST be enabled
	•	no interactive prompts allowed
	•	timeouts MUST be enforced on exit

⸻

Appendix: Guidance for Coding Agents

When implementing:
	•	Prefer clarity over cleverness
	•	Fail fast on timeouts
	•	Avoid global state
	•	Do not refactor test helpers

⸻

Non-Goals
	•	Testing Claude reasoning quality
	•	Optimizing prompt performance
	•	Simulating other editors before automation exists

⸻

Summary

This specification defines a deterministic, PTY-backed, agent-agnostic test suite that validates Aiki’s provenance guarantees across real Claude Code sessions.
