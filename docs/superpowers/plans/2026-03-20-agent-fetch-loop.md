# Agent Fetch Loop Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a blocking `lgtm fetch` CLI command, a `POST /api/submit` server endpoint, and a "Submit to agent" UI button that together create an agent-agnostic review loop.

**Architecture:** The `.review/.submit` marker file is the coordination primitive. The web UI creates it (via server route), `lgtm fetch` watches for it (via `notify`), reads session.json, prints open threads, deletes the marker, and exits. The server broadcasts marker status over WebSocket so the UI can reactively update.

**Tech Stack:** Rust 2024 edition, clap 4, notify + notify-debouncer-mini, serde, chrono. Frontend: Svelte 5. All existing workspace dependencies — no new crates.

**Spec:** `docs/superpowers/specs/2026-03-20-agent-fetch-loop-design.md`

---

## File Structure

### Modified files

```
crates/lgtm-cli/src/main.rs              # Add Fetch subcommand
crates/lgtm-cli/tests/cli.rs             # Add fetch integration tests
crates/lgtm-server/src/routes/mod.rs      # Register submit routes
crates/lgtm-server/src/routes/submit.rs   # New: POST/GET /api/submit handlers
crates/lgtm-server/src/ws.rs              # Add SubmitStatus variant to WsMessage
crates/lgtm-server/src/watcher.rs         # Watch for .submit marker changes
packages/web/src/lib/types.ts             # Add SubmitStatus WsMessage type
packages/web/src/lib/api.ts               # Add submitToAgent(), getSubmitStatus()
packages/web/src/lib/stores/submit.ts              # New: submitPending store
packages/web/src/lib/components/StatusBar.svelte  # Add Submit button
```

---

## Chunk 1: Server — Submit Route and WebSocket

### Task 1: Add `SubmitStatus` to `WsMessage`

**Files:**
- Modify: `crates/lgtm-server/src/ws.rs:9-15`

- [ ] **Step 1: Add SubmitStatus variant**

In `crates/lgtm-server/src/ws.rs`, add a new variant to the `WsMessage` enum:

```rust
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", content = "data")]
#[serde(rename_all = "snake_case")]
pub enum WsMessage {
    SessionUpdated(lgtm_session::Session),
    DiffUpdated(Vec<lgtm_git::DiffFile>),
    SubmitStatus(SubmitStatusData),
}

#[derive(Debug, Clone, Serialize)]
pub struct SubmitStatusData {
    pub pending: bool,
}
```

- [ ] **Step 2: Verify it compiles**

Run: `cargo build -p lgtm-server`
Expected: Compiles without errors

- [ ] **Step 3: Commit**

```bash
git add crates/lgtm-server/src/ws.rs
git commit -m "feat(server): add SubmitStatus variant to WsMessage"
```

### Task 2: Add submit routes

**Files:**
- Create: `crates/lgtm-server/src/routes/submit.rs`
- Modify: `crates/lgtm-server/src/routes/mod.rs:1-22`

- [ ] **Step 1: Write tests for submit routes**

Create `crates/lgtm-server/src/routes/submit.rs` with tests at the bottom:

