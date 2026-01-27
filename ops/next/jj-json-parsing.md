Here’s a clean one-pager in Markdown you can drop straight into the repo or a design doc.

⸻

Using JJ JSON Templates for Stable, Native Session & Event Queries

Summary

To robustly query session and turn activity from Jujutsu (jj), Aiki will use jj’s native template language with the json() function to emit machine-parseable JSON, instead of scraping human-formatted output or parsing raw descriptions.

This approach gives us deterministic parsing, forward compatibility, and native jj semantics without requiring a fork or direct dependency on jj-lib.

⸻

Problem

We need to answer questions like:
	•	“What was the most recent event for this session?”
	•	“Has this session seen activity within the TTL window?”
	•	“What turn number should resume next?”

Naive approaches are fragile:
	•	Parsing default jj log output is brittle (format, locale, version drift)
	•	Regex-scraping description() text is error-prone
	•	Relying on timestamps embedded in filenames or session state diverges from jj’s source of truth

⸻

Core Idea

Use jj’s native template language with the built-in json() function to emit explicit JSON objects representing exactly the fields Aiki needs.

Instead of:

jj log ... | grep | awk | regex

We do:

jj log ... -T 'json({ ts: author.timestamp, change_id: change_id }) ++ "\n"'

This produces one JSON object per line, suitable for direct parsing.

⸻

Why This Works Well

Native to jj
	•	json() is part of jj’s template language
	•	No plugins, no forks, no external parsers

Stable & Explicit
	•	We define the schema ourselves
	•	No dependency on jj’s human output format
	•	Minimal surface area for breakage

Matches Aiki’s Model
	•	jj remains the source of truth
	•	Session/turn logic derives from event history
	•	Cleanup and resume logic is deterministic

⸻

Recommended Pattern

1. Encode session metadata explicitly in descriptions

When Aiki writes events to aiki/conversations, include a unique marker line:

aiki.session_id=<uuid>
aiki.turn=3

This allows reliable filtering without substring ambiguity.

⸻

2. Query latest activity using a JSON template

Example: query the most recent event for a session

jj log --no-graph \
  -r 'aiki/conversations & description("aiki.session_id=<uuid>")' \
  --limit 1 \
  -T 'json({
        ts: author.timestamp,
        change_id: change_id
      }) ++ "\n"'

Possible outcomes:
	•	1 JSON line → latest event found
	•	No output → no events exist
	•	Command error → jj unavailable / transient failure

These map cleanly to application logic.

⸻

3. Parse line-delimited JSON

Aiki treats jj output as:

<json>\n
<json>\n
...

Each line is parsed independently:
	•	Simple streaming
	•	Easy to unit test
	•	No array framing needed

⸻

What Fields to Use

Recommended minimal schema:

{
  "ts": "2026-01-21T18:42:11Z",
  "change_id": "kvusmpqw"
}

Notes:
	•	Use author.timestamp for “event happened at” semantics
	•	Avoid json(self) in production (too much, less stable)
	•	Keep schemas small and intentional

⸻

What This Is Not
	•	❌ Not a plugin or extension system
	•	❌ Not custom Rust parsers inside jj
	•	❌ Not dependent on jj’s default formatting
	•	❌ Not a replacement for jj-lib (but compatible with future migration)

⸻

Alternatives Considered

Parsing human output

Rejected: brittle, version-dependent, error-prone.

Using jj-lib directly

Viable long-term, but:
	•	Tighter coupling to jj internals
	•	More complexity up front
	•	Harder to ship quickly

JSON templates give us 80% of the benefit with 5% of the cost.

⸻

Recommendation

Adopt --template + json() as the canonical interface between Aiki and jj.
	•	One helper function owns all jj invocations
	•	Templates define a stable contract
	•	Future migration to jj-lib remains possible without redesigning semantics

This keeps Aiki:
	•	correct
	•	debuggable
	•	native to jj
	•	operationally boring (the good kind)

⸻
