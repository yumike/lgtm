# Agent Fetch Loop Design

## Goal

Add a blocking `lgtm fetch` command that lets any coding agent wait for the developer to submit review comments from the web UI, then return the open threads as JSON. This creates an agent-agnostic review loop — no MCP, no channels, no SDK required. Any agent that can shell out to `lgtm fetch` and `lgtm reply` can participate.

## Motivation

The developer reviews code in the lgtm web UI and leaves comments on specific lines. At some point they want the agent to pick up those comments, fix the code, and reply. The agent needs to:

1. **Wait** for the developer to finish commenting (not poll)
2. **Receive** all open threads in a structured format
3. **Act** on them (fix code, reply)
4. **Wait again** for the next batch

`lgtm fetch` is the "wait for your turn" primitive. It blocks until the developer clicks "Submit to agent" in the UI, then returns the open threads and exits.

This design replaces the "Ready for agent" button and `.review/ready` marker file described in the main spec (sections 3 and 4). Those references should be updated to reflect this design.

## Design

### The `lgtm fetch` command

```
lgtm fetch [--timeout <seconds>]
```

**Behavior:**

1. If `.review/` directory or `session.json` does not exist, exit code 2.
2. If session status is not `in_progress`, exit code 6.
3. Check for `.review/.submit` marker file.
4. If marker exists: read `session.json`, print open threads as JSON to stdout, delete marker, exit 0.
5. If marker does not exist: watch `.review/` directory via `notify` for `.submit` creation, and also watch `session.json` for status changes. Block until either event occurs.
6. If `.submit` appears, proceed as in step 4.
7. If the session transitions to `approved` or `abandoned` while waiting, unblock and return the terminal status with empty `open_threads`.
8. If `--timeout` is specified and the timeout elapses before either event, exit 0 with `"timed_out": true`, `"session_status": "in_progress"`, and empty `"open_threads": []`. Without `--timeout`, blocks indefinitely.

**Reading session.json:** `lgtm fetch` reads session.json without acquiring the advisory lock. This is safe because all writers use atomic writes (write to `.tmp`, then rename), so readers always see a complete file. This is the same approach used by `lgtm status`.

**Concurrency:** Only one `lgtm fetch` process should run at a time. If multiple agents call `lgtm fetch` concurrently, the first one to detect and delete the marker wins; the others continue blocking. This is acceptable — running multiple agents against the same review session is not a supported use case.

**Signal handling:** If `lgtm fetch` receives SIGINT/SIGTERM while blocking, it exits without deleting the marker. The marker persists and the next `lgtm fetch` call will pick it up.

**The server does not need to be running** for `lgtm fetch` to work. The developer can also create `.review/.submit` manually (e.g., `touch .review/.submit`). The server is just a convenient way to create the marker from the UI.

**Output format (stdout):**

```json
{
  "session_status": "in_progress",
  "base": "main",
  "head": "feature/foo",
  "merge_base": "abc1234",
  "open_threads": [
    {
      "id": "t_01J8XYZABC",
      "origin": "developer",
      "severity": null,
      "status": "open",
      "file": "src/auth.py",
      "line_start": 42,
      "line_end": 48,
      "diff_side": "right",
      "anchor_context": "def retry_request(self, url, max_retries=10):",
      "comments": [
        {
          "id": "c_01J8XYZ001",
          "author": "developer",
          "body": "This retry logic has no backoff.",
          "timestamp": "2026-03-18T14:22:00Z"
        }
      ]
    }
  ]
}
```

The `timed_out` field is only present (and `true`) when the timeout elapses. This lets agents distinguish "developer submitted with no open threads" from "timed out waiting."

**Errors go to stderr.** JSON output only appears on stdout on success (exit 0). On error exits (2, 6), a human-readable message is written to stderr.

**Differences from `lgtm status --json`:** `lgtm fetch` includes `merge_base` (useful for the agent to understand the diff range) and omits `stats` (the agent has the full thread list and can compute its own). The `timed_out` field is unique to `lgtm fetch`.

**Exit codes:**

