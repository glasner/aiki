# Aiki Task Ledger Design
## Goal
Create an Aiki task-based source-of-truth ledger for Ash operations, replacing ad hoc markdown notes as authoritative evidence store.

## Scope
- Canonical source-of-truth: `aiki` task stream for execution events.
- Maintain a lightweight, append-only event record for coordination (`start/milestone/blocker`/`done`).
- Preserve auditability with strict evidence fields.
- Keep manual intervention minimal and consistent with existing execution contract.

## Principles
- Source-of-truth first: one stream for Ash-ledger tasks.
- Append-only evidence: never overwrite events.
- Evidence-first: every status event must carry command/run/file proof.
- Machine-auditable: include normalized metadata for filtering and reporting.
- Minimal runtime overhead.

## Proposed data model
### Primary task record
- `id`: `ledger-YYYYMMDD-HHMMSS-<slug>` (e.g., `ledger-20260309-172100-main-recovery`)
- `title`: short summary (<= 80 chars)
- `status`: open / blocked / done
- `tags`: ["ledger", "ash", "coordination"]
- `owner: ash`
- `source_session`: OpenClaw session key, e.g., `agent:main:main`
- `run_id`: optional, if available
- `evidence_paths`: array of file paths
- `run_ids`: array of run IDs (optional)
- `commit_refs`: array of commit hashes
- `created_at`: ISO timestamp
- `updated_at`: ISO timestamp
- `closed_at`: optional ISO timestamp

### Event comments
For each state transition:
- `event_type`: `start` | `milestone` | `blocker` | `done`
- `summary`: one-line update
- `owner`: `ash`
- `evidence`: object with:
  - `commands`: array of command strings
  - `artifact_paths`: array of file paths
  - `session_key`: optional session identifier
  - `run_id`: optional run id
  - `commit`: optional commit hash
  - `notes`: brief interpretation

## Proposed command flow
1. **Create a stream anchor (optional daily parent)**
   - `aiki task start "Ledger: Aiki Coordination"`  
   - Then add tags via existing CLI/task UI: `ledger`, `ash`, `coordination`.

2. **Emit lifecycle events as task comments**
   - Start:
     - `aiki task comment <task-id> "{\"event_type\":\"start\",\"summary\":\"...\",\"owner\":\"ash\",\"evidence\":{...}}"`
   - Milestone:
     - `aiki task comment <task-id> "{\"event_type\":\"milestone\",\"summary\":\"...\",\"owner\":\"ash\",\"evidence\":{...}}"`
   - Blocker:
     - `aiki task comment <task-id> "{\"event_type\":\"blocker\",\"summary\":\"...\",\"owner\":\"ash\",\"evidence\":{...},\"status\":\"blocked\"}"`
   - Done:
     - `aiki task comment <task-id> "{\"event_type\":\"done\",\"summary\":\"...\",\"owner\":\"ash\",\"evidence\":{...}}"`

3. **Close as authoritative evidence gate**
   - `aiki task close <task-id>` when fully done and validated.

4. **Query canonical state**
   - Use task filtering (`tag:ledger`, `owner:ash`) to produce today's active blockers and last done events.

> If `aiki task comment` has different argument syntax in your build, keep the same structure by putting a JSON block in the first line of each update and enforce a stable parse regex in the reader.

## Reference event template
Use this exact JSON payload at top of each event comment:

```json
{
  "event_type": "start|milestone|blocker|done",
  "source_session": "agent:main:main",
  "run_id": "run_id_here",
  "evidence": {
    "commands": ["openclaw status --all", "aiki task ..."],
    "artifact_paths": ["/Users/tu/.openclaw/workspace/memory/2026-03-09.md", "/tmp/aiki-gs-latest-verify-.../report.log"],
    "session_key": "agent:ash:main",
    "commit": "fadc257",
    "notes": "One-line outcome and rationale"
  }
}
```

Then append human sentence after JSON block for readability.

## Reporting and readback policy
- **Source-of-truth queries**: always read `aiki task` entries tagged `ledger` and `ash` first.
- **Mirror**: `memory/2026-03-09.md` remains a convenience summary only.
- **Retention**: keep tasks closed (done) as immutable historical records.
- **Escalation rule**: any `blocker` event sets task status to blocked until explicitly superseded by a follow-up `done` event on that blocker.

## Migration plan
1. Define and publish this schema in `MEMORY_PROTOCOL.md`.
2. Add lightweight helper in `agents/ash/` for writing canonical events.
3. Start dual-write from Ash handoffs for 1 day (`aiki task + memory note`).
4. After validation, switch memory notes to passive mirror only.
5. Add a weekly smoke check: `aiki task list --tag ledger --status open`.

## Milestone estimate
- **Design complete:** now
- **Pilot with dual-write:** 1 day
- **Cutover to task-only source-of-truth:** next day after pilot sign-off
