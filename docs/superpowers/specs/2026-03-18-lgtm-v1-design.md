# lgtm v1 — Design Document

## Overview

lgtm is a local code review tool that gives developers a GitHub-like review experience for branch changes on their machine. v1 focuses on the web UI: a diff viewer with inline commenting, file review tracking, and real-time updates.

The agent integration loop (Claude Code reading/writing session.json) is out of scope for v1. The session file format is defined minimally to power the UI, with the full schema from the spec as the target for later versions.

## Decisions

| Decision | Choice | Rationale |
|----------|--------|-----------|
| Language | Rust | Single binary distribution, performance |
| Frontend | Svelte + Vite | Small bundles, simple reactivity, user preference |
| Git library | gix (gitoxide) | Pure Rust, no C deps, ecosystem momentum (Cargo migrating to it) |
| Web server | axum | User's existing experience (rw project), modern tower/hyper stack |
| Syntax highlighting | Shiki | VS Code-quality, built-in diff support, token-level API, actively maintained |
| Diff view | Unified only | Simpler to implement; side-by-side deferred |
| Asset bundling | rust-embed with `embed` feature flag | Dev: reads from dist/. Release: baked into binary. Same pattern as rw |
| Comments | Plain text | No markdown rendering complexity for v1 |
| ID generation | ULID | Sortable, no coordination needed, both Rust and JS can generate |

## Architecture

### Workspace layout

```
lgtm/
  crates/
    lgtm-cli/          # CLI entry point (clap), binary target
    lgtm-server/        # axum HTTP + WebSocket server, file watcher
    lgtm-git/           # gix wrapper: merge-base, diff, blob hash
    lgtm-session/       # session.json data model, read/write
    lgtm-assets/        # rust-embed for frontend static files
  packages/
    web/                # Svelte + Vite + Shiki frontend
  Cargo.toml            # workspace root
```

### Data flow

```
Browser  <--WebSocket-->  lgtm-server  <--reads/writes-->  .review/session.json
                               |                                   |
                           lgtm-git                           Claude Code
                         (gix: diff,                        (reads/writes
                          merge-base)                        session.json)
                                                            [out of scope v1]
```

The server is the hub: it serves the Svelte SPA, provides a REST API for the frontend to read diffs and write comments, and pushes real-time updates over WebSocket when the session file or working tree changes.

The session file is the contract between the UI and Claude Code. They never talk directly. For v1, only the UI reads and writes the session file.

## Data Model (minimal for v1)

### What's in

- Session metadata: `version`, `status`, `base`, `head`, `merge_base`, `created_at`, `updated_at`
- Threads: `id`, `status`, `file`, `line_start`, `line_end`, `diff_side`, `anchor_context`, `comments`
- Comments: `id`, `author`, `body`, `timestamp`
- Files map: `file` -> `status` ("pending" | "reviewed")

### What's deferred

- `origin` / `severity` on threads (agent-initiated threads)
- `diff_snapshot` on comments (commit tracking)
- `reviewed_at` timestamp on files (when the file was marked reviewed)
- `reviewed_hash` on files (blob hash comparison for auto-reset)
- `stats` object (computed on the fly from thread data)
- Lock file / atomic writes (single writer for v1)

### Rust types

```rust
struct Session {
    version: u32,
    status: SessionStatus,        // InProgress | Approved | Abandoned
    base: String,
    head: String,
    merge_base: String,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
    threads: Vec<Thread>,
    files: HashMap<String, FileStatus>,
}

struct Thread {
    id: String,                   // ULID
    status: ThreadStatus,         // Open | Resolved | Wontfix | Dismissed (forward-compat)
    file: String,
    line_start: u32,
    line_end: u32,
    diff_side: DiffSide,          // Left | Right
    anchor_context: String,
    comments: Vec<Comment>,
}

struct Comment {
    id: String,                   // ULID
    author: Author,               // Developer | Agent
    body: String,
    timestamp: DateTime<Utc>,
}

enum FileStatus {
    Pending,
    Reviewed,
}
```

## Server API

### REST endpoints

