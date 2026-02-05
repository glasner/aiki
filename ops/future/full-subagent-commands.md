# Full Subagent Command Surface

These commands would enhance subagent lifecycle management beyond the MVP (run/stop).

## Additional Lifecycle Commands

```bash
# List running/completed subagents
aiki subagent list
aiki subagent list --status running
aiki subagent list --status completed

# Check status of a specific subagent
aiki subagent status <subagent-id>

# View subagent logs
aiki subagent logs <subagent-id>
aiki subagent logs <subagent-id> --follow

# Attach to a running subagent (interactive)
aiki subagent attach <subagent-id>
```

## Use Cases

**`list`** - Overview of all subagents across sessions
- See what's running in the background
- Filter by status (running, completed, failed)
- Quick audit of autonomous work

**`status`** - Detailed information about a specific subagent
- Current state (running, waiting, completed)
- Task it's working on
- Agent type being used
- Start time, duration
- Last activity/output

**`logs`** - Stream or view subagent output
- Debug what the subagent is doing
- Follow along with long-running work
- Review completed subagent output

**`attach`** - Take over interactive control
- Convert background subagent to interactive session
- Useful if subagent gets stuck or needs guidance
- Hand back control when done

## Implementation Considerations

- **Persistence**: Subagent metadata needs to persist across Aiki restarts
- **Log storage**: Where do logs live? JJ branch? Separate log files?
- **Attach mechanism**: How to connect to running agent session? Might require agent-specific support
- **ID generation**: Subagent IDs separate from task IDs (one task → multiple subagent attempts)

## See Also

- `ops/now/subagents.md` - Core MVP specification
