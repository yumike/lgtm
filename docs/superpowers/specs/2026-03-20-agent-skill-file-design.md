# Agent Skill File Design

## Goal

Create a `CLAUDE.md` file in the repo root that teaches Claude Code how to participate in the lgtm review loop using CLI commands. After stabilization, this will move to a dedicated plugin.

This replaces the direct session.json manipulation described in the main spec's section 4. Agents should use CLI commands exclusively (`lgtm fetch`, `lgtm reply`, `lgtm thread`, `lgtm status`).

## Placement

`CLAUDE.md` in the repo root. Claude Code reads this automatically.

## Content

The file teaches a **single-pass** workflow: the developer asks the agent to address review comments, the agent runs `lgtm fetch` (blocks indefinitely, no `--timeout`), fixes all open threads, replies, commits, and reports a summary. The developer then reviews in the UI and can ask again if needed.

### Protocol

1. Run `lgtm fetch` — blocks until the developer clicks "Submit to agent" in the UI. Returns JSON to stdout.
2. Check `session_status` in the output. If `approved` or `abandoned`, stop — the session is over. Tell the developer.
3. If `open_threads` is empty, report "No open threads to address" and stop.
4. For each open thread (process grouped by file, top to bottom by line number):
   a. Read the referenced file and lines (`file`, `line_start`, `line_end`)
   b. Read the full comment history in `comments` to understand what the developer is asking
   c. Fix the code
   d. Run `lgtm reply <thread_id> "explanation of what you changed"`
5. If the agent discovers additional issues while fixing (bugs, security problems, missing error handling), raise them:
   `lgtm thread --file <path> --line <n> --severity <level> "description"`
   Do not fix agent-raised issues unless the developer explicitly asks. Flag them for the developer to decide.
6. Commit changes with `[lgtm]` prefix.
7. Report a summary.

### Example CLI invocations

```bash
# Wait for developer to submit comments (blocks)
lgtm fetch

# Reply to a thread
lgtm reply t_01J8XYZABC "Added exponential backoff with jitter. See lines 42-67."

# Raise an observation
lgtm thread --file src/auth.py --line 71 --severity warning "Hardcoded API key — should use environment variable"

# Check session status without blocking
lgtm status --json
```

### Key fields in `lgtm fetch` output

```json
{
  "session_status": "in_progress",
  "base": "main",
  "head": "feature/foo",
  "merge_base": "abc1234",
  "open_threads": [
    {
      "id": "t_01J8XYZABC",
      "file": "src/auth.py",
      "line_start": 42,
      "line_end": 48,
      "anchor_context": "def retry_request(self, url, max_retries=10):",
      "comments": [
        {
          "author": "developer",
          "body": "This retry logic has no backoff."
        }
      ]
    }
  ]
}
```

### Commit strategy

- Independent fixes → one commit per thread (e.g., `[lgtm] fix: add retry backoff per thread t_01J8XYZABC`)
- Related changes touching the same code → batch into one commit (e.g., `[lgtm] fix: address auth service review comments`)
- Trivial fixes (typos, imports) → batch together
- Prefix all commit messages with `[lgtm]`

### Rules

- Never mark a thread as resolved or dismissed — only the developer does that
- Never edit or delete existing comments
- Keep replies concise — state what changed and where
- Only raise agent threads for genuine issues — not style preferences or trivial observations
- Don't fix issues you raised — unless the developer explicitly asks
- If a thread references lines that no longer exist, explain in your reply that the code has moved and reference the new location
- If you disagree with a comment, explain your reasoning — the developer will decide

### Example reply

Good: "Added exponential backoff with jitter (base 1s, max 30s). See lines 42-67."

Bad: "I have carefully reviewed your comment and I agree that the retry logic needs improvement. After thorough analysis, I have implemented an exponential backoff strategy with jitter to prevent thundering herd problems..."

### Error handling

- `lgtm fetch` exit code 2: no review session exists — tell the developer
- `lgtm fetch` exit code 6: session is not active at startup — tell the developer
- `lgtm fetch` returns `session_status` of `approved` or `abandoned`: session is over, stop
- `lgtm reply` exit code 4: thread not found (may have been deleted) — skip and continue with remaining threads

### Summary format

After addressing all threads, report:

> Addressed N threads in M files. Raised X new observations. Please review in the lgtm UI.

### Diagnostic command

`lgtm status --json` can be used to check session state without blocking. Useful for debugging, but not needed in the standard workflow.
