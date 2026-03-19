# Agent-Facing CLI Commands Design

## Context

The lgtm tool has a web UI for developers to review code and leave comments, but the AI agent (Claude Code) currently must hand-edit `.review/session.json` to participate in the review loop. This is error-prone — the agent must construct valid JSON, generate ULIDs, manage timestamps, handle atomic writes, and avoid race conditions.

CLI commands give the agent a safe, permission-friendly interface. Claude Code's permission model is command-based, so `lgtm reply` can be pre-authorized while arbitrary file writes cannot.

## Prerequisites

The following changes to existing types are required before implementing these commands:

1. **Add `Stats` struct to `Session`** — the main spec defines a `stats` object in the session JSON, but the Rust `Session` struct does not include it. Add a `Stats` struct with fields: `total_threads`, `open`, `resolved`, `wontfix`, `dismissed`, `agent_initiated`. Serialized with `skip_serializing_if` to remain backward compatible.

2. **Add `diff_snapshot` to `Comment`** — the main spec shows `diff_snapshot` on agent comments. Add `diff_snapshot: Option<String>` to the `Comment` struct with `skip_serializing_if = "Option::is_none"`. Format: a single commit hash (the current HEAD when the comment was written), not a range.

## Design Principles

- **Standalone** — all commands operate directly on `.review/session.json`, no running server required. The server's file watcher picks up changes automatically.
- **Agent-first** — output is machine-parseable (JSON), exit codes are specific, error messages are actionable.
- **Safe writes** — atomic file operations (tmp + rename) with advisory locking to avoid corruption when the server is also running.

## Commands

### `lgtm reply <thread-id> <body>`

Append a comment to an existing thread.

**Arguments:**
- `<thread-id>` — required, the thread ULID (e.g., `t_01J8XYZABC`)
- `<body>` — required, the comment text. Can also be read from stdin with `--stdin`.
- `--stdin` — optional, read body from stdin instead of positional argument. Useful for multi-line or quote-heavy text.

**Behavior:**
1. Acquire advisory lock
2. Read `session.json`
3. Verify session status is `in_progress` (exit 6 if not)
4. Find thread by ID
5. Append comment:
   ```json
   {
     "id": "<generated ULID>",
     "author": "agent",
     "body": "<body>",
     "timestamp": "<now ISO 8601>",
     "diff_snapshot": "<current HEAD commit hash>"
   }
   ```
6. Update `session.updated_at`
7. Recompute `session.stats`
8. Write atomically (tmp + rename)
9. Release lock

**Output:** Nothing on success (exit 0).

