# Unified Event Model Review

## Findings

1. **`file.permission_asked` cannot gate operations**  
   `handle_file_permission_asked` ignores the `FlowEngine` result and always returns `Decision::Allow` (`cli/src/events/file_permission_asked.rs:35-66`), so policies described in the plan (ops/current/plan.md:288-297) can never block writes/deletes.

2. **`session.resumed` event never emitted**  
   Vendors only emit `session.started`; neither Claude nor Cursor ever construct `AikiEvent::SessionResumed`, leaving flows blind to resumed sessions despite the plan’s event catalog (ops/current/plan.md:22-27).

3. **MCP payloads lack `server` metadata**  
   `AikiMcpPermissionAskedPayload` / `AikiMcpCompletedPayload` expose only `tool_name`, forcing flows to parse server names from strings, contrary to the payload schema in the plan (ops/current/plan.md:145-154).

4. **Unknown Claude tools silently mapped to MCP**  
   `ClaudeTool::parse` routes any unrecognized `tool_name` to `ClaudeTool::Mcp` without warning (`cli/src/vendors/claude_code/tools.rs:168-213`), conflicting with the plan’s requirement to warn and skip so new native tools don’t hide (ops/current/plan.md:159-211).

5. **Benchmark still reports deprecated `change.*` events**  
   `cli/src/commands/benchmark.rs` continues to record and print `change.permission_asked` / `change.done` timings (`cli/src/commands/benchmark.rs:45-66`, `cli/src/commands/benchmark.rs:1019-1035`), so the new `file.*` events aren’t measured during performance regressions (ops/current/plan.md:262-269).