```rust
use std::sync::Arc;

use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use serde::Serialize;

use crate::AppState;
use crate::ws::{WsMessage, SubmitStatusData};

#[derive(Serialize)]
pub struct SubmitResponse {
    pub pending: bool,
}

pub async fn post_submit(
    State(state): State<Arc<AppState>>,
) -> Result<(StatusCode, Json<SubmitResponse>), (StatusCode, Json<serde_json::Value>)> {
    let submit_path = state.session_path.parent().unwrap().join(".submit");
    // Use create_new(true) for O_CREAT | O_EXCL semantics — atomic, no TOCTOU race
    match std::fs::OpenOptions::new().write(true).create_new(true).open(&submit_path) {
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
            return Err((
                StatusCode::CONFLICT,
                Json(serde_json::json!({ "error": "Submit already pending" })),
            ));
        }
        Err(e) => {
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": e.to_string() })),
            ));
        }
        Ok(_) => {}
    }
    let _ = state.broadcast_tx.send(WsMessage::SubmitStatus(SubmitStatusData { pending: true }));
    Ok((StatusCode::CREATED, Json(SubmitResponse { pending: true })))
}

pub async fn get_submit(
    State(state): State<Arc<AppState>>,
) -> Json<SubmitResponse> {
    let submit_path = state.session_path.parent().unwrap().join(".submit");
    Json(SubmitResponse { pending: submit_path.exists() })
}

#[cfg(test)]
mod tests {
    use crate::test_helpers::create_test_app;

    #[tokio::test]
    async fn test_post_submit_creates_marker() {
        let server = create_test_app().await;
        let resp = server.post("/api/submit").await;
        resp.assert_status(axum::http::StatusCode::CREATED);
        let body: serde_json::Value = resp.json();
        assert_eq!(body["pending"], true);
    }

    #[tokio::test]
    async fn test_post_submit_conflict_when_pending() {
        let server = create_test_app().await;
        server.post("/api/submit").await;
        let resp = server.post("/api/submit").await;
        resp.assert_status(axum::http::StatusCode::CONFLICT);
    }

    #[tokio::test]
    async fn test_get_submit_false_initially() {
        let server = create_test_app().await;
        let resp = server.get("/api/submit").await;
        resp.assert_status_ok();
        let body: serde_json::Value = resp.json();
        assert_eq!(body["pending"], false);
    }

    #[tokio::test]
    async fn test_get_submit_true_after_post() {
        let server = create_test_app().await;
        server.post("/api/submit").await;
        let resp = server.get("/api/submit").await;
        let body: serde_json::Value = resp.json();
        assert_eq!(body["pending"], true);
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p lgtm-server test_post_submit test_get_submit`
Expected: Compilation error — module not registered

- [ ] **Step 3: Register submit module and routes**

In `crates/lgtm-server/src/routes/mod.rs`, add the module declaration and route:

```rust
pub mod assets;
pub mod diff;
pub mod files;
pub mod session;
pub mod submit;
pub mod threads;

use std::sync::Arc;

use axum::Router;
use axum::routing::{get, patch, post};

use crate::AppState;

pub fn api_routes() -> Router<Arc<AppState>> {
    Router::new()
        .route("/session", get(session::get_session).patch(session::patch_session))
        .route("/diff", get(diff::get_diff))
        .route("/threads", post(threads::create_thread))
        .route("/threads/{id}/comments", post(threads::add_comment))
        .route("/threads/{id}", patch(threads::patch_thread))
        .route("/files", patch(files::patch_file))
        .route("/submit", post(submit::post_submit).get(submit::get_submit))
}
```

- [ ] **Step 4: Update test helper to use real temp directory**

The submit routes need a real filesystem. Check that `create_test_app()` in `crates/lgtm-server/src/test_helpers.rs` uses a temp directory for `session_path`. If it uses `/tmp/test-session.json`, update it to use `tempfile::TempDir` so the `.review/` parent directory exists for the submit marker.

Update `create_test_app` to create a `.review/` dir and use it:

```rust
pub async fn create_test_app() -> TestServer {
    let dir = tempfile::TempDir::new().unwrap();
    let review_dir = dir.path().join(".review");
    std::fs::create_dir_all(&review_dir).unwrap();
    let session_path = review_dir.join("session.json");
    let session = Session::new("main", "feature/test", "abc1234");
    lgtm_session::write_session_atomic(&session_path, &session).unwrap();

    let (broadcast_tx, _) = tokio::sync::broadcast::channel(32);
    let state = Arc::new(AppState {
        session: RwLock::new(session),
        session_path,
        diff_provider: Box::new(MockDiffProvider),
        repo_path: dir.path().to_path_buf(),
        broadcast_tx,
    });
    let app = create_router(state);
    // Leak TempDir so it's not cleaned up during the test
    std::mem::forget(dir);
    TestServer::new(app).unwrap()
}
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test -p lgtm-server submit`
Expected: All 4 tests pass

