# Build Warnings Cleanup - Dead Code Analysis

This document catalogs all `#[allow(dead_code)]` annotations added to eliminate compiler warnings without changing behavior.

## Summary

| Category | Count | Can Remove? |
|----------|-------|-------------|
| Serde deserialization | 8 | No - breaks JSON parsing |
| Session management API | 8 | Only if you remove the entire session feature |
| Flow system API | 17 | Only if you remove flow composition |
| Task system API | 8 | Only if you simplify task system |
| Agent runtime API | 7 | Only if you remove agent execution |
| Event/Hook result API | 6 | Only if you simplify hook responses |
| Change operation API | 4 | Only if you remove change tracking |
| History API | 6 | Only if you remove history feature |
| Future/reserved | 3 | Could remove if feature abandoned |
| Enum variants | 3 | Error variants should stay for completeness |
| Miscellaneous | 4 | Varies |
| **Total** | **74** | |

---

## 1. Serde Deserialization Structs (8 items)

**Why keep:** These fields must exist for JSON deserialization to work correctly. The struct fields are populated by serde when parsing JSON, even if we don't read them in Rust code.

| File | Item | Justification |
|------|------|---------------|
| `editors/cursor/events.rs:62` | `BeforeSubmitPromptPayload` | Cursor hook JSON payload - fields like `generation_id`, `model`, `user_email` needed for parsing |
| `editors/cursor/events.rs:81` | `StopPayload` | Cursor stop event JSON payload |
| `editors/cursor/events.rs:100` | `BeforeShellExecutionPayload` | Cursor shell execution JSON payload |
| `editors/cursor/events.rs:119` | `AfterShellExecutionPayload` | Cursor shell completion JSON payload |
| `editors/cursor/events.rs:139` | `BeforeMcpExecutionPayload` | Cursor MCP execution JSON payload |
| `editors/cursor/events.rs:160` | `AfterMcpExecutionPayload` | Cursor MCP completion JSON payload |
| `editors/cursor/events.rs:184` | `AfterFileEditPayload` | Cursor file edit JSON payload |
| `editors/claude_code/tools.rs:88` | `WebFetchToolInput` | Claude Code tool input - `prompt` field needed for deserialization |

---

## 2. Enum Variants for Future Use (3 items)

**Why keep:** These variants store data that may be used in future features or for debugging/logging purposes.

| File | Item | Justification |
|------|------|---------------|
| `editors/claude_code/tools.rs:145` | `ClaudeTool::Internal(String)` | Stores tool name for Task/TodoRead/TodoWrite - useful for future tool tracking |
| `editors/claude_code/tools.rs:147` | `ClaudeTool::Mcp(String)` | Stores MCP tool name - useful for future MCP integration |
| `error.rs:23` | `AikiError` enum | Many variants exist for API completeness (e.g., `JjInitFailed`, `GpgSmNotSupported`) - better to have comprehensive error coverage |

---

## 3. Session Management API (8 items)

**Why keep:** These form a complete session management API that external code or future features may use.

| File | Item | Justification |
|------|------|---------------|
| `session/mod.rs:118` | `AikiSessionFile::read_agent_version()` | Allows reading cached agent version from session files |
| `session/mod.rs:405` | `count_sessions()` | Counts active sessions - useful for diagnostics/debugging |
| `session/mod.rs:424` | `SessionContext` enum | Represents session detection results (NoSession/SingleSession/MultipleSessions) |
| `session/mod.rs:440` | `get_session_context()` | Detects current session state - needed for PID-based session detection |
| `session/mod.rs:481` | `get_current_agent_type()` | Gets agent from active session - useful for context-aware commands |
| `session/mod.rs:493` | `has_active_session()` | Checks if specific session is active - for precise session lookup |
| `session/mod.rs:505` | `end_session()` | Ends a session and cleans up - needed for session lifecycle |
| `session/mod.rs:571` | `SessionMatch` struct | Result type for PID-based session lookup |

---

## 4. Flow System API (17 items)

**Why keep:** These form the complete flow composition API. External flows or future flow features need these methods.