| Code | Meaning |
|------|---------|
| 0 | Success (threads returned, or timeout elapsed, or session ended while waiting) |
| 2 | Session not found |
| 6 | Session not active at startup |

### The submit marker file

When the developer clicks "Submit to agent" in the web UI, the server creates:

```
.review/.submit
```

**Why a marker file instead of a field in session.json:**

- `session.json` has concurrent readers/writers with advisory locking. Adding a signaling field mixes data and coordination concerns.
- A marker file is atomic to create (`O_CREAT | O_EXCL`) and delete. No locking needed.
- `lgtm fetch` can watch for file creation via `notify` (already a workspace dependency) without parsing JSON.

**Lifecycle:**

1. Developer clicks "Submit to agent" in UI.
2. Server creates `.review/.submit` via `POST /api/submit`.
3. `lgtm fetch` detects the marker, reads `session.json`, prints threads, deletes `.submit`, exits.
4. UI detects `.submit` disappearance (via WebSocket) and re-enables the button.

**Comments added after clicking "Submit"** are included — `lgtm fetch` reads session.json at pickup time, not at submit time. This is intentional: the developer can continue adding comments right up until the agent picks them up.

**Stale markers:** `lgtm start` deletes any existing `.review/.submit` marker from a previous session to prevent `lgtm fetch` from immediately returning stale data.

### Web server changes

**New routes:**

`POST /api/submit` — creates `.review/.submit` marker file. Empty request body. Returns 201 on success, 409 if marker already exists (agent hasn't picked up yet).

`GET /api/submit` — returns `{"pending": true|false}` indicating whether a submit is waiting for the agent.

**WebSocket updates:**

The server already watches the `.review/` directory. When `.submit` is created or deleted, push a WebSocket event:

```json
{"type": "submit_status", "pending": true}
```

This follows the existing `WsMessage` pattern with `#[serde(tag = "type", content = "data")]` serialization.

### Web UI changes

Replace the "Ready for agent" concept with a **"Submit to agent"** button in the status bar.

**Button states:**

| State | Condition | Appearance |
|-------|-----------|------------|
| Enabled | No `.submit` marker, at least one open thread | "Submit to agent" button |
| Disabled, waiting | `.submit` marker exists | "Waiting for agent..." (grayed out) |
| Disabled, no threads | No open threads | "Submit to agent" (grayed out) |

The button transitions are driven by WebSocket `submit_status` events. No polling.

### Agent workflow loop

From any coding agent's perspective:

```
while true:
    result = run("lgtm fetch")
    if result.session_status != "in_progress":
        break
    if result.open_threads is empty:
        break
    if result.timed_out:
        continue

    for thread in result.open_threads:
        read the file and lines referenced
        understand the comment conversation
        fix the code
        run("lgtm reply <thread.id> 'what I changed'")

    optionally create new observations:
        run("lgtm thread --file ... --line ... --severity warning 'issue'")
```

No SDK, no protocol — just CLI commands. Works with Claude Code, Codex, Gemini CLI, Aider, or a shell script.

Commit strategy stays with the agent. lgtm does not dictate when to commit.

### Agent integration

Agents learn the `lgtm fetch` protocol through a skill file (CLAUDE.md or equivalent) committed to the repo. The skill file replaces the direct session.json manipulation described in the main spec's section 4. Agents should use CLI commands exclusively (`lgtm fetch`, `lgtm reply`, `lgtm thread`, `lgtm status`) rather than reading or writing session.json directly.

### What does NOT change

- `lgtm reply` and `lgtm thread` commands — unchanged
- `lgtm status` — unchanged
- `session.json` schema — no new fields
- WebSocket diff/session updates — unchanged
- File watching for code changes — unchanged
- Advisory locking — unchanged

### Changes to `lgtm start`

- Delete any existing `.review/.submit` marker on startup (prevents stale markers)
- No other changes to `lgtm start`

## Exit codes (consolidated)

| Code | Meaning |
|------|---------|
| 0 | Success |
| 1 | General error |
| 2 | Session not found |
| 4 | Thread not found |
| 5 | File not found or line out of range |
| 6 | Session not active |