- [ ] **Step 6: Run all server tests to check for regressions**

Run: `cargo test -p lgtm-server`
Expected: All pass

- [ ] **Step 7: Commit**

```bash
git add crates/lgtm-server/src/routes/submit.rs crates/lgtm-server/src/routes/mod.rs crates/lgtm-server/src/test_helpers.rs
git commit -m "feat(server): add POST/GET /api/submit routes for agent handoff"
```

### Task 3: Broadcast submit status from session watcher

**Files:**
- Modify: `crates/lgtm-server/src/watcher.rs:16-60`

- [ ] **Step 1: Add submit marker detection to session watcher**

In `crates/lgtm-server/src/watcher.rs`, the session file watcher callback (lines 26-39) already watches the `.review/` directory. Extend it to also detect `.submit` changes:

```rust
move |events: Result<Vec<notify_debouncer_mini::DebouncedEvent>, notify::Error>| {
    if let Ok(events) = events {
        let submit_path = session_path.parent().unwrap().join(".submit");

        let has_session_change = events
            .iter()
            .any(|e| e.kind == DebouncedEventKind::Any && e.path == session_path);
        if has_session_change {
            let state = state_for_session.clone();
            let tx = tx_for_session.clone();
            rt.spawn(async move {
                if let Ok(session) = lgtm_session::read_session(&state.session_path) {
                    *state.session.write().await = session.clone();
                    let _ = tx.send(WsMessage::SessionUpdated(session));
                }
            });
        }

        let has_submit_change = events
            .iter()
            .any(|e| e.kind == DebouncedEventKind::Any && e.path == submit_path);
        if has_submit_change {
            let pending = submit_path.exists();
            let tx = tx_for_session.clone();
            rt.spawn(async move {
                let _ = tx.send(WsMessage::SubmitStatus(
                    crate::ws::SubmitStatusData { pending },
                ));
            });
        }
    }
}
```

- [ ] **Step 2: Verify it compiles**

Run: `cargo build -p lgtm-server`
Expected: Compiles without errors

- [ ] **Step 3: Commit**

```bash
git add crates/lgtm-server/src/watcher.rs
git commit -m "feat(server): broadcast submit status changes over WebSocket"
```

### Task 4: Clean up stale submit marker in `lgtm start`

**Files:**
- Modify: `crates/lgtm-cli/src/main.rs:103-143`

- [ ] **Step 1: Add marker cleanup after session setup**

In `crates/lgtm-cli/src/main.rs`, in the `start()` function, after the session is loaded/created (around line 143), add:

```rust
    // Clean up stale submit marker from previous sessions
    let submit_path = repo_path.join(".review").join(".submit");
    if submit_path.exists() {
        let _ = std::fs::remove_file(&submit_path);
    }
```

Insert this right before `let (broadcast_tx, _) = tokio::sync::broadcast::channel(64);` (line 145).

- [ ] **Step 2: Verify it compiles**

Run: `cargo build -p lgtm`
Expected: Compiles without errors

- [ ] **Step 3: Commit**

```bash
git add crates/lgtm-cli/src/main.rs
git commit -m "fix(cli): clean up stale .submit marker on lgtm start"
```

---

## Chunk 2: CLI — `lgtm fetch` Command

### Task 5: Add `Fetch` subcommand and blocking implementation

**Files:**
- Modify: `crates/lgtm-cli/src/main.rs`

- [ ] **Step 1: Add Fetch to Commands enum**

In `crates/lgtm-cli/src/main.rs`, add the `Fetch` variant to the `Commands` enum (after the `Thread` variant, before `Start`):