| File | Item | Justification |
|------|------|---------------|
| `flows/composer.rs:366` | `FlowComposer::depth()` | Gets call stack depth - useful for debugging/testing cycle detection |
| `flows/composer.rs:375` | `FlowComposer::is_in_stack()` | Checks if path is in call stack - helper for testing cycle detection |
| `flows/context.rs:204` | `ContextChunk::check_id()` | Generates hash ID for chunk deduplication |
| `flows/context.rs:418` | `ContextAssembler::chunk_count()` | Gets number of accumulated chunks |
| `flows/context.rs:452` | `ContextAssembler::clear()` | Resets assembler for error recovery |
| `flows/flow_resolver.rs:57` | `FlowResolver::new()` | Creates resolver from current directory |
| `flows/flow_resolver.rs:80` | `FlowResolver::project_root()` | Gets discovered project root |
| `flows/flow_resolver.rs:87` | `FlowResolver::home_dir()` | Gets home directory for user flows |
| `flows/loader.rs:55` | `FlowLoader::new()` | Creates loader from current directory |
| `flows/loader.rs:167` | `FlowLoader::load_core_flow()` | Loads bundled core flow |
| `flows/loader.rs:176` | `FlowLoader::clear_cache()` | Resets flow cache for reloading |
| `flows/loader.rs:183` | `FlowLoader::cache_size()` | Gets cached flow count |
| `flows/loader.rs:190` | `FlowLoader::project_root()` | Gets project root via resolver |
| `flows/loader.rs:197` | `FlowLoader::home_dir()` | Gets home directory via resolver |
| `flows/loader.rs:206` | `FlowLoader::default_flows_dir()` | Gets `.aiki/flows` directory path |
| `flows/path_resolver.rs:50` | `PathResolver::new()` | Creates resolver from current directory |
| `flows/path_resolver.rs:136` | `PathResolver::resolve()` | Resolves @/, ./, ../, / paths |
| `flows/path_resolver.rs:182` | `PathResolver::resolve_tilde()` | Resolves ~ paths to home directory |

---

## 5. Flow State & Variables API (3 items)

**Why keep:** Part of the flow execution state API.

| File | Item | Justification |
|------|------|---------------|
| `flows/state.rs:82` | `AikiState::agent_type()` | Gets agent type from event - useful for agent-specific flow logic |
| `flows/state.rs:166` | `AikiState::failures_count()` | Gets failure count - useful for conditional flow logic |
| `flows/variables.rs:66` | `VariableResolver::add_env_vars()` | Adds environment variables to resolver |

---

## 6. Task System API (8 items)

**Why keep:** These form the complete task management API for querying and manipulating tasks.

| File | Item | Justification |
|------|------|---------------|
| `tasks/id.rs:113` | `is_child_of()` | Checks parent-child relationship between task IDs |
| `tasks/manager.rs:210` | `get_stopped()` | Gets tasks with Stopped status |
| `tasks/manager.rs:220` | `get_closed()` | Gets tasks with Closed status |
| `tasks/manager.rs:309` | `get_current_scopes()` | Gets current scopes as Vec (backward compat) |
| `tasks/manager.rs:323` | `get_unclosed_children()` | Gets unclosed children of parent task |
| `tasks/manager.rs:451` | `get_ready_queue_for_human()` | Gets tasks visible to humans (unassigned or human-assigned) |
| `tasks/xml.rs:45` | `XmlBuilder::with_scope()` | Sets single scope for XML response |
| `tasks/xml.rs:142` | `format_task()` | Formats task as XML element |

---

## 7. Agent Runtime API (7 items)

**Why keep:** These form the agent execution runtime API for spawning and managing agent sessions.

| File | Item | Justification |
|------|------|---------------|
| `agents/detect.rs:19` | `detect_agent_from_process_tree()` | Detects agent by walking process tree - for auto-detection |
| `agents/detect.rs:61` | `match_agent()` | Matches process name to agent type - helper for detection |
| `agents/runtime/mod.rs:64` | `AgentSessionResult::is_completed()` | Checks if session completed successfully |
| `agents/runtime/mod.rs:71` | `AgentSessionResult::is_failed()` | Checks if session failed |
| `agents/runtime/mod.rs:79` | `AgentSpawnOptions` struct | Options for spawning agent sessions - fields like `agent_override` |
| `agents/runtime/mod.rs:102` | `AgentSpawnOptions::with_agent_override()` | Builder method for agent override |
| `agents/runtime/mod.rs:113` | `AgentRuntime` trait | Trait for agent runtime implementations - `agent_type()` method |

