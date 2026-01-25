# Individual Agent Response Tracking (Future)

**Status:** Deferred - not currently planned for implementation

## Concept

Track individual agent messages at a finer granularity than turn completion. This would enable message-level observability during multi-message turns.

## Current State

The event system tracks **turn completion** via `turn.completed` event:
- Fires when agent loop ends (`Stop`/`stop` hooks)
- Captures the final state after all messages in a turn

## Proposal

Add `response.received` event that fires for each individual message:
- **Cursor:** Map `afterAgentResponse` hook → `response.received`
- **ACP agents:** Map `agent_message_chunk` notifications → `response.received`
- **Claude Code hooks:** No equivalent (only `Stop` at turn end)

### Payload Structure

```rust
pub struct AikiResponseReceivedPayload {
    pub session: AikiSession,
    pub cwd: PathBuf,
    pub timestamp: DateTime<Utc>,
    /// Individual message text (mid-turn)
    pub response: String,
    /// Files modified in this message
    pub modified_files: Vec<PathBuf>,
}
```

### Flow Usage Example

```yaml
# Message-level (fires on each message, Cursor/ACP only)
response.received:
  - action: shell
    command: "stream-to-ui.sh '{{$event.response}}'"

# Turn-level (fires on loop end, all agents)
turn.completed:
  - action: shell
    command: "finalize-ui.sh '{{$event.response}}'"
  - action: shell
    command: "aiki review --auto"
```

## Use Cases

- **Real-time UI updates:** Stream messages to UI as they're generated
- **Message-level auditing:** Track each message for compliance/review
- **Incremental processing:** Process messages as they arrive, not just at turn end
- **Streaming observability:** Monitor agent reasoning in real-time

## Challenges

### Agent Support Disparity

| Agent | Integration | Message-Level Event |
|-------|-------------|---------------------|
| Cursor | Hooks | ✅ `afterAgentResponse` |
| Claude Code | Hooks | ❌ None (only `Stop` at turn end) |
| Claude Code | ACP | ✅ `agent_message_chunk` |
| ACP agents | ACP | ✅ `agent_message_chunk` |

**Problem:** Claude Code hook users would never fire `response.received` events, creating inconsistent behavior across integration methods.

### Implementation Complexity

- Need to handle both streaming (ACP) and discrete (Cursor) message events
- Flows would need to account for events that may or may not fire depending on agent
- Documentation complexity: "This event only works for X and Y, not Z"

### Limited Demand

- Most use cases satisfied by `turn.completed`
- Streaming UI updates can be handled outside aiki flows
- No clear user demand for message-level granularity in flows

## Decision

**Deferred indefinitely** - not implementing unless users specifically request this functionality.

## Alternative Approaches

If message-level tracking becomes necessary:

1. **External tooling:** Build separate tools that consume hook output directly
2. **ACP-only feature:** Only expose for ACP agents where it's consistent
3. **Agent-specific flows:** Allow Cursor-specific flows that have access to message events

## Related Documentation

- `ops/now/session-end-policy.md` - Removed Phase 4 (this concept)
- `ops/now/turn-completed-event-design.md` - Event model without message-level tracking
- `ops/now/turn-completed-ux-comparison.md` - UX comparison focused on turn completion