| Method | Path | Description |
|--------|------|-------------|
| `GET` | `/` | Serves the Svelte SPA (via lgtm-assets) |
| `GET` | `/api/session` | Returns current session state |
| `GET` | `/api/diff` | Returns computed diff (merge-base..HEAD) with hunks |
| `GET` | `/api/diff?file=<path>` | Returns diff for a single file |
| `POST` | `/api/threads` | Create a new thread with initial comment |
| `POST` | `/api/threads/:id/comments` | Add a comment to a thread |
| `PATCH` | `/api/threads/:id` | Update thread status (resolve, wontfix, reopen) |
| `PATCH` | `/api/files?path=<path>` | Mark file as reviewed/pending |
| `PATCH` | `/api/session` | Update session status (approve, abandon) |

File paths use query strings to avoid `/` encoding issues in path segments.

### Error handling

All error responses return JSON: `{ "error": "<message>" }` with appropriate HTTP status codes (400 for bad requests, 404 for missing threads/files, 500 for internal errors). If `session.json` is corrupt or missing, the server returns 500 with a descriptive error. The frontend displays errors as a toast notification.

### WebSocket

Single connection at `ws://localhost:4567/ws`. Server-push only.

Server -> Browser messages:
- `session_updated` — full session state after any change to session.json
- `diff_updated` — changed hunks for specific files when working tree changes

Browser -> Server: none. All mutations go through REST.

REST for mutations gives proper status codes and error handling. WebSocket is only for real-time push.

## Frontend Architecture

### Svelte app structure

```
packages/web/
  src/
    App.svelte              # Root: layout shell, WebSocket connection
    lib/
      api.ts                # REST client (fetch wrappers)
      ws.ts                 # WebSocket client, reconnect logic
      stores/
        session.ts          # Svelte store: session state
        diff.ts             # Svelte store: parsed diff data
      components/
        FileTree.svelte     # Left panel: file list + review checkboxes
        DiffView.svelte     # Main panel: unified diff with line gutters
        Hunk.svelte         # Single diff hunk with highlighted code
        Thread.svelte       # Inline thread: comments + reply box
        Comment.svelte      # Single comment bubble
        StatusBar.svelte    # Bottom bar: summary + approve button
        NewComment.svelte   # Comment input on line click
      utils/
        highlight.ts        # Shiki setup: lazy-load languages, codeToTokens()
        ulid.ts             # ULID generation for client-side IDs
  vite.config.ts
  package.json
```

### Key interactions

1. On load: fetch `/api/session` and `/api/diff`, populate stores
2. WebSocket connects, listens for `session_updated` and `diff_updated`
3. Click line gutter -> `NewComment.svelte` appears inline -> submit POSTs to `/api/threads`
4. Thread status changes -> PATCH to `/api/threads/:id`
5. File review checkbox -> PATCH to `/api/files/:path`
6. All mutations optimistically update local stores; the next `session_updated` WebSocket message overwrites local state, correcting any rejected mutations

### Syntax highlighting

Shiki with fine-grained bundles and the JavaScript engine (no WASM). Create a single highlighter instance on load with a base set of languages (JS, TS, Python, Rust, Go, CSS, HTML, JSON). Detect additional languages from file extensions in the diff and lazy-load them. Use `codeToTokens()` for per-token control in diff rendering.

## Diff Computation

### lgtm-git crate

Wraps `gix` to provide:
1. `merge_base(head, base)` — finds the common ancestor
2. `diff_files(merge_base, head)` — lists changed files with status (added/modified/deleted/renamed)
3. `diff_file(merge_base, head, path)` — returns hunks for a single file

**gix feasibility:** `gix` supports merge-base computation and tree-level diffing. For hunk-level file diffs, `gix-diff` provides blob diffing via the `imara-diff` crate. If `gix`'s hunk-level API proves insufficient during implementation, the fallback is to shell out to `git diff <merge_base>..<head> -- <path>` and parse the unified diff output. The `lgtm-git` crate should define a trait so the backend can be swapped without affecting consumers.

### Hunk data model

