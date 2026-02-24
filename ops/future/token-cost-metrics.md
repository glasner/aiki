# Token & Cost Tracking for Aiki Metrics

> **Status**: Future work - deferred from main metrics implementation
> **Blocked by**: Editor API limitations (neither Claude Code nor Cursor expose token usage in hooks)

## Problem

Aiki's metrics system needs token usage and cost data to answer questions like:
- Why did Task A cost 90K tokens when Task B cost 12K?
- What's the ROI of using different models for different phases?
- Are we spending more on cache misses over time?

Currently, aiki does not capture token counts or costs.

## Current Limitation: Editors Don't Expose Usage Data

**Neither Claude Code nor Cursor expose token usage information in their hook payloads.** Investigation of both editors' hook APIs confirms:

- **Claude Code**: No hook event includes `input_tokens`, `output_tokens`, or cost fields (verified via official docs)
- **Cursor**: No hook event includes token usage metrics (verified via official docs; feature request exists but not implemented)
- **Cursor preCompact hook**: Provides `context_tokens` and `context_window_size` but these are about context size, not API usage

This means direct hook-based capture is **not currently feasible** without upstream changes to editor APIs.

## Potential Approaches

### Option 1: Transcript Parsing (Most Viable)

Parse the conversation transcript JSONL file to extract usage data. Claude Code's `Stop` hook provides a `transcript_path` that contains the full conversation including API responses with token usage.

```rust
// In AikiTurnCompletedPayload (new optional fields)
pub struct AikiTurnCompletedPayload {
    // ... existing fields ...

    /// Token usage for this turn (parsed from transcript)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub usage: Option<TurnUsage>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TurnUsage {
    /// Input tokens consumed
    pub input_tokens: Option<u64>,
    /// Output tokens generated
    pub output_tokens: Option<u64>,
    /// Cache creation tokens
    pub cache_creation_input_tokens: Option<u64>,
    /// Cache read tokens
    pub cache_read_input_tokens: Option<u64>,
    /// Model used for this turn
    pub model: Option<String>,
    /// Estimated cost in USD (computed from model pricing)
    pub cost_usd: Option<f64>,
}
```

**Implementation**: On `Stop` hook, parse `transcript_path` JSONL file, extract usage from Claude API responses, and emit `turn.completed` event with usage data.

**Pros**:
- Accurate (comes from API responses)
- Works for Claude Code today
- No agent training needed

**Cons**:
- Requires JSONL parsing
- Only works for editors that expose transcript paths
- Transcript format may change

### Option 2: Agent Self-Report (Fallback)

Agents report their own estimated token usage when closing tasks:

```bash
aiki task close <id> --summary "Done" \
  --data estimated_input_tokens=45000 \
  --data estimated_output_tokens=8200
```

**Pros**:
- Works for any editor
- Simple implementation (already supported via task data fields)

**Cons**:
- Less accurate (estimates only)
- Requires agent training
- Agents may not have access to real numbers

### Option 3: Advocate for Upstream Changes

File feature requests with Claude Code and Cursor to add token usage fields to their hook payloads (particularly `Stop`/`stop` hooks).

**Ideal payload for Stop hook**:
```json
{
  "hook_event_name": "Stop",
  "session_id": "...",
  "transcript_path": "...",
  "usage": {
    "total_input_tokens": 45000,
    "total_output_tokens": 8200,
    "total_cache_creation_input_tokens": 5000,
    "total_cache_read_input_tokens": 12000,
    "models_used": ["claude-sonnet-4-5"]
  }
}
```

**Pros**:
- Most straightforward implementation
- Official support means stability
- All editors benefit

**Cons**:
- Depends on upstream changes
- Timeline uncertain
- May never happen

## Recommendation

**Phase 1 (When Needed)**: Implement Option 1 (transcript parsing) for Claude Code
- Provides accurate data today
- Can be implemented entirely in aiki
- Sufficient for initial metrics rollout

**Phase 2 (Later)**: Add Option 2 (self-report) for other editors
- Fallback for editors without transcript access
- Optional, not required

**Parallel Track**: File feature requests with Claude Code and Cursor (Option 3)
- Advocate for `usage` field in Stop/stop hooks
- If accepted, can deprecate transcript parsing

## Integration with Metrics Architecture

Once token/cost data is available, integrate into existing metrics tiers:

### Tier 2: Execution Tracking

Add token/cost fields to `MetricsEvent::TaskMetrics`:

```rust
pub enum MetricsEvent {
    TaskMetrics {
        // ... existing fields ...
        
        /// Total input tokens across all turns
        total_input_tokens: Option<u64>,
        /// Total output tokens across all turns
        total_output_tokens: Option<u64>,
        /// Total cache creation tokens
        total_cache_creation_input_tokens: Option<u64>,
        /// Total cache read tokens
        total_cache_read_input_tokens: Option<u64>,
        /// Total estimated cost in USD
        total_cost_usd: Option<f64>,
        /// Models used
        models: Vec<String>,
    },
}
```

### Tier 3: Learning Signals

Add cost-based signals to `aiki metrics` output:

- **Cost per complexity level** - Average cost for simple/medium/complex/very_complex tasks
- **Model efficiency** - Compare cost/quality tradeoffs across models
- **Cache hit rate** - Ratio of cache_read to total input tokens
- **Cost prediction** - Estimate task cost based on complexity + similar tasks
- **Cost trends** - Track cost changes over time (improving or degrading?)

### CLI Output Updates

```bash
# In aiki metrics <task-id>
Tokens: 90,200 input / 12,400 output
Cache: 5,000 created / 15,000 read (hit rate: 62%)
Cost: $0.38

# In aiki metrics summary
Cost:
  Total: $41.20
  Average: $2.06/task
  Range: $0.45 — $2.80
  By complexity:
    simple: $0.52 avg
    medium: $1.80 avg
    complex: $4.20 avg
  Cache hit rate: 58% avg
  
Cost Trends (10-task rolling):
  Cost: $2.40 → $1.85 (↓23%)
  Cache hits: 45% → 62% (↑38%)
```

## Open Questions

1. **Transcript format stability** - How stable is Claude Code's JSONL transcript format? Need to handle schema changes gracefully.

2. **Model pricing** - Where should model pricing data live? Hardcoded, config file, or fetched from Anthropic API?

3. **Cost calculation** - Should we compute cost from tokens + pricing, or expect editors to provide it?

4. **Historical data** - Can we backfill token/cost data for existing tasks by parsing their transcript files?

5. **Multi-editor normalization** - Different editors may structure usage data differently. Need adapter pattern?

## References

- Claude Code hooks docs: https://code.claude.com/docs/en/hooks
- Cursor hooks docs: https://cursor.com/docs/agent/hooks
- Cursor feature request for token usage: https://forum.cursor.com/t/cursor-hooks-token-usage-support/147216
- Community tool (ccusage): https://github.com/ryoppippi/ccusage
