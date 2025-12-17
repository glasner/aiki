
# File Operation Refactor review
1. **ACP vendor still emits only write events**  
   `fire_pre_file_change_event()` and `record_post_change_events()` in `cli/src/commands/acp.rs` always build `AikiEvent::WritePermissionAsked`/`WriteCompleted` regardless of the tool kind. Delete/Move operations therefore never surface `delete.*` events for ACP sessions, so flows cannot gate or record delete provenance for that vendor.

3. **Shell delete detection drops dash-prefixed paths**  
   `parse_file_operation_from_shell_command()` filters out every argument that starts with `-` (`cli/src/tools.rs`), so commands like `rm -- -important` end up with an empty `file_paths` list and never trigger `delete.*` events. That bypasses delete gating/provenance for files whose names begin with a dash.
