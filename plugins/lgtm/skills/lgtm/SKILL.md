---
name: lgtm
description: Address review comments from the lgtm UI. Use when asked to handle code review feedback.
disable-model-invocation: true
---

# lgtm review protocol

When invoked, enter a review loop that runs until the session is approved or abandoned.

## Workflow

Loop:

1. Run `lgtm fetch` — blocks until the developer clicks "Submit to agent" in the UI
2. Check `session_status` in the output. If `approved` or `abandoned`, report and stop
3. If `open_threads` is empty, report "No open threads to address" and go to step 1
4. For each open thread (grouped by file, top to bottom by line number):
   - Read the referenced file and lines (`file`, `line_start`, `line_end`)
   - Read the full comment history in `comments` to understand what the developer is asking
   - Fix the code
   - Run `lgtm reply <thread_id> "explanation of what you changed"`
5. If you discover additional issues while fixing (bugs, security problems, missing error handling), raise them:
   `lgtm thread --file <path> --line <n> --severity <level> "description"`
   Do not fix agent-raised issues unless the developer explicitly asks
6. Commit changes
7. Report summary: "Addressed N threads in M files. Raised X new observations. Please review in the lgtm UI."
8. Go to step 1

## CLI commands

```bash
# Start a review session (launches lgtm app if not running)
lgtm start --base main

# Wait for developer to submit comments (blocks until ready)
lgtm fetch

# Reply to a thread after fixing the code
lgtm reply t_01J8XYZABC "Added exponential backoff with jitter. See lines 42-67."

# Raise an observation for the developer to review
lgtm thread --file src/auth.py --line 71 --severity warning "Hardcoded API key — should use environment variable"

# Check session status without blocking
lgtm status --json

# Approve the session (all threads must be resolved)
lgtm approve

# Abandon the session
lgtm abandon

# Remove a session
lgtm clean
```

## Architecture

The lgtm app is a persistent Tauri desktop application with multiple tabs (one per review session). The CLI is a thin HTTP client that communicates with the app via its local API. No per-session servers — one app handles all reviews.

## Rules

- Never mark a thread as resolved or dismissed — only the developer does that
- Never edit or delete existing comments
- Keep replies concise — state what changed and where
- Only raise agent threads for genuine issues — not style preferences
- Don't fix issues you raised — unless the developer explicitly asks
- If a thread references lines that no longer exist, explain in your reply and reference the new location
- If you disagree with a comment, explain your reasoning — the developer will decide

## Error handling

- If the lgtm app is not running, `lgtm start` will launch it automatically
- Connection errors mean the app is not running — run `lgtm start` first
- `lgtm reply` with an invalid thread ID: skip and continue with remaining threads