```rust
    /// Wait for developer to submit review comments, then print open threads
    Fetch {
        /// Timeout in seconds (default: wait indefinitely)
        #[arg(long)]
        timeout: Option<u64>,
    },
```

- [ ] **Step 2: Add match arm in main**

In the `match cli.command` block (line 89), add:

```rust
Commands::Fetch { timeout } => fetch(timeout)?,
```

- [ ] **Step 3: Add notify dependencies to lgtm-cli**

In `crates/lgtm-cli/Cargo.toml`, add to `[dependencies]`:

```toml
notify = { workspace = true }
notify-debouncer-mini = { workspace = true }
```

- [ ] **Step 4: Implement the `fetch` function**

Add the `fetch` function to `main.rs`:

```rust
fn fetch(timeout: Option<u64>) -> Result<()> {
    let repo_path = find_repo_root()?;
    let review_dir = repo_path.join(".review");
    let session_path = review_dir.join("session.json");
    let submit_path = review_dir.join(".submit");

    if !review_dir.exists() || !session_path.exists() {
        eprintln!("Error: no review session found");
        std::process::exit(2);
    }

    let session = lgtm_session::read_session(&session_path)
        .context("Failed to read session")?;

    if session.status != SessionStatus::InProgress {
        eprintln!("Error: session is not active (status: {:?})", session.status);
        std::process::exit(6);
    }

    // If marker already exists, pick up immediately
    if !submit_path.exists() {
        // Wait for .submit marker or session status change
        if !wait_for_submit(&review_dir, &submit_path, &session_path, timeout)? {
            // Timed out
            let session = lgtm_session::read_session(&session_path)
                .context("Failed to read session")?;
            let output = serde_json::json!({
                "timed_out": true,
                "session_status": session.status,
                "base": session.base,
                "head": session.head,
                "merge_base": session.merge_base,
                "open_threads": [],
            });
            println!("{}", serde_json::to_string_pretty(&output)?);
            return Ok(());
        }
    }

    // Re-read session (may have changed since we started waiting)
    let session = lgtm_session::read_session(&session_path)
        .context("Failed to read session")?;

    // Delete the marker
    let _ = std::fs::remove_file(&submit_path);

    // Check if session ended while we were waiting
    if session.status != SessionStatus::InProgress {
        let output = serde_json::json!({
            "session_status": session.status,
            "base": session.base,
            "head": session.head,
            "merge_base": session.merge_base,
            "open_threads": [],
        });
        println!("{}", serde_json::to_string_pretty(&output)?);
        return Ok(());
    }

    let open_threads: Vec<&lgtm_session::Thread> = session
        .threads
        .iter()
        .filter(|t| t.status == lgtm_session::ThreadStatus::Open)
        .collect();

    let output = serde_json::json!({
        "session_status": session.status,
        "base": session.base,
        "head": session.head,
        "merge_base": session.merge_base,
        "open_threads": open_threads,
    });

    println!("{}", serde_json::to_string_pretty(&output)?);
    Ok(())
}

fn wait_for_submit(
    review_dir: &std::path::Path,
    submit_path: &std::path::Path,
    session_path: &std::path::Path,
    timeout: Option<u64>,
) -> Result<bool> {
    use std::sync::mpsc;

    let (tx, rx) = mpsc::channel();

    let submit_target = submit_path.to_path_buf();
    let session_target = session_path.to_path_buf();
    let mut debouncer = notify_debouncer_mini::new_debouncer(
        std::time::Duration::from_millis(300),
        move |events: Result<Vec<notify_debouncer_mini::DebouncedEvent>, notify::Error>| {
            if let Ok(events) = events {
                for event in events {
                    if event.kind == notify_debouncer_mini::DebouncedEventKind::Any {
                        if event.path == submit_target {
                            let _ = tx.send(WaitEvent::SubmitCreated);
                            return;
                        }
                        if event.path == session_target {
                            let _ = tx.send(WaitEvent::SessionChanged);
                            return;
                        }
                    }
                }
            }
        },
    )
    .context("Failed to create file watcher")?;

    debouncer
        .watcher()
        .watch(review_dir, notify::RecursiveMode::NonRecursive)
        .context("Failed to watch .review directory")?;

    // Check again after watcher is set up (race condition window)
    if submit_path.exists() {
        return Ok(true);
    }

    let deadline = timeout.map(|s| std::time::Instant::now() + std::time::Duration::from_secs(s));

    loop {
        let recv_result = if let Some(deadline) = deadline {
            let remaining = deadline.saturating_duration_since(std::time::Instant::now());
            if remaining.is_zero() {
                return Ok(false); // timed out
            }
            rx.recv_timeout(remaining)
        } else {
            rx.recv().map_err(|_| mpsc::RecvTimeoutError::Disconnected)
        };

        match recv_result {
            Ok(WaitEvent::SubmitCreated) => return Ok(true),
            Ok(WaitEvent::SessionChanged) => {
                // Check if session status changed to approved/abandoned
                if let Ok(session) = lgtm_session::read_session(session_path) {
                    if session.status != SessionStatus::InProgress {
                        return Ok(true); // session ended, unblock
                    }
                }
            }
            Err(mpsc::RecvTimeoutError::Timeout) => return Ok(false),
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                bail!("File watcher disconnected unexpectedly");
            }
        }
    }
}

enum WaitEvent {
    SubmitCreated,
    SessionChanged,
}
```