**Exit codes:**
| Code | Meaning |
|------|---------|
| 0 | Success |
| 1 | General error (I/O, JSON parse, lock timeout) |
| 2 | No session found (`.review/session.json` doesn't exist) |
| 4 | Thread not found |
| 6 | Session not active (status is `approved` or `abandoned`) |

### `lgtm thread --file <path> --line <start> [--line-end <end>] --severity <critical|warning|info> <body>`

Create a new agent-initiated thread.

**Arguments:**
- `--file <path>` — required, relative path from repo root
- `--line <start>` — required, 1-indexed line number in the new file
- `--line-end <end>` — optional, defaults to `--line` (for single-line threads)
- `--severity <level>` — required, one of `critical`, `warning`, `info`
- `<body>` — required, the observation/issue description. Can also be read from stdin with `--stdin`.
- `--stdin` — optional, read body from stdin instead of positional argument.

**Behavior:**
1. Acquire advisory lock
2. Read `session.json`
3. Verify session status is `in_progress` (exit 6 if not)
4. Read the target file at `--line` to populate `anchor_context`
5. Create thread:
   ```json
   {
     "id": "<generated ULID>",
     "origin": "agent",
     "severity": "<severity>",
     "status": "open",
     "file": "<file>",
     "line_start": <start>,
     "line_end": <end>,
     "diff_side": "right",
     "anchor_context": "<content of line_start from actual file>",
     "comments": [
       {
         "id": "<generated ULID>",
         "author": "agent",
         "body": "<body>",
         "timestamp": "<now ISO 8601>",
         "diff_snapshot": "<current HEAD commit hash>"
       }
     ]
   }
   ```
6. Append thread to `session.threads`
7. Update `session.updated_at`
8. Recompute `session.stats`
9. Write atomically (tmp + rename)
10. Release lock

**Output:** Prints the new thread ID to stdout (e.g., `t_01J8XYZ999`) so the agent can reference it later.

**Exit codes:**
| Code | Meaning |
|------|---------|
| 0 | Success |
| 1 | General error |
| 2 | No session found |
| 5 | File not found or line out of range |
| 6 | Session not active |

**Note:** `diff_side` is always `"right"`. Agent-initiated threads comment on the new version of code. Commenting on deleted lines (left side) is not supported for agent threads.

### `lgtm status --json`

Print open threads with full context for the agent to act on.

**Arguments:**
- `--json` — required flag. Bare `lgtm status` (human-readable output) is out of scope for this design and left as future work.

**Output schema:**
```json
{
  "session_status": "in_progress",
  "base": "main",
  "head": "feature/foo",
  "stats": {
    "total_threads": 12,
    "open": 3,
    "resolved": 8,
    "wontfix": 1,
    "dismissed": 0,
    "agent_initiated": 2
  },
  "open_threads": [
    {
      "id": "t_01J8XYZABC",
      "origin": "developer",
      "severity": null,
      "file": "src/services/auth.py",
      "line_start": 42,
      "line_end": 48,
      "diff_side": "right",
      "anchor_context": "def retry_request(self, url, max_retries=10):",
      "comments": [
        {
          "id": "c_01J8XYZ001",
          "author": "developer",
          "body": "This retry logic has no backoff.",
          "timestamp": "2026-03-18T14:22:00Z",
          "diff_snapshot": null
        }
      ]
    }
  ]
}
```

**Exit codes:**
| Code | Meaning |
|------|---------|
| 0 | Success (even if no open threads) |
| 1 | General error |
| 2 | No session found |

## Consolidated Exit Codes

All exit codes across lgtm commands:

| Code | Meaning | Used by |
|------|---------|---------|
| 0 | Success | all |
| 1 | General error | all |
| 2 | No session found | all |
| 3 | Approval blocked (reserved for future `approve` command) | — |
| 4 | Thread not found | `reply` |
| 5 | File not found or line out of range | `thread` |
| 6 | Session not active | `reply`, `thread` |

## Atomic Writes & Advisory Locking

Both `reply` and `thread` mutate the session file. Safe write protocol:

1. **Acquire lock** — create `.review/.lock` with PID. If lock exists and PID is alive, retry every 50ms for up to 2 seconds, then exit 1. If PID is dead (stale lock), steal the lock.
2. **Read** — parse `.review/session.json`
3. **Mutate** — modify in memory
4. **Write tmp** — serialize to `.review/session.json.tmp`
5. **Rename** — atomic `rename(session.json.tmp, session.json)`
6. **Release lock** — remove `.review/.lock`

This logic lives in `lgtm-session` crate so the server can adopt it later. The current `write_session` function is replaced with this atomic version.

## Stats Recomputation

After any mutation, stats are recomputed from the threads array:

```
total_threads = threads.len()
open = threads where status == "open"
resolved = threads where status == "resolved"
wontfix = threads where status == "wontfix"
dismissed = threads where status == "dismissed"
agent_initiated = threads where origin == "agent"
```

This is a pure function on the threads vec, added to `lgtm-session`.

Note: `files_reviewed` and `files_total` from the main spec are excluded — the current `FileReviewStatus` type needs to be enriched before these can be computed accurately. Left as future work.

## Getting HEAD Commit Hash

Both `reply` and `thread` need the current HEAD for `diff_snapshot`. This is a simple `git rev-parse HEAD` call. The CLI crate already depends on running git commands (for repo root detection in `lgtm start`), so this adds no new dependency.

## Deliberate Omissions

- **`lgtm resolve` / `lgtm dismiss`** — the agent must NOT resolve or dismiss threads. This is the developer's decision per the main spec.
- **Human-readable `lgtm status`** — future work. Agent-first means `--json` is the priority.
- **Developer-facing commands** (`approve`, `abandon`, `clean`, `diff`) — the web UI already handles these. Can be added later.
