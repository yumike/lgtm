# lgtm review protocol

When a file `.review/session.json` exists in the repository root, there is an active code review session. Follow this protocol when asked to address review comments.

## Workflow

1. Run `lgtm fetch` — blocks until the developer clicks "Submit to agent" in the UI
2. Check `session_status` in the output. If `approved` or `abandoned`, stop — the session is over
3. If `open_threads` is empty, report "No open threads to address" and stop
4. For each open thread (grouped by file, top to bottom by line number):
   - Read the referenced file and lines (`file`, `line_start`, `line_end`)
   - Read the full comment history in `comments` to understand what the developer is asking
   - Fix the code
   - Run `lgtm reply <thread_id> "explanation of what you changed"`
5. If you discover additional issues while fixing (bugs, security problems, missing error handling), raise them:
   `lgtm thread --file <path> --line <n> --severity <level> "description"`
   Do not fix agent-raised issues unless the developer explicitly asks
6. Commit changes with `[lgtm]` prefix
7. Report summary: "Addressed N threads in M files. Raised X new observations. Please review in the lgtm UI."

## CLI commands

```bash
# Wait for developer to submit comments (blocks until ready)
lgtm fetch

# Reply to a thread after fixing the code
lgtm reply t_01J8XYZABC "Added exponential backoff with jitter. See lines 42-67."

# Raise an observation for the developer to review
lgtm thread --file src/auth.py --line 71 --severity warning "Hardcoded API key — should use environment variable"

# Check session status without blocking
lgtm status --json
```

## Commit strategy

- Independent fixes → one commit per thread (e.g., `[lgtm] fix: add retry backoff per thread t_01J8XYZABC`)
- Related changes touching the same code → batch into one commit (e.g., `[lgtm] fix: address auth service review comments`)
- Trivial fixes (typos, imports) → batch together

## Rules

- Never mark a thread as resolved or dismissed — only the developer does that
- Never edit or delete existing comments
- Keep replies concise — state what changed and where
- Only raise agent threads for genuine issues — not style preferences
- Don't fix issues you raised — unless the developer explicitly asks
- If a thread references lines that no longer exist, explain in your reply and reference the new location
- If you disagree with a comment, explain your reasoning — the developer will decide

## Error handling

- Exit code 2: no review session exists
- Exit code 6: session is not active
- `lgtm reply` exit code 4: thread not found — skip and continue with remaining threads