- [ ] **Step 5: Verify it compiles**

Run: `cargo build -p lgtm`
Expected: Compiles without errors

- [ ] **Step 6: Commit**

```bash
git add crates/lgtm-cli/Cargo.toml crates/lgtm-cli/src/main.rs
git commit -m "feat(cli): add lgtm fetch blocking command"
```

### Task 6: Add fetch integration tests

**Files:**
- Modify: `crates/lgtm-cli/tests/cli.rs`

- [ ] **Step 1: Add test for fetch with no session**

Add to `crates/lgtm-cli/tests/cli.rs`:

```rust
#[test]
fn fetch_no_session_exits_2() {
    let dir = setup_repo();
    lgtm()
        .arg("fetch")
        .current_dir(dir.path())
        .assert()
        .code(2);
}
```

- [ ] **Step 2: Add test for fetch with non-active session**

```rust
#[test]
fn fetch_abandoned_session_exits_6() {
    let dir = setup_repo();
    let json = r#"{
        "version": 1,
        "status": "abandoned",
        "base": "main",
        "head": "feature/test",
        "merge_base": "abc1234",
        "created_at": "2026-03-18T14:00:00Z",
        "updated_at": "2026-03-18T14:00:00Z",
        "threads": [],
        "files": {}
    }"#;
    write_session(dir.path(), json);
    lgtm()
        .arg("fetch")
        .current_dir(dir.path())
        .assert()
        .code(6);
}
```

- [ ] **Step 3: Add test for fetch with approved session**

```rust
#[test]
fn fetch_approved_session_exits_6() {
    let dir = setup_repo();
    let json = r#"{
        "version": 1,
        "status": "approved",
        "base": "main",
        "head": "feature/test",
        "merge_base": "abc1234",
        "created_at": "2026-03-18T14:00:00Z",
        "updated_at": "2026-03-18T14:00:00Z",
        "threads": [],
        "files": {}
    }"#;
    write_session(dir.path(), json);
    lgtm()
        .arg("fetch")
        .current_dir(dir.path())
        .assert()
        .code(6);
}
```

- [ ] **Step 4: Add test for fetch with pre-existing marker**