```rust
struct DiffFile {
    path: String,
    status: FileChangeKind,       // Added | Modified | Deleted | Renamed
    old_path: Option<String>,     // For renames
    hunks: Vec<Hunk>,
}

struct Hunk {
    old_start: u32,
    old_count: u32,
    new_start: u32,
    new_count: u32,
    lines: Vec<DiffLine>,
}

struct DiffLine {
    kind: LineKind,               // Context | Add | Delete
    content: String,
    old_lineno: Option<u32>,
    new_lineno: Option<u32>,
}
```

The server returns this as JSON. The frontend feeds each line's content through Shiki for highlighting, then renders the unified diff view with old/new line number gutters.

### Incremental updates

When the file watcher detects working tree changes:
1. Recompute diff only for the changed file
2. Push updated hunks over WebSocket (`diff_updated` message)
3. Full diff recomputation only on page load or branch change

## File Watching

Uses the `notify` crate.

| Path | Debounce | Action |
|------|----------|--------|
| `.review/session.json` | 300ms | Re-read session, push to browser via WebSocket |
| Repo working tree | 500ms | Recompute diff for affected files, push updated hunks |

The watcher monitors the entire repo working tree (filtered by `.gitignore` patterns). This catches new files created on the branch after `lgtm start`. The server filters change events to only recompute diffs for files that are in the current diff or newly created.

Ignored: `.git/` internals, `node_modules/`, `__pycache__/`, build directories, `.gitignore` patterns.

## v1 Scope

### In

- `lgtm start --base <ref>` — computes diff, creates or resumes session, starts server, opens browser
- Svelte web UI with file tree, unified diff view, Shiki syntax highlighting
- Create threads by clicking line gutters (single line and range)
- Reply to threads, resolve/wontfix/reopen
- Mark files as reviewed (checkbox in file tree)
- WebSocket live updates when session.json or working tree changes
- Session file read/write (minimal schema)
- Embedded static assets via rust-embed

### Out (deferred)

- Agent-initiated threads (origin, severity, dismiss)
- Claude Code skill file / agent integration
- `lgtm status`, `lgtm approve`, `lgtm abandon`, `lgtm clean`, `lgtm diff` CLI commands
- Side-by-side diff view
- Re-anchoring threads after code changes
- Lock file / concurrent writer safety
- `reviewed_hash` auto-reset on file change
- Markdown in comments
- Session persistence across rebases

## Behavioral Details

### Thread anchoring in v1

Re-anchoring is deferred, but the working tree watcher means diffs update live. When line numbers shift after a code edit, existing threads remain at their original line numbers. The UI renders them at the stored `line_start` position. If that line no longer exists in the diff (e.g., the hunk shrank), the thread renders at the end of the nearest hunk in the same file with a "position may have shifted" indicator. This is a known limitation of v1.

### `lgtm start` resume behavior

If `.review/session.json` already exists when `lgtm start` runs:
- If `status` is `in_progress`: resume. Recompute the merge-base. If it changed, update `merge_base` in the session. Keep all existing threads and file states. Start the server.
- If `status` is `approved` or `abandoned`: print a message suggesting `lgtm clean` first. Exit with code 1.

### Approve button in UI

The StatusBar "Approve session" button is functional in v1. It calls `PATCH /api/session` with `{ "status": "approved" }`. The button is disabled unless: all developer-authored threads are resolved or wontfix, all agent-initiated threads (if any exist in the session file) are resolved or dismissed, and all files are reviewed. This matches the spec's approval requirements and is forward-compatible with agent integration.

### WebSocket reconnection

The frontend reconnects with exponential backoff: 1s, 2s, 4s, 8s, capped at 30s. On reconnect, it re-fetches `/api/session` and `/api/diff` to resync state. No max retry limit — the server is local and expected to come back.

### Author enum forward-compatibility

The `Author` enum includes `Agent` for forward-compatibility with the session file format. In v1, only `Developer` is used by the UI. If a session file contains agent-authored comments (e.g., from manual editing or future agent integration), the UI renders them with a distinct style.

## Open Questions (resolved for v1)

1. **Session persistence across rebases** — Deferred. `lgtm clean` and start over.
2. **Markdown in comments** — Plain text for v1.
3. **Thread ordering in UI** — By position in file, interleaved regardless of origin.
4. **Batch dismiss** — Not needed until agent-initiated threads exist.
