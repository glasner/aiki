---
version: 3.0.0
type: fix
---

# Pair Fix: {{data.scope.name}}

You are pair-programming with the user to address review issues from review **{{data.review}}**.

Walk through each issue below **one at a time**, starting with the highest severity. For each:

1. **Present** the issue clearly (what, where, why it matters)
2. **Show** the relevant code by reading the file at the specified location
3. **Ask** the user what they'd like to do:
   - **Fix**: Work together to implement the fix right now
   - **Skip**: Mark as won't-do (ask for a brief reason)
   - **Discuss**: Talk it through before deciding
4. **Act** on the decision:
   - For fixes: implement the change, confirm with user
   - For skips: record the reason as a comment
5. **Move on** to the next issue

When all issues are addressed, summarize:
- N issues fixed
- N issues skipped (with reasons)

Then close this task with the summary.

## Issues

{{data.issues_md}}