```rust
#[test]
fn fetch_returns_immediately_when_marker_exists() {
    let dir = setup_repo();
    write_session(dir.path(), &session_json_with_thread());
    // Create the submit marker
    std::fs::write(dir.path().join(".review/.submit"), "").unwrap();

    let output = lgtm()
        .arg("fetch")
        .current_dir(dir.path())
        .assert()
        .code(0)
        .get_output()
        .stdout
        .clone();

    let result: serde_json::Value = serde_json::from_slice(&output).unwrap();
    assert_eq!(result["session_status"], "in_progress");
    assert!(!result["open_threads"].as_array().unwrap().is_empty());
    // Marker should be deleted
    assert!(!dir.path().join(".review/.submit").exists());
}
```

- [ ] **Step 5: Add test for fetch with timeout**

```rust
#[test]
fn fetch_timeout_returns_timed_out() {
    let dir = setup_repo();
    write_session(dir.path(), &session_json_with_thread());

    let output = lgtm()
        .args(["fetch", "--timeout", "1"])
        .current_dir(dir.path())
        .assert()
        .code(0)
        .get_output()
        .stdout
        .clone();

    let result: serde_json::Value = serde_json::from_slice(&output).unwrap();
    assert_eq!(result["timed_out"], true);
    assert!(result["open_threads"].as_array().unwrap().is_empty());
}
```

- [ ] **Step 6: Add test for fetch picks up marker created during wait**

```rust
#[test]
fn fetch_unblocks_when_marker_created() {
    let dir = setup_repo();
    write_session(dir.path(), &session_json_with_thread());

    let review_dir = dir.path().join(".review");
    let submit_path = review_dir.join(".submit");

    // Spawn a thread that creates the marker after 500ms
    let submit_path_clone = submit_path.clone();
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(500));
        std::fs::write(&submit_path_clone, "").unwrap();
    });

    let output = lgtm()
        .args(["fetch", "--timeout", "5"])
        .current_dir(dir.path())
        .assert()
        .code(0)
        .get_output()
        .stdout
        .clone();

    let result: serde_json::Value = serde_json::from_slice(&output).unwrap();
    assert_eq!(result["session_status"], "in_progress");
    assert!(!result["open_threads"].as_array().unwrap().is_empty());
    // Marker should be deleted
    assert!(!submit_path.exists());
}
```

- [ ] **Step 7: Run tests**

Run: `cargo test -p lgtm -- fetch`
Expected: All 6 tests pass

- [ ] **Step 8: Run all CLI tests for regressions**

Run: `cargo test -p lgtm`
Expected: All pass

- [ ] **Step 9: Commit**

```bash
git add crates/lgtm-cli/tests/cli.rs
git commit -m "test(cli): add integration tests for lgtm fetch command"
```

---

## Chunk 3: Frontend — Submit to Agent Button

### Task 7: Add submit store, API, and WsMessage type

**Files:**
- Create: `packages/web/src/lib/stores/submit.ts`
- Modify: `packages/web/src/lib/types.ts`
- Modify: `packages/web/src/lib/api.ts`

- [ ] **Step 1: Create submitPending store**

Create `packages/web/src/lib/stores/submit.ts`:

```typescript
import { writable } from 'svelte/store';

export const submitPending = writable(false);
```

- [ ] **Step 2: Add SubmitStatus to WsMessage type**

In `packages/web/src/lib/types.ts`, update the `WsMessage` type (line 65-67):

```typescript
export type WsMessage =
  | { type: 'session_updated'; data: Session }
  | { type: 'diff_updated'; data: DiffFile[] }
  | { type: 'submit_status'; data: { pending: boolean } };
```

- [ ] **Step 2: Add submit API functions**

In `packages/web/src/lib/api.ts`, add at the bottom:

```typescript
export function submitToAgent(): Promise<{ pending: boolean }> {
  return request('/submit', { method: 'POST' });
}

export function getSubmitStatus(): Promise<{ pending: boolean }> {
  return request('/submit');
}
```

- [ ] **Step 4: Commit**

```bash
git add packages/web/src/lib/stores/submit.ts packages/web/src/lib/types.ts packages/web/src/lib/api.ts
git commit -m "feat(web): add submit store, API client, and WsMessage type"
```

