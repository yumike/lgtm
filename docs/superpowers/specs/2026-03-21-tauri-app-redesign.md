# LGTM Tauri App Redesign

## Problem

Each Claude Code session that runs `/lgtm` spawns its own Axum server on port 4567. With 4-6 concurrent sessions, this causes port conflicts. The current architecture assumes one server per review session.

## Solution

Replace the per-session server + browser model with a single persistent Tauri app that manages all review sessions. The CLI becomes a thin HTTP client.

## Architecture

Three components:

- **Tauri app** — native window with embedded Axum server, Svelte frontend in webview, manages all session state
- **CLI** (`lgtm`) — stateless HTTP client, resolves sessions by repo path + branch via API
- **State store** — `~/.lgtm/sessions/{session_id}.json` on disk, in-memory `HashMap` for active use

```
┌──────────────────────────────────────────┐
│  Tauri App (single instance)             │
│  ┌──────────────┐  ┌──────────────────┐  │
│  │ Axum Server   │  │ Svelte Webview   │  │
│  │ (127.0.0.1)   │◄─►│ (tabs per session)│  │
│  └───────┬───────┘  └──────────────────┘  │
│  ┌───────▼───────┐                        │
│  │ Session Store  │  ~/.lgtm/sessions/    │
│  └───────────────┘                        │
└──────────────────────────────────────────┘
       ▲
       │ HTTP API
┌──────┴──────┐
│  lgtm CLI   │  (thin client)
└─────────────┘
```

## Session Identity & State

Each session identified by a ULID, scoped to repo path + head ref. Same repo on different branches = different sessions.

**In-memory:** `HashMap<SessionId, Session>` with all active sessions. Each session holds repo path, base ref, head ref, merge base, threads, file review status, timestamps, and a WebSocket broadcast channel.

**On-disk:** `~/.lgtm/sessions/{session_id}.json` — same schema as current session.json plus `repo_path` field. Written atomically on every mutation. On app launch, loads all sessions with status `in_progress`.

**Lockfile:** `~/.lgtm/server.json`
```json
{
  "pid": 1234,
  "port": 4567
}
```
Written on app start, deleted on clean shutdown. CLI checks PID liveness to detect stale lockfiles.

## API Design

All routes scoped per session.

### Session management
```
POST   /api/sessions                    # Create session (repo_path, base) → session_id
GET    /api/sessions                    # List all sessions
GET    /api/sessions?repo_path=X&head=Y # Resolve session by repo + branch
DELETE /api/sessions/{id}               # Remove session
```

POST /api/sessions returns existing session_id if an in-progress session already exists for the same repo+branch.

### Per-session routes
```
GET    /api/sessions/{id}               # Session details
PATCH  /api/sessions/{id}               # Approve/abandon
GET    /api/sessions/{id}/diff          # All diffs
GET    /api/sessions/{id}/diff?file=p   # Single file diff
POST   /api/sessions/{id}/threads       # Create thread
POST   /api/sessions/{id}/threads/{tid}/comments  # Add comment
PATCH  /api/sessions/{id}/threads/{tid} # Update thread status
PATCH  /api/sessions/{id}/files         # Mark file reviewed
POST   /api/sessions/{id}/submit        # Submit to agent
GET    /api/sessions/{id}/submit        # Check submit status
GET    /api/sessions/{id}/ws            # WebSocket for this session
```

### CLI command mapping
| Command | API call |
|---------|----------|
| `lgtm start --base main` | POST /api/sessions |
| `lgtm status --json` | GET /api/sessions/{id} |
| `lgtm fetch` | WebSocket /api/sessions/{id}/ws, blocks on SubmitStatus |
| `lgtm reply <tid> "body"` | POST /api/sessions/{id}/threads/{tid}/comments |
| `lgtm thread ...` | POST /api/sessions/{id}/threads |
| `lgtm approve` | PATCH /api/sessions/{id} |
| `lgtm abandon` | PATCH /api/sessions/{id} |
| `lgtm diff` | GET /api/sessions/{id}/diff |
| `lgtm clean` | DELETE /api/sessions/{id} |

All CLI commands resolve session_id by detecting current repo path (`git rev-parse --show-toplevel`) and HEAD ref, then querying `GET /api/sessions?repo_path=...&head=...`. The CLI is fully stateless — no `.review/` directory.

## CLI ↔ App Lifecycle

### First `lgtm start` (app not running)
1. CLI reads `~/.lgtm/server.json` — not found or PID dead
2. CLI launches Tauri app as detached process
3. CLI polls `~/.lgtm/server.json` until it appears (timeout ~5s)
4. CLI calls POST /api/sessions
5. App creates session, opens tab, returns session_id

### Subsequent `lgtm start` (app running)
1. CLI reads `~/.lgtm/server.json` — found, PID alive
2. CLI calls POST /api/sessions
3. App creates session, opens new tab, returns session_id

### `lgtm fetch` (agent waiting)
1. CLI resolves session_id via API (repo path + branch)
2. Opens WebSocket to /api/sessions/{id}/ws
3. Blocks until SubmitStatus message received

### App quit
1. User closes Tauri window
2. App deletes `~/.lgtm/server.json`
3. Running CLI commands get connection errors, report "lgtm app not running", exit

### App restart / crash recovery
1. App launches, scans `~/.lgtm/sessions/*.json`
2. Loads sessions with status `in_progress`
3. Reopens tabs for them

## Tauri App Structure

### Window model
- Single window with tab bar at top
- Each tab = one session, labeled `repo_name / branch_name`
- Tab badges show open thread count
- Closing a tab hides it, doesn't destroy the session
- `lgtm start` on same repo+branch focuses existing tab

### Internal architecture
- Tauri backend (Rust): embeds Axum server, starts on app launch, writes lockfile
- Tauri frontend (Svelte): existing UI wrapped in tab container
- Webview talks to embedded Axum server via localhost

### New frontend components
- `TabBar.svelte` — tab strip, session labels, badges, close buttons
- `App.svelte` refactored — becomes per-tab content, instantiated per session

### Crate changes
- `lgtm-server` — multi-session support, session store, updated routes
- `lgtm-cli` — becomes HTTP client, drops server spawning and file watching
- `lgtm-session` — centralized store under `~/.lgtm/`
- `lgtm-git` — no changes
- `lgtm-assets` — removed (Tauri bundles frontend)
- New: `lgtm-app` — Tauri app crate, embeds server, manages window

## File watchers

The `.review/session.json` file watcher is removed. State changes go through the API, and the server broadcasts via WebSocket on mutation.

Working tree file watchers are still needed: the Tauri app watches each session's repo directory for file changes to update diffs (same debounced approach as today).

## What stays the same
- CLI command names and arguments
- Svelte frontend components (diff view, threads, file tree, status bar)
- Git diff computation (`lgtm-git` crate)
- Session data model (threads, comments, file review status)
- The `/lgtm` skill — same commands, same loop

## What gets removed
- `.review/` directory in repos
- `lgtm-assets` crate
- File watchers for session.json
- Advisory `.lock` file (single process owns all state)

## Breaking change
Users must install the Tauri app. The CLI binary alone no longer works standalone.