---

## 8. Event/Hook Result API (6 items)

**Why keep:** These form the hook result builder API for constructing responses to hook events.

| File | Item | Justification |
|------|------|---------------|
| `events/result.rs:18` | `Decision::is_continue()` | Checks if decision allows operation |
| `events/result.rs:54` | `HookResult::success_with_context()` | Creates success with context string |
| `events/result.rs:73` | `HookResult::blocking_failure()` | Creates blocking failure response |
| `events/result.rs:83` | `HookResult::with_context()` | Builder to add context to result |
| `events/result.rs:90` | `HookResult::with_failure()` | Builder to add failure message |
| `events/result.rs:104` | `HookResult::is_success()` | Checks if response is successful |

---

## 9. Change Operation API (4 items)

**Why keep:** Methods for accessing change operation details in a unified way.

| File | Item | Justification |
|------|------|---------------|
| `events/change_completed.rs:276` | `ChangeOperation::file_paths()` | Gets all affected file paths (Write/Delete/Move) |
| `events/change_completed.rs:287` | `ChangeOperation::edit_details()` | Gets edit details for Write operations |
| `events/change_completed.rs:297` | `ChangeOperation::source_paths()` | Gets source paths for Move operations |
| `events/change_completed.rs:307` | `ChangeOperation::destination_paths()` | Gets destination paths for Move operations |

---

## 10. History API (6 items)

**Why keep:** These form the conversation history API for reading/writing events.

| File | Item | Justification |
|------|------|---------------|
| `history/storage.rs:97` | `read_events()` | Reads all conversation events from branch |
| `history/storage.rs:178` | `unescape_metadata_value()` | Unescapes metadata values - needed by parse_metadata_block |
| `history/storage.rs:286` | `parse_list_field()` | Parses list fields from metadata - needed by parse_metadata_block |
| `history/storage.rs:295` | `parse_metadata_block()` | Parses metadata block into ConversationEvent |
| `history/types.rs:54` | `Session` struct | Materialized session view from events |
| `history/types.rs:66` | `LogEntry` struct | Materialized log entry from response events |

---

## 11. Future/Reserved Code (3 items)

**Why keep:** Code reserved for planned features that haven't been integrated yet.

| File | Item | Justification |
|------|------|---------------|
| `flows/core/functions.rs:833` | `run_jj_split()` | Reserved for future jj split-based separation strategy |
| `flows/core/functions.rs:893` | `parse_split_output()` | Parses jj split output - needed by run_jj_split |
| `flows/core/functions.rs:985` | `task_list_size()` | Flow function for getting task queue size - part of flow function API |

---

## 12. Miscellaneous (4 items)

**Why keep:** Various items needed for specific purposes.

| File | Item | Justification |
|------|------|---------------|
| `repo.rs:73` | `RepoDetector::resolve_git_dir()` | Resolves .git directory (handles worktrees/submodules) |
| `blame.rs:25` | `BlameEntry::change_id` | Stores jj change ID - may be used for future blame features |
| `blame.rs:31` | `BlameEntry::tool_name` | Stores tool name - may be used for future blame features |
| `commands/benchmark.rs:13` | `DEFAULT_EDITS` | Documentation constant for default edit count |

---

## Recommendations for Future Cleanup

1. **Safe to remove if feature abandoned:**
   - `run_jj_split()` and `parse_split_output()` - if jj split strategy is not pursued
   - `BlameEntry::change_id` and `BlameEntry::tool_name` - if these fields won't be displayed

2. **Consider consolidating:**
   - Session management functions could be methods on a `SessionManager` struct
   - Flow resolver/loader/path_resolver could be unified into fewer types

3. **Keep for API stability:**
   - All serde deserialization structs - removing fields breaks JSON parsing
   - Error variants - comprehensive error types are valuable
   - Public API methods - external code may depend on them