### Task 8: Add Submit to Agent button to StatusBar

**Files:**
- Modify: `packages/web/src/lib/components/StatusBar.svelte`

- [ ] **Step 1: Update StatusBar with submit button**

Replace the full content of `packages/web/src/lib/components/StatusBar.svelte`:

```svelte
<script lang="ts">
  import { session } from '../stores/session';
  import { diffFiles } from '../stores/diff';
  import { submitPending } from '../stores/submit';
  import { patchSession, submitToAgent } from '../api';

  let threads = $derived($session?.threads ?? []);
  let openCount = $derived(threads.filter(t => t.status === 'open').length);
  let resolvedCount = $derived(threads.filter(t => t.status === 'resolved').length);
  let wontfixCount = $derived(threads.filter(t => t.status === 'wontfix').length);
  let dismissedCount = $derived(threads.filter(t => t.status === 'dismissed').length);
  let totalFiles = $derived($diffFiles.length);
  let reviewedFiles = $derived(Object.values($session?.files ?? {}).filter(s => s === 'reviewed').length);

  // Approve requires:
  // - developer threads: all resolved or wontfix
  // - agent threads: all resolved or dismissed
  let devThreadsCleared = $derived(
    threads.filter(t => t.origin !== 'agent').every(t => t.status === 'resolved' || t.status === 'wontfix')
  );
  let agentThreadsCleared = $derived(
    threads.filter(t => t.origin === 'agent').every(t => t.status === 'resolved' || t.status === 'dismissed')
  );

  let isApproved = $derived($session?.status === 'approved');
  let isAbandoned = $derived($session?.status === 'abandoned');
  let canApprove = $derived(
    !isApproved && !isAbandoned && openCount === 0 &&
    devThreadsCleared && agentThreadsCleared &&
    reviewedFiles >= totalFiles && totalFiles > 0
  );

  // Submit state — reads from the shared store (updated via WebSocket in App.svelte)
  let canSubmit = $derived(
    !isApproved && !isAbandoned && !$submitPending && openCount > 0
  );

  async function submit() {
    if (!canSubmit) return;
    try {
      await submitToAgent();
      submitPending.set(true);
    } catch {
      // toast
    }
  }

  async function approve() {
    if (!canApprove) return;
    try {
      await patchSession('approved');
    } catch {
      // toast
    }
  }
</script>

<footer class="status-bar" class:approved={isApproved} class:abandoned={isAbandoned}>
  <div class="status-left">
    {#if isApproved}
      <span class="status-badge approved-badge">Approved</span>
    {:else if isAbandoned}
      <span class="status-badge abandoned-badge">Abandoned</span>
    {/if}
    <span>{openCount} open</span>
    <span>&middot;</span>
    <span>{resolvedCount} resolved</span>
    {#if wontfixCount > 0}
      <span>&middot;</span>
      <span>{wontfixCount} won't fix</span>
    {/if}
    {#if dismissedCount > 0}
      <span>&middot;</span>
      <span>{dismissedCount} dismissed</span>
    {/if}
    <span>&middot;</span>
    <span>{reviewedFiles}/{totalFiles} files reviewed</span>
  </div>
  <div class="status-right">
    {#if isApproved}
      <span class="approved-text">Session approved</span>
    {:else}
      <button class="btn-submit" disabled={!canSubmit} onclick={submit}>
        {#if $submitPending}
          Waiting for agent...
        {:else}
          Submit to agent
        {/if}
      </button>
      <button class="btn-approve" disabled={!canApprove} onclick={approve}>
        Approve session
      </button>
    {/if}
  </div>
</footer>

<style>
  .status-bar {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 8px 16px;
    background: #161b22;
    border-top: 1px solid #30363d;
    font-size: 13px;
    color: #8b949e;
  }

  .status-left {
    display: flex;
    gap: 8px;
  }

  .status-right {
    display: flex;
    gap: 8px;
  }

  .btn-submit {
    padding: 4px 16px;
    border: 1px solid #30363d;
    border-radius: 6px;
    background: #21262d;
    color: #c9d1d9;
    cursor: pointer;
    font-size: 13px;
  }

  .btn-submit:disabled {
    opacity: 0.3;
    cursor: not-allowed;
  }

  .btn-approve {
    padding: 4px 16px;
    border: none;
    border-radius: 6px;
    background: #238636;
    color: white;
    cursor: pointer;
    font-size: 13px;
  }

  .btn-approve:disabled {
    opacity: 0.3;
    cursor: not-allowed;
  }

  .status-bar.approved {
    border-top: 1px solid #238636;
    background: #0d1117;
  }

  .status-bar.abandoned {
    border-top: 1px solid #da3633;
  }

  .status-badge {
    font-weight: 600;
    padding: 2px 8px;
    border-radius: 12px;
    font-size: 12px;
  }

  .approved-badge {
    background: #238636;
    color: white;
  }

  .abandoned-badge {
    background: #da3633;
    color: white;
  }

  .approved-text {
    color: #3fb950;
    font-weight: 600;
  }
</style>
```

- [ ] **Step 2: Wire up submit_status WebSocket messages in App.svelte**

In `packages/web/src/App.svelte`, add the import for the submit store and the `getSubmitStatus` API function:

```typescript
import { submitPending } from './lib/stores/submit';
import { getSubmitStatus } from './lib/api';
```

In the `onMount` function, alongside the existing `getSession()` and `getDiff()` calls, add initial submit status loading:

```typescript
getSubmitStatus().then(s => submitPending.set(s.pending)).catch(() => {});
```

In the WebSocket `onMessage` handler, add an `else if` branch matching the existing pattern:

```typescript
} else if (msg.type === 'submit_status') {
  submitPending.set(msg.data.pending);
}
```

- [ ] **Step 3: Build frontend to verify**

Run: `cd packages/web && pnpm build`
Expected: Build succeeds without errors

- [ ] **Step 4: Commit**

```bash
git add packages/web/src/lib/components/StatusBar.svelte packages/web/src/App.svelte
git commit -m "feat(web): add Submit to agent button in status bar"
```

---

## Chunk 4: End-to-End Verification

### Task 9: Full integration test

- [ ] **Step 1: Run all Rust tests**

Run: `cargo test`
Expected: All pass

- [ ] **Step 2: Build frontend**

Run: `cd packages/web && pnpm build`
Expected: Build succeeds

- [ ] **Step 3: Build release binary**

Run: `cargo build --release`
Expected: Compiles without errors

- [ ] **Step 4: Manual smoke test**

```bash
# In a test repo with a branch that has changes vs main:
cargo run -- start --no-open --port 4567

# In another terminal:
# 1. Create a submit marker manually
touch .review/.submit

# 2. In another terminal, run fetch (should return immediately)
cargo run -- fetch
# Expected: JSON with open_threads

# 3. Run fetch without marker (should block)
cargo run -- fetch --timeout 3
# Expected: blocks, then returns with timed_out: true after 3 seconds

# 4. Run fetch, then create marker while blocking
cargo run -- fetch --timeout 10 &
sleep 1 && touch .review/.submit
# Expected: fetch unblocks, prints JSON, marker deleted
```

- [ ] **Step 5: Manual UI smoke test**

```
# Open the web UI in browser at http://localhost:4567
# 1. Create a thread via the UI (click a line number, leave a comment)
# 2. Verify "Submit to agent" button is enabled
# 3. Click "Submit to agent"
# 4. Verify button shows "Waiting for agent..." and is disabled
# 5. In a terminal, run: cargo run -- fetch
# 6. Verify fetch returns immediately with the open thread
# 7. Verify the UI button re-enables to "Submit to agent"
```

- [ ] **Step 6: Commit any fixes**

If any issues found during smoke testing, fix and commit.
