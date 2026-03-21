# LGTM Tauri App Redesign Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace per-session server + browser with a single persistent Tauri app managing multiple review sessions, with CLI as a thin HTTP client.

**Architecture:** Tauri app embeds Axum server (dynamic port) + Svelte webview with tabs. CLI discovers app via `~/.lgtm/server.json` lockfile, communicates via HTTP API. Session state centralized in `~/.lgtm/sessions/`.

**Tech Stack:** Rust (Tauri 2, Axum 0.8, Tokio), TypeScript (Svelte 5, Vite 8)

---

## File Structure

### New files
- `crates/lgtm-session/src/store.rs` — multi-session in-memory store with disk persistence
- `crates/lgtm-server/src/lockfile.rs` — `~/.lgtm/server.json` lockfile management
- `crates/lgtm-server/src/routes/sessions.rs` — session CRUD routes (POST/GET/DELETE /api/sessions)
- `crates/lgtm-app/` — Tauri app crate (new workspace member)
- `crates/lgtm-app/Cargo.toml`
- `crates/lgtm-app/src/main.rs` — Tauri entry point, embeds Axum server
- `crates/lgtm-app/tauri.conf.json` — Tauri configuration
- `crates/lgtm-app/build.rs` — Tauri build script
- `packages/web/src/lib/components/TabBar.svelte` — tab strip component
- `packages/web/src/lib/components/Shell.svelte` — root shell with tabs + per-session content
- `packages/web/src/lib/stores/sessions.ts` — multi-session store

### Modified files
- `Cargo.toml` — add lgtm-app member, remove lgtm-assets dep
- `crates/lgtm-session/src/lib.rs` — add `id`, `repo_path` to Session; remove lock/file I/O
- `crates/lgtm-server/Cargo.toml` — remove lgtm-assets dep, add reqwest for tests
- `crates/lgtm-server/src/lib.rs` — new AppState with SessionStore, remove single-session state
- `crates/lgtm-server/src/routes/mod.rs` — new route tree under /api/sessions/{id}/
- `crates/lgtm-server/src/routes/threads.rs` — extract session from store by path param
- `crates/lgtm-server/src/routes/diff.rs` — extract session from store by path param
- `crates/lgtm-server/src/routes/files.rs` — extract session from store by path param
- `crates/lgtm-server/src/routes/submit.rs` — in-memory boolean, no file marker
- `crates/lgtm-server/src/ws.rs` — per-session WebSocket with session_id path param
- `crates/lgtm-server/src/watcher.rs` — deduplicated per-repo watchers
- `crates/lgtm-cli/Cargo.toml` — remove server deps, add reqwest + tungstenite
- `crates/lgtm-cli/src/main.rs` — rewrite as HTTP client
- `packages/web/src/main.ts` — mount Shell instead of App
- `packages/web/src/lib/api.ts` — session_id prefix on all routes
- `packages/web/src/lib/types.ts` — add id, repo_path to Session
- `packages/web/src/lib/ws.ts` — session_id in WebSocket URL
- `packages/web/src/App.svelte` — move to `packages/web/src/lib/components/App.svelte`, becomes per-session with session_id prop
- `packages/web/src/lib/stores/session.ts` — keyed by session_id
- `packages/web/src/lib/stores/diff.ts` — keyed by session_id
- `packages/web/src/lib/stores/submit.ts` — keyed by session_id

### Removed files
- `crates/lgtm-assets/` — entire crate (Tauri bundles frontend)
- `crates/lgtm-server/src/routes/assets.rs` — no longer needed
- `crates/lgtm-server/src/test_helpers.rs` — will be rewritten for multi-session

---

## Chunk 1: Session Model & Store

### Task 1: Add id and repo_path to Session struct

**Files:**
- Modify: `crates/lgtm-session/src/lib.rs`

- [ ] **Step 1: Write failing test for new Session fields**

In `crates/lgtm-session/src/lib.rs`, add test:

```rust
#[test]
fn test_session_has_id_and_repo_path() {
    let session = Session::new("main", "feature/foo", "abc123", PathBuf::from("/tmp/repo"));
    assert!(!session.id.to_string().is_empty());
    assert_eq!(session.repo_path, PathBuf::from("/tmp/repo"));
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p lgtm-session test_session_has_id_and_repo_path`
Expected: FAIL — `Session::new` doesn't accept `repo_path`, no `id` field

- [ ] **Step 3: Add id and repo_path fields to Session**

In `crates/lgtm-session/src/lib.rs`, add to `Session` struct:

```rust
pub struct Session {
    pub id: ulid::Ulid,
    pub repo_path: PathBuf,
    // ... existing fields
}
```

Update `Session::new` to accept `repo_path: PathBuf` and generate `id: ulid::Ulid::new()`.

Add `use std::path::PathBuf;` to imports.

- [ ] **Step 4: Fix all existing tests and call sites**

The `Session::new` signature changed. Update all callers:
- `crates/lgtm-session/src/lib.rs` — existing tests
- `crates/lgtm-server/src/` — test helpers, route tests
- `crates/lgtm-cli/src/main.rs` — start command

Run: `cargo test --workspace`
Expected: all tests pass

- [ ] **Step 5: Commit**

```bash
git add -A && git commit -m "feat(session): add id and repo_path fields to Session"
```

### Task 2: Create SessionStore for multi-session management

**Files:**
- Create: `crates/lgtm-session/src/store.rs`
- Modify: `crates/lgtm-session/src/lib.rs` (add module, re-export)

- [ ] **Step 1: Write failing tests for SessionStore**

Create `crates/lgtm-session/src/store.rs` with tests:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use tempfile::TempDir;

    #[test]
    fn test_create_and_get_session() {
        let dir = TempDir::new().unwrap();
        let store = SessionStore::new(dir.path().to_path_buf());
        let session = store.create("main", "feature/x", "abc123", PathBuf::from("/tmp/repo")).unwrap();
        let retrieved = store.get(session.id).unwrap();
        assert_eq!(retrieved.id, session.id);
        assert_eq!(retrieved.repo_path, PathBuf::from("/tmp/repo"));
    }

    #[test]
    fn test_find_by_repo_and_branch() {
        let dir = TempDir::new().unwrap();
        let store = SessionStore::new(dir.path().to_path_buf());
        let session = store.create("main", "feature/x", "abc123", PathBuf::from("/tmp/repo")).unwrap();
        let found = store.find_by_repo_and_head(Path::new("/tmp/repo"), "feature/x").unwrap();
        assert_eq!(found.unwrap().id, session.id);
    }

    #[test]
    fn test_create_returns_existing_for_same_repo_branch() {
        let dir = TempDir::new().unwrap();
        let store = SessionStore::new(dir.path().to_path_buf());
        let s1 = store.create("main", "feature/x", "abc123", PathBuf::from("/tmp/repo")).unwrap();
        let s2 = store.create("main", "feature/x", "abc123", PathBuf::from("/tmp/repo")).unwrap();
        assert_eq!(s1.id, s2.id);
    }

    #[test]
    fn test_list_sessions() {
        let dir = TempDir::new().unwrap();
        let store = SessionStore::new(dir.path().to_path_buf());
        store.create("main", "feature/a", "abc", PathBuf::from("/repo1")).unwrap();
        store.create("main", "feature/b", "def", PathBuf::from("/repo2")).unwrap();
        let sessions = store.list();
        assert_eq!(sessions.len(), 2);
    }

    #[test]
    fn test_remove_session() {
        let dir = TempDir::new().unwrap();
        let store = SessionStore::new(dir.path().to_path_buf());
        let session = store.create("main", "feature/x", "abc", PathBuf::from("/repo")).unwrap();
        store.remove(session.id).unwrap();
        assert!(store.get(session.id).is_err());
    }

    #[test]
    fn test_persistence_to_disk() {
        let dir = TempDir::new().unwrap();
        let store = SessionStore::new(dir.path().to_path_buf());
        let session = store.create("main", "feature/x", "abc", PathBuf::from("/repo")).unwrap();

        // Create new store from same directory — should load persisted sessions
        let store2 = SessionStore::new(dir.path().to_path_buf());
        store2.load().unwrap();
        let loaded = store2.get(session.id).unwrap();
        assert_eq!(loaded.id, session.id);
    }

    #[test]
    fn test_update_session() {
        let dir = TempDir::new().unwrap();
        let store = SessionStore::new(dir.path().to_path_buf());
        let session = store.create("main", "feature/x", "abc", PathBuf::from("/repo")).unwrap();
        store.update(session.id, |s| {
            s.status = SessionStatus::Approved;
        }).unwrap();
        let updated = store.get(session.id).unwrap();
        assert_eq!(updated.status, SessionStatus::Approved);
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p lgtm-session store`
Expected: FAIL — module doesn't exist yet

- [ ] **Step 3: Implement SessionStore**

In `crates/lgtm-session/src/store.rs`:

```rust
use crate::{Session, SessionStatus, SessionError};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::RwLock;
use ulid::Ulid;

pub struct SessionStore {
    dir: PathBuf,
    sessions: RwLock<HashMap<Ulid, Session>>,
}

impl SessionStore {
    pub fn new(dir: PathBuf) -> Self {
        Self {
            dir,
            sessions: RwLock::new(HashMap::new()),
        }
    }

    /// Load all in-progress sessions from disk
    pub fn load(&self) -> Result<(), SessionError> {
        std::fs::create_dir_all(&self.dir)?;
        let mut sessions = self.sessions.write().unwrap();
        for entry in std::fs::read_dir(&self.dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.extension().is_some_and(|e| e == "json") {
                let content = std::fs::read_to_string(&path)?;
                let session: Session = serde_json::from_str(&content)?;
                if session.status == SessionStatus::InProgress {
                    sessions.insert(session.id, session);
                }
            }
        }
        Ok(())
    }

    /// Create a new session or return existing one for same repo+branch
    pub fn create(
        &self,
        base: &str,
        head: &str,
        merge_base: &str,
        repo_path: PathBuf,
    ) -> Result<Session, SessionError> {
        let mut sessions = self.sessions.write().unwrap();

        // Check for existing in-progress session with same repo+head
        for session in sessions.values() {
            if session.repo_path == repo_path
                && session.head == head
                && session.status == SessionStatus::InProgress
            {
                return Ok(session.clone());
            }
        }

        let session = Session::new(base, head, merge_base, repo_path);
        self.persist(&session)?;
        sessions.insert(session.id, session.clone());
        Ok(session)
    }

    pub fn get(&self, id: Ulid) -> Result<Session, SessionError> {
        let sessions = self.sessions.read().unwrap();
        sessions.get(&id).cloned().ok_or(SessionError::NotFound(id.to_string()))
    }

    pub fn find_by_repo_and_head(
        &self,
        repo_path: &Path,
        head: &str,
    ) -> Result<Option<Session>, SessionError> {
        let sessions = self.sessions.read().unwrap();
        Ok(sessions.values().find(|s| {
            s.repo_path == repo_path
                && s.head == head
                && s.status == SessionStatus::InProgress
        }).cloned())
    }

    pub fn list(&self) -> Vec<Session> {
        let sessions = self.sessions.read().unwrap();
        sessions.values().cloned().collect()
    }

    pub fn update<F>(&self, id: Ulid, f: F) -> Result<Session, SessionError>
    where
        F: FnOnce(&mut Session),
    {
        let mut sessions = self.sessions.write().unwrap();
        let session = sessions.get_mut(&id).ok_or(SessionError::NotFound(id.to_string()))?;
        f(session);
        session.updated_at = chrono::Utc::now();
        self.persist(session)?;
        Ok(session.clone())
    }

    pub fn remove(&self, id: Ulid) -> Result<(), SessionError> {
        let mut sessions = self.sessions.write().unwrap();
        sessions.remove(&id);
        let path = self.dir.join(format!("{}.json", id));
        if path.exists() {
            std::fs::remove_file(&path)?;
        }
        Ok(())
    }

    fn persist(&self, session: &Session) -> Result<(), SessionError> {
        std::fs::create_dir_all(&self.dir)?;
        let path = self.dir.join(format!("{}.json", session.id));
        let tmp = path.with_extension("json.tmp");
        let content = serde_json::to_string_pretty(session)?;
        std::fs::write(&tmp, content)?;
        std::fs::rename(&tmp, &path)?;
        Ok(())
    }
}
```

Add to `crates/lgtm-session/src/lib.rs`:

```rust
pub mod store;
pub use store::SessionStore;
```

Add `NotFound(String)` variant to `SessionError` if not already present.

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p lgtm-session store`
Expected: all 7 tests pass

- [ ] **Step 5: Commit**

```bash
git add -A && git commit -m "feat(session): add SessionStore for multi-session management"
```

---

## Chunk 2: Server Multi-Session Routes

### Task 3: Refactor AppState to use SessionStore

**Files:**
- Modify: `crates/lgtm-server/src/lib.rs`
- Modify: `crates/lgtm-server/Cargo.toml`

- [ ] **Step 1: Update AppState struct**

Replace the current `AppState` in `crates/lgtm-server/src/lib.rs`:

```rust
pub struct AppState {
    pub store: Arc<SessionStore>,
    pub diff_providers: RwLock<HashMap<Ulid, Box<dyn DiffProvider>>>,
    pub broadcast_channels: RwLock<HashMap<Ulid, broadcast::Sender<ws::WsMessage>>>,
    pub submit_pending: RwLock<HashMap<Ulid, bool>>,
}

impl AppState {
    pub fn new(store: Arc<SessionStore>) -> Self {
        Self {
            store,
            diff_providers: RwLock::new(HashMap::new()),
            broadcast_channels: RwLock::new(HashMap::new()),
            submit_pending: RwLock::new(HashMap::new()),
        }
    }

    pub fn register_session(&self, session_id: Ulid, diff_provider: Box<dyn DiffProvider>) {
        let (tx, _) = broadcast::channel(64);
        self.diff_providers.write().unwrap().insert(session_id, diff_provider);
        self.broadcast_channels.write().unwrap().insert(session_id, tx);
        self.submit_pending.write().unwrap().insert(session_id, false);
    }

    pub fn unregister_session(&self, session_id: Ulid) {
        self.diff_providers.write().unwrap().remove(&session_id);
        self.broadcast_channels.write().unwrap().remove(&session_id);
        self.submit_pending.write().unwrap().remove(&session_id);
    }

    pub fn broadcast(&self, session_id: Ulid, msg: ws::WsMessage) {
        if let Some(tx) = self.broadcast_channels.read().unwrap().get(&session_id) {
            let _ = tx.send(msg);
        }
    }
}
```

**Note:** Do NOT commit this change alone — it will break compilation. Tasks 3, 4, and 5 must be committed together as a single atomic change.

### Task 4: Add session management routes (including patch_session)

**Files:**
- Create: `crates/lgtm-server/src/routes/sessions.rs`
- Modify: `crates/lgtm-server/src/routes/mod.rs`

- [ ] **Step 1: Write tests for session CRUD routes**

In `crates/lgtm-server/src/routes/sessions.rs`, add tests:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use tower::ServiceExt;

    // Helper to create test app — uses test_app() from parent module

    #[tokio::test]
    async fn test_create_session() {
        let app = test_app();
        let resp = app.oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/sessions")
                .header("content-type", "application/json")
                .body(Body::from(r#"{"repo_path":"/tmp/repo","base":"main"}"#))
                .unwrap(),
        ).await.unwrap();
        assert_eq!(resp.status(), StatusCode::CREATED);
    }

    #[tokio::test]
    async fn test_list_sessions() {
        let app = test_app();
        // create one first
        app.clone().oneshot(create_session_request("/tmp/repo", "main")).await.unwrap();
        let resp = app.oneshot(
            Request::builder().uri("/api/sessions").body(Body::empty()).unwrap()
        ).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_find_session_by_repo_and_head() {
        let app = test_app();
        app.clone().oneshot(create_session_request("/tmp/repo", "main")).await.unwrap();
        let resp = app.oneshot(
            Request::builder()
                .uri("/api/sessions?repo_path=/tmp/repo&head=feature/x")
                .body(Body::empty())
                .unwrap()
        ).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p lgtm-server sessions`
Expected: FAIL — module doesn't exist

- [ ] **Step 3: Implement session management routes**

In `crates/lgtm-server/src/routes/sessions.rs`:

```rust
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Json,
};
use serde::Deserialize;
use std::sync::Arc;
use ulid::Ulid;

use crate::AppState;
use lgtm_git::CliDiffProvider;

#[derive(Deserialize)]
pub struct CreateSession {
    pub repo_path: String,
    pub base: String,
}

#[derive(Deserialize)]
pub struct SessionQuery {
    pub repo_path: Option<String>,
    pub head: Option<String>,
}

pub async fn create_session(
    State(state): State<Arc<AppState>>,
    Json(body): Json<CreateSession>,
) -> Result<(StatusCode, Json<lgtm_session::Session>), (StatusCode, Json<serde_json::Value>)> {
    let repo_path = std::path::PathBuf::from(&body.repo_path);

    // Detect head ref from the repo
    let diff_provider = CliDiffProvider::new(&repo_path);
    let head = diff_provider.head_ref()
        .map_err(|e| (StatusCode::BAD_REQUEST, Json(serde_json::json!({"error": e.to_string()}))))?;

    // Check for existing session
    if let Some(existing) = state.store.find_by_repo_and_head(&repo_path, &head)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": e.to_string()}))))?
    {
        return Ok((StatusCode::OK, Json(existing)));
    }

    let merge_base = diff_provider.merge_base(&head, &body.base)
        .map_err(|e| (StatusCode::BAD_REQUEST, Json(serde_json::json!({"error": e.to_string()}))))?;

    let session = state.store.create(&body.base, &head, &merge_base, repo_path)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": e.to_string()}))))?;

    state.register_session(session.id, Box::new(diff_provider));

    Ok((StatusCode::CREATED, Json(session)))
}

pub async fn list_sessions(
    State(state): State<Arc<AppState>>,
    Query(query): Query<SessionQuery>,
) -> Json<Vec<lgtm_session::Session>> {
    let sessions = state.store.list();

    // Filter by repo_path and head if provided
    let filtered: Vec<_> = sessions.into_iter().filter(|s| {
        let repo_match = query.repo_path.as_ref()
            .map_or(true, |rp| s.repo_path == std::path::PathBuf::from(rp));
        let head_match = query.head.as_ref()
            .map_or(true, |h| s.head == *h);
        repo_match && head_match
    }).collect();

    Json(filtered)
}

pub async fn get_session(
    State(state): State<Arc<AppState>>,
    Path(session_id): Path<String>,
) -> Result<Json<lgtm_session::Session>, (StatusCode, Json<serde_json::Value>)> {
    let id = parse_session_id(&session_id)?;
    let session = state.store.get(id)
        .map_err(|_| (StatusCode::NOT_FOUND, Json(serde_json::json!({"error": "session not found"}))))?;
    Ok(Json(session))
}

pub async fn delete_session(
    State(state): State<Arc<AppState>>,
    Path(session_id): Path<String>,
) -> Result<StatusCode, (StatusCode, Json<serde_json::Value>)> {
    let id = parse_session_id(&session_id)?;
    state.unregister_session(id);
    state.store.remove(id)
        .map_err(|_| (StatusCode::NOT_FOUND, Json(serde_json::json!({"error": "session not found"}))))?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn patch_session(
    State(state): State<Arc<AppState>>,
    Path(session_id): Path<String>,
    Json(body): Json<PatchSession>,
) -> Result<Json<lgtm_session::Session>, (StatusCode, Json<serde_json::Value>)> {
    let id = parse_session_id(&session_id)?;
    let session = state.store.update(id, |s| {
        s.status = body.status.clone();
    }).map_err(|_| (StatusCode::NOT_FOUND, Json(serde_json::json!({"error": "session not found"}))))?;
    state.broadcast(id, crate::ws::WsMessage::SessionUpdated(session.clone()));
    Ok(Json(session))
}

#[derive(Deserialize)]
pub struct PatchSession {
    pub status: lgtm_session::SessionStatus,
}

fn parse_session_id(s: &str) -> Result<Ulid, (StatusCode, Json<serde_json::Value>)> {
    s.parse::<Ulid>()
        .map_err(|_| (StatusCode::BAD_REQUEST, Json(serde_json::json!({"error": "invalid session id"}))))
}
```

Also add a `test_app()` helper for route tests:

```rust
#[cfg(test)]
pub fn test_app() -> axum::Router {
    use lgtm_session::SessionStore;
    use std::sync::Arc;
    use tempfile::TempDir;

    let dir = TempDir::new().unwrap();
    let store = Arc::new(SessionStore::new(dir.path().to_path_buf()));
    let state = Arc::new(crate::AppState::new(store));
    crate::create_router(state)
}
```

- [ ] **Step 4: Run tests**

Run: `cargo test -p lgtm-server sessions`
Expected: PASS

**Note:** Do not commit yet — continue to Task 5 and commit all route changes together.

### Task 5: Migrate per-session routes to use session_id path param

**Files:**
- Modify: `crates/lgtm-server/src/routes/mod.rs`
- Modify: `crates/lgtm-server/src/routes/threads.rs`
- Modify: `crates/lgtm-server/src/routes/diff.rs`
- Modify: `crates/lgtm-server/src/routes/files.rs`
- Modify: `crates/lgtm-server/src/routes/submit.rs`
- Remove: `crates/lgtm-server/src/routes/assets.rs`
- Remove: `crates/lgtm-server/src/routes/session.rs` (replaced by sessions.rs)

- [ ] **Step 1: Create session extractor helper**

Add to `crates/lgtm-server/src/routes/mod.rs` a helper that extracts session_id from path and fetches the session from store. All per-session routes will use this pattern:

```rust
use ulid::Ulid;

pub(crate) fn parse_session_id(s: &str) -> Result<Ulid, (StatusCode, Json<serde_json::Value>)> {
    s.parse::<Ulid>()
        .map_err(|_| (StatusCode::BAD_REQUEST, Json(serde_json::json!({"error": "invalid session id"}))))
}
```

- [ ] **Step 2: Update route tree**

Restructure `crates/lgtm-server/src/routes/mod.rs`:

```rust
pub fn api_routes() -> Router<Arc<AppState>> {
    Router::new()
        // Session management
        .route("/sessions", post(sessions::create_session).get(sessions::list_sessions))
        .route("/sessions/{id}", get(sessions::get_session).patch(sessions::patch_session).delete(sessions::delete_session))
        // Per-session routes
        .route("/sessions/{id}/diff", get(diff::get_diff))
        .route("/sessions/{id}/threads", post(threads::create_thread))
        .route("/sessions/{id}/threads/{tid}/comments", post(threads::add_comment))
        .route("/sessions/{id}/threads/{tid}", patch(threads::patch_thread))
        .route("/sessions/{id}/files", patch(files::patch_file))
        .route("/sessions/{id}/submit", post(submit::post_submit).get(submit::get_submit))
}
```

- [ ] **Step 3: Update each route handler to extract session_id**

For each handler in threads.rs, diff.rs, files.rs, submit.rs:
- Change `State(state): State<Arc<AppState>>` stays the same
- Add `Path(session_id): Path<String>` (or `Path((session_id, thread_id)): Path<(String, String)>` for nested)
- Replace `state.session.read()` with `state.store.get(parse_session_id(&session_id)?)`
- Replace `state.session.write()` with `state.store.update(id, |session| { ... })`
- Replace `state.broadcast_tx.send()` with `state.broadcast(id, msg)`
- Replace `.submit` file marker with `state.submit_pending` HashMap

For submit.rs specifically:
```rust
pub async fn post_submit(
    State(state): State<Arc<AppState>>,
    Path(session_id): Path<String>,
) -> Result<(StatusCode, Json<SubmitResponse>), (StatusCode, Json<serde_json::Value>)> {
    let id = parse_session_id(&session_id)?;
    let mut pending = state.submit_pending.write().unwrap();
    let is_pending = pending.get(&id).copied().unwrap_or(false);
    if is_pending {
        return Err((StatusCode::CONFLICT, Json(serde_json::json!({"error": "already pending"}))));
    }
    pending.insert(id, true);
    drop(pending);
    state.broadcast(id, WsMessage::SubmitStatus(SubmitStatusData { pending: true }));
    Ok((StatusCode::CREATED, Json(SubmitResponse { pending: true })))
}

pub async fn get_submit(
    State(state): State<Arc<AppState>>,
    Path(session_id): Path<String>,
) -> Result<Json<SubmitResponse>, (StatusCode, Json<serde_json::Value>)> {
    let id = parse_session_id(&session_id)?;
    let pending = state.submit_pending.read().unwrap();
    let is_pending = pending.get(&id).copied().unwrap_or(false);
    Ok(Json(SubmitResponse { pending: is_pending }))
}
```

- [ ] **Step 4: Remove assets.rs and old session.rs**

Delete `crates/lgtm-server/src/routes/assets.rs` and `crates/lgtm-server/src/routes/session.rs`. Remove asset fallback from `create_router()`.

- [ ] **Step 5: Run tests**

Run: `cargo test -p lgtm-server`
Expected: PASS (some old tests may need updating for new route paths)

- [ ] **Step 6: Commit Tasks 3+4+5 together**

```bash
git add -A && git commit -m "refactor(server): multi-session AppState, routes, and API under /api/sessions/{id}/"
```

### Task 6: Update WebSocket to be per-session

**Files:**
- Modify: `crates/lgtm-server/src/ws.rs`

- [ ] **Step 1: Update ws_handler to accept session_id**

```rust
pub async fn ws_handler(
    ws: WebSocketUpgrade,
    Path(session_id): Path<String>,
    State(state): State<Arc<AppState>>,
) -> Result<Response, (StatusCode, Json<serde_json::Value>)> {
    let id = session_id.parse::<Ulid>()
        .map_err(|_| (StatusCode::BAD_REQUEST, Json(serde_json::json!({"error": "invalid session id"}))))?;
    // Verify session exists
    state.store.get(id)
        .map_err(|_| (StatusCode::NOT_FOUND, Json(serde_json::json!({"error": "session not found"}))))?;
    Ok(ws.on_upgrade(move |socket| handle_socket(socket, state, id)))
}
```

- [ ] **Step 2: Update handle_socket to subscribe to session-specific channel**

```rust
async fn handle_socket(socket: WebSocket, state: Arc<AppState>, session_id: Ulid) {
    let (mut sender, mut _receiver) = socket.split();

    // Send initial session state
    if let Ok(session) = state.store.get(session_id) {
        let msg = WsMessage::SessionUpdated(session);
        if let Ok(json) = serde_json::to_string(&msg) {
            let _ = sender.send(axum::extract::ws::Message::Text(json.into())).await;
        }
    }

    // Subscribe to session-specific broadcast
    let rx = {
        let channels = state.broadcast_channels.read().unwrap();
        channels.get(&session_id).map(|tx| tx.subscribe())
    };

    if let Some(mut rx) = rx {
        while let Ok(msg) = rx.recv().await {
            if let Ok(json) = serde_json::to_string(&msg) {
                if sender.send(axum::extract::ws::Message::Text(json.into())).await.is_err() {
                    break;
                }
            }
        }
    }
}
```

- [ ] **Step 3: Add WebSocket route to router**

In `crates/lgtm-server/src/routes/mod.rs`, add:
```rust
.route("/sessions/{id}/ws", get(ws::ws_handler))
```

- [ ] **Step 4: Run tests**

Run: `cargo test -p lgtm-server`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add -A && git commit -m "feat(server): per-session WebSocket connections"
```

---

## Chunk 3: File Watchers & Lockfile

### Task 7: Refactor file watchers for multi-session with deduplication

**Files:**
- Modify: `crates/lgtm-server/src/watcher.rs`

- [ ] **Step 1: Write test for watcher deduplication**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_watcher_registry_deduplication() {
        let registry = WatcherRegistry::new();
        let id1 = Ulid::new();
        let id2 = Ulid::new();
        let repo = PathBuf::from("/tmp/repo");

        registry.register(repo.clone(), id1);
        registry.register(repo.clone(), id2);
        assert_eq!(registry.repo_count(), 1); // only one watcher

        registry.unregister(&repo, id1);
        assert_eq!(registry.repo_count(), 1); // still watching for id2

        registry.unregister(&repo, id2);
        assert_eq!(registry.repo_count(), 0); // watcher dropped
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p lgtm-server watcher`
Expected: FAIL

- [ ] **Step 3: Implement WatcherRegistry**

```rust
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, RwLock};
use ulid::Ulid;

pub struct WatcherRegistry {
    watchers: RwLock<HashMap<PathBuf, WatcherEntry>>,
}

struct WatcherEntry {
    session_ids: Vec<Ulid>,
    // The watcher handle — dropping it stops the watcher
    _handle: Option<notify_debouncer_mini::Debouncer<notify::RecommendedWatcher>>,
}

impl WatcherRegistry {
    pub fn new() -> Self {
        Self { watchers: RwLock::new(HashMap::new()) }
    }

    pub fn register(&self, repo_path: PathBuf, session_id: Ulid) {
        let mut watchers = self.watchers.write().unwrap();
        let entry = watchers.entry(repo_path.clone()).or_insert_with(|| {
            WatcherEntry {
                session_ids: Vec::new(),
                _handle: None, // Will be set by start_watcher()
            }
        });
        if !entry.session_ids.contains(&session_id) {
            entry.session_ids.push(session_id);
        }
    }

    pub fn unregister(&self, repo_path: &PathBuf, session_id: Ulid) {
        let mut watchers = self.watchers.write().unwrap();
        if let Some(entry) = watchers.get_mut(repo_path) {
            entry.session_ids.retain(|id| *id != session_id);
            if entry.session_ids.is_empty() {
                watchers.remove(repo_path);
            }
        }
    }

    pub fn repo_count(&self) -> usize {
        self.watchers.read().unwrap().len()
    }

    pub fn session_ids_for_repo(&self, repo_path: &PathBuf) -> Vec<Ulid> {
        self.watchers.read().unwrap()
            .get(repo_path)
            .map(|e| e.session_ids.clone())
            .unwrap_or_default()
    }
}
```

Rewrite `start_watchers` to work with registry:
- Remove session.json watcher (no longer needed)
- Working tree watcher: on file change, look up all session_ids for that repo, broadcast DiffUpdated to each

- [ ] **Step 4: Run tests**

Run: `cargo test -p lgtm-server watcher`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add -A && git commit -m "refactor(server): deduplicated per-repo file watchers with WatcherRegistry"
```

### Task 8: Add lockfile management

**Files:**
- Create: `crates/lgtm-server/src/lockfile.rs`

- [ ] **Step 1: Write tests for lockfile**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_write_and_read_lockfile() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("server.json");
        write_lockfile(&path, 1234, 5678).unwrap();
        let info = read_lockfile(&path).unwrap().unwrap();
        assert_eq!(info.pid, 1234);
        assert_eq!(info.port, 5678);
    }

    #[test]
    fn test_read_missing_lockfile() {
        let info = read_lockfile(Path::new("/nonexistent/server.json")).unwrap();
        assert!(info.is_none());
    }

    #[test]
    fn test_remove_lockfile() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("server.json");
        write_lockfile(&path, 1234, 5678).unwrap();
        remove_lockfile(&path).unwrap();
        assert!(read_lockfile(&path).unwrap().is_none());
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p lgtm-server lockfile`
Expected: FAIL

- [ ] **Step 3: Implement lockfile module**

```rust
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Serialize, Deserialize)]
pub struct ServerInfo {
    pub pid: u32,
    pub port: u16,
}

pub fn lgtm_dir() -> std::path::PathBuf {
    dirs::home_dir().expect("no home directory").join(".lgtm")
}

pub fn lockfile_path() -> std::path::PathBuf {
    lgtm_dir().join("server.json")
}

pub fn sessions_dir() -> std::path::PathBuf {
    lgtm_dir().join("sessions")
}

pub fn write_lockfile(path: &Path, pid: u32, port: u16) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let info = ServerInfo { pid, port };
    let content = serde_json::to_string_pretty(&info).unwrap();
    std::fs::write(path, content)
}

pub fn read_lockfile(path: &Path) -> std::io::Result<Option<ServerInfo>> {
    match std::fs::read_to_string(path) {
        Ok(content) => {
            let info: ServerInfo = serde_json::from_str(&content)
                .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
            Ok(Some(info))
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e),
    }
}

pub fn remove_lockfile(path: &Path) -> std::io::Result<()> {
    match std::fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e),
    }
}

/// Check if a PID is alive
pub fn is_pid_alive(pid: u32) -> bool {
    unsafe { libc::kill(pid as i32, 0) == 0 }
}
```

Add `dirs = "6"` to workspace dependencies and `crates/lgtm-server/Cargo.toml`. Also add `libc` to `crates/lgtm-server/Cargo.toml` (already in workspace deps but not in this crate) for the `is_pid_alive` function.

- [ ] **Step 4: Run tests**

Run: `cargo test -p lgtm-server lockfile`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add -A && git commit -m "feat(server): add lockfile management for app discovery"
```

---

## Chunk 4: Tauri App

### Task 9: Scaffold Tauri app crate

**Files:**
- Create: `crates/lgtm-app/Cargo.toml`
- Create: `crates/lgtm-app/src/main.rs`
- Create: `crates/lgtm-app/tauri.conf.json`
- Create: `crates/lgtm-app/build.rs`
- Modify: `Cargo.toml` (workspace)

- [ ] **Step 1: Add Tauri dependencies to workspace**

In root `Cargo.toml`, add to `[workspace.dependencies]`:
```toml
tauri = { version = "2", features = ["devtools"] }
tauri-build = "2"
```

Add `lgtm-app` to workspace members (if not using glob, add explicitly).

- [ ] **Step 2: Create crate files**

`crates/lgtm-app/Cargo.toml`:
```toml
[package]
name = "lgtm-app"
version.workspace = true
edition.workspace = true
rust-version.workspace = true

[[bin]]
name = "lgtm-app"

[dependencies]
lgtm-server.workspace = true
lgtm-session.workspace = true
lgtm-git.workspace = true
tauri.workspace = true
tokio.workspace = true
axum.workspace = true
tracing.workspace = true
tracing-subscriber.workspace = true
serde_json.workspace = true
url = "2"

[build-dependencies]
tauri-build.workspace = true
```

`crates/lgtm-app/build.rs`:
```rust
fn main() {
    tauri_build::build()
}
```

`crates/lgtm-app/tauri.conf.json`:
```json
{
  "productName": "lgtm",
  "identifier": "dev.lgtm.app",
  "build": {
    "frontendDist": "../../packages/web/dist",
    "devUrl": "http://localhost:5173",
    "beforeDevCommand": "npm run dev --prefix ../../packages/web",
    "beforeBuildCommand": "npm run build --prefix ../../packages/web"
  },
  "app": {
    "title": "lgtm",
    "windows": [
      {
        "label": "main",
        "title": "lgtm",
        "width": 1200,
        "height": 800,
        "minWidth": 800,
        "minHeight": 600
      }
    ]
  }
}
```

- [ ] **Step 3: Implement main.rs**

`crates/lgtm-app/src/main.rs`:

```rust
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use lgtm_server::lockfile;
use lgtm_session::SessionStore;
use std::sync::Arc;

fn main() {
    tracing_subscriber::fmt::init();

    let store_dir = lockfile::sessions_dir();
    let store = Arc::new(SessionStore::new(store_dir));
    store.load().expect("failed to load sessions");

    let state = Arc::new(lgtm_server::AppState::new(store));

    // Channel to communicate port from server thread to main thread
    let (port_tx, port_rx) = std::sync::mpsc::channel();

    // Start Axum server on dynamic port in background thread
    let state_clone = state.clone();
    std::thread::spawn(move || {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let app = lgtm_server::create_router(state_clone);
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let port = listener.local_addr().unwrap().port();

            // Write lockfile
            let lockfile_path = lockfile::lockfile_path();
            lockfile::write_lockfile(&lockfile_path, std::process::id(), port)
                .expect("failed to write lockfile");

            tracing::info!("Server listening on 127.0.0.1:{}", port);

            // Send port to main thread before starting to serve
            port_tx.send(port).unwrap();

            axum::serve(listener, app).await.unwrap();
        });
    });

    // Wait for server to be ready and get the port
    let port = port_rx.recv().expect("failed to get server port");

    // Run Tauri app — setup() navigates webview to the Axum server
    tauri::Builder::default()
        .setup(move |app| {
            use tauri::Manager;
            let window = app.get_webview_window("main").unwrap();
            window.navigate(
                url::Url::parse(&format!("http://127.0.0.1:{}", port)).unwrap()
            )?;
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");

    // Cleanup lockfile on exit
    let _ = lockfile::remove_lockfile(&lockfile::lockfile_path());
}
```

**Note:** The `port_rx.recv()` blocks the main thread until the server thread binds and sends the port. This happens before `tauri::Builder::default().run()`, so Tauri starts only after the server is ready. The `setup()` callback then navigates the webview to the Axum server URL.

- [ ] **Step 4: Verify it compiles**

Run: `cargo check -p lgtm-app`
Expected: compiles (may have warnings)

- [ ] **Step 5: Commit**

```bash
git add -A && git commit -m "feat: scaffold Tauri app crate with embedded Axum server"
```

---

## Chunk 5: CLI Rewrite

### Task 11: Rewrite CLI as HTTP client

**Files:**
- Modify: `crates/lgtm-cli/Cargo.toml`
- Modify: `crates/lgtm-cli/src/main.rs`

- [ ] **Step 1: Update CLI dependencies**

In `crates/lgtm-cli/Cargo.toml`:
- Remove: `lgtm-assets`, `axum`, `notify`, `notify-debouncer-mini`, `open`
- Add: `reqwest = { version = "0.12", features = ["json", "blocking"] }`
- Add: `tungstenite = "0.26"` (for WebSocket in fetch command)
- Add: `which = "7"` (for finding lgtm-app binary)
- Keep: `clap`, `lgtm-session` (for types), `lgtm-git` (for repo detection), `serde_json`, `lgtm-server` (for lockfile module and ws::WsMessage types)

**Note:** The CLI no longer needs `#[tokio::main]` — switch to a plain `fn main()` since all HTTP calls use `reqwest::blocking::Client`. Only `lgtm fetch` uses WebSocket via `tungstenite` (also synchronous).

Retain the existing `find_repo_root()` helper from current main.rs. Add `git_head_ref()`:
```rust
fn git_head_ref(repo_path: &Path) -> Result<String, String> {
    let output = std::process::Command::new("git")
        .args(["symbolic-ref", "--short", "HEAD"])
        .current_dir(repo_path)
        .output()
        .map_err(|e| format!("git failed: {}", e))?;
    if !output.status.success() {
        return Err("failed to get HEAD ref".into());
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}
```

- [ ] **Step 2: Write test for server discovery**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_discover_server_from_lockfile() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("server.json");
        lgtm_server::lockfile::write_lockfile(&path, std::process::id(), 12345).unwrap();
        let info = lgtm_server::lockfile::read_lockfile(&path).unwrap().unwrap();
        assert_eq!(info.port, 12345);
    }
}
```

- [ ] **Step 3: Implement CLI client helper**

Add a `client.rs` module or inline in main.rs:

```rust
use lgtm_server::lockfile::{self, ServerInfo};

fn discover_server() -> Result<ServerInfo, String> {
    let path = lockfile::lockfile_path();
    match lockfile::read_lockfile(&path) {
        Ok(Some(info)) => {
            if lockfile::is_pid_alive(info.pid) {
                Ok(info)
            } else {
                let _ = lockfile::remove_lockfile(&path);
                Err("lgtm app not running (stale lockfile cleaned up)".into())
            }
        }
        Ok(None) => Err("lgtm app not running. Start it with: lgtm start".into()),
        Err(e) => Err(format!("failed to read lockfile: {}", e)),
    }
}

fn base_url(info: &ServerInfo) -> String {
    format!("http://127.0.0.1:{}", info.port)
}

fn resolve_session(client: &reqwest::blocking::Client, base: &str) -> Result<Session, String> {
    let repo_path = find_repo_root()?;
    let head = git_head_ref(&repo_path)?;
    let resp = client.get(format!("{}/api/sessions", base))
        .query(&[("repo_path", repo_path.to_str().unwrap()), ("head", &head)])
        .send()
        .map_err(|e| format!("failed to connect to lgtm app: {}", e))?;
    let sessions: Vec<Session> = resp.json()
        .map_err(|e| format!("bad response: {}", e))?;
    sessions.into_iter().next()
        .ok_or_else(|| "no active session for this repo/branch".into())
}
```

- [ ] **Step 4: Rewrite each CLI command**

**start:**
```rust
fn cmd_start(base: &str) -> Result<(), String> {
    let server = discover_server().or_else(|_| launch_app())?;
    let client = reqwest::blocking::Client::new();
    let repo_path = find_repo_root()?;
    let resp = client.post(format!("{}/api/sessions", base_url(&server)))
        .json(&serde_json::json!({ "repo_path": repo_path, "base": base }))
        .send()
        .map_err(|e| format!("failed to create session: {}", e))?;
    let session: Session = resp.json().map_err(|e| format!("bad response: {}", e))?;
    println!("Session {} started for {} ({})", session.id, session.head, session.base);
    Ok(())
}

fn launch_app() -> Result<ServerInfo, String> {
    // Try to find lgtm-app binary
    let app_path = which::which("lgtm-app")
        .map_err(|_| "lgtm app not installed. Install it from: https://github.com/yumike/lgtm/releases".to_string())?;

    std::process::Command::new(app_path)
        .spawn()
        .map_err(|e| format!("failed to launch lgtm app: {}", e))?;

    // Poll lockfile for up to 10 seconds
    let start = std::time::Instant::now();
    loop {
        if start.elapsed() > std::time::Duration::from_secs(10) {
            return Err("timed out waiting for lgtm app to start. Check if it launched correctly.".into());
        }
        std::thread::sleep(std::time::Duration::from_millis(200));
        if let Ok(Some(info)) = lockfile::read_lockfile(&lockfile::lockfile_path()) {
            if lockfile::is_pid_alive(info.pid) {
                return Ok(info);
            }
        }
    }
}
```

**status:**
```rust
fn cmd_status(json_output: bool) -> Result<(), String> {
    let server = discover_server()?;
    let client = reqwest::blocking::Client::new();
    let session = resolve_session(&client, &base_url(&server))?;
    if json_output {
        println!("{}", serde_json::to_string_pretty(&session).unwrap());
    } else {
        println!("Session: {}", session.id);
        println!("Status: {:?}", session.status);
        println!("Branch: {} → {}", session.head, session.base);
        let stats = lgtm_session::compute_stats(&session);
        println!("Threads: {} open, {} resolved", stats.open, stats.resolved);
    }
    Ok(())
}
```

**fetch:**
```rust
fn cmd_fetch(timeout_secs: Option<u64>) -> Result<(), String> {
    let server = discover_server()?;
    let client = reqwest::blocking::Client::new();
    let session = resolve_session(&client, &base_url(&server))?;

    let ws_url = format!("ws://127.0.0.1:{}/api/sessions/{}/ws", server.port, session.id);
    let (mut socket, _) = tungstenite::connect(&ws_url)
        .map_err(|e| format!("WebSocket connect failed: {}", e))?;

    let deadline = timeout_secs.map(|s| std::time::Instant::now() + std::time::Duration::from_secs(s));

    loop {
        if let Some(dl) = deadline {
            if std::time::Instant::now() > dl {
                return Err("timeout waiting for submission".into());
            }
        }

        let msg = socket.read().map_err(|e| format!("WebSocket read error: {}", e))?;
        if let tungstenite::Message::Text(text) = msg {
            if let Ok(ws_msg) = serde_json::from_str::<lgtm_server::ws::WsMessage>(&text) {
                match ws_msg {
                    lgtm_server::ws::WsMessage::SubmitStatus(data) if data.pending => {
                        // Fetch latest session state and print open threads
                        let session = resolve_session(&client, &base_url(&server))?;
                        println!("{}", serde_json::to_string_pretty(&session).unwrap());
                        return Ok(());
                    }
                    lgtm_server::ws::WsMessage::SessionUpdated(s) => {
                        if s.status != lgtm_session::SessionStatus::InProgress {
                            println!("{}", serde_json::to_string_pretty(&s).unwrap());
                            return Ok(());
                        }
                    }
                    _ => {}
                }
            }
        }
    }
}
```

**reply:**
```rust
fn cmd_reply(thread_id: &str, body: &str) -> Result<(), String> {
    let server = discover_server()?;
    let client = reqwest::blocking::Client::new();
    let session = resolve_session(&client, &base_url(&server))?;
    let resp = client.post(format!("{}/api/sessions/{}/threads/{}/comments",
            base_url(&server), session.id, thread_id))
        .json(&serde_json::json!({ "body": body, "author": "agent" }))
        .send()
        .map_err(|e| format!("failed to reply: {}", e))?;
    if !resp.status().is_success() {
        return Err(format!("reply failed: {}", resp.status()));
    }
    Ok(())
}
```

**thread:**
```rust
fn cmd_thread(file: &str, line: u32, line_end: Option<u32>, severity: &str, body: &str) -> Result<(), String> {
    let server = discover_server()?;
    let client = reqwest::blocking::Client::new();
    let session = resolve_session(&client, &base_url(&server))?;
    let resp = client.post(format!("{}/api/sessions/{}/threads", base_url(&server), session.id))
        .json(&serde_json::json!({
            "file": file,
            "line_start": line,
            "line_end": line_end.unwrap_or(line),
            "severity": severity,
            "body": body,
            "origin": "agent",
            "diff_side": "right",
            "anchor_context": ""
        }))
        .send()
        .map_err(|e| format!("failed to create thread: {}", e))?;
    if !resp.status().is_success() {
        return Err(format!("create thread failed: {}", resp.status()));
    }
    let thread: serde_json::Value = resp.json().map_err(|e| format!("bad response: {}", e))?;
    println!("{}", thread["id"].as_str().unwrap_or(""));
    Ok(())
}
```

**approve:**
```rust
fn cmd_approve() -> Result<(), String> {
    let server = discover_server()?;
    let client = reqwest::blocking::Client::new();
    let session = resolve_session(&client, &base_url(&server))?;
    let resp = client.patch(format!("{}/api/sessions/{}", base_url(&server), session.id))
        .json(&serde_json::json!({ "status": "approved" }))
        .send()
        .map_err(|e| format!("failed to approve: {}", e))?;
    if !resp.status().is_success() {
        let body: serde_json::Value = resp.json().unwrap_or_default();
        return Err(format!("approve failed: {}", body["error"].as_str().unwrap_or("unknown error")));
    }
    println!("Session approved.");
    Ok(())
}
```

**abandon, diff, clean** follow the same pattern.

- [ ] **Step 5: Update Commands enum**

```rust
#[derive(clap::Parser)]
#[command(name = "lgtm")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(clap::Subcommand)]
enum Commands {
    Start { #[arg(long, default_value = "main")] base: String },
    Status { #[arg(long)] json: bool },
    Fetch { #[arg(long)] timeout: Option<u64> },
    Reply { thread_id: String, body: Option<String>, #[arg(long)] stdin: bool },
    Thread {
        #[arg(long)] file: String,
        #[arg(long)] line: u32,
        #[arg(long)] line_end: Option<u32>,
        #[arg(long)] severity: String,
        body: Option<String>,
        #[arg(long)] stdin: bool,
    },
    Approve,
    Abandon,
    Diff { #[arg(long)] stat: bool },
    Clean,
}
```

- [ ] **Step 6: Run tests**

Run: `cargo test -p lgtm-cli`
Expected: PASS

- [ ] **Step 7: Commit**

```bash
git add -A && git commit -m "refactor(cli): rewrite as HTTP client against lgtm app"
```

---

## Chunk 6: Frontend Multi-Session Tabs

### Task 12: Update frontend types and API for multi-session

**Files:**
- Modify: `packages/web/src/lib/types.ts`
- Modify: `packages/web/src/lib/api.ts`
- Modify: `packages/web/src/lib/ws.ts`

- [ ] **Step 1: Add id and repo_path to Session type**

In `packages/web/src/lib/types.ts`:
```typescript
export interface Session {
    id: string;         // ULID
    repo_path: string;  // absolute path
    // ... existing fields
}
```

- [ ] **Step 2: Update API functions to accept session_id**

In `packages/web/src/lib/api.ts`, update all functions:

```typescript
function apiBase(sessionId: string) {
    return `/api/sessions/${sessionId}`;
}

export async function getSession(sessionId: string): Promise<Session> {
    return request(`${apiBase(sessionId)}`);
}

export async function patchSession(sessionId: string, status: SessionStatus): Promise<Session> {
    return request(`${apiBase(sessionId)}`, {
        method: 'PATCH',
        body: JSON.stringify({ status }),
    });
}

export async function getDiff(sessionId: string, file?: string): Promise<DiffFile[]> {
    const params = file ? `?file=${encodeURIComponent(file)}` : '';
    return request(`${apiBase(sessionId)}/diff${params}`);
}

export async function createThread(sessionId: string, params: CreateThreadParams): Promise<Thread> {
    return request(`${apiBase(sessionId)}/threads`, {
        method: 'POST',
        body: JSON.stringify(params),
    });
}

export async function addComment(sessionId: string, threadId: string, body: string): Promise<Comment> {
    return request(`${apiBase(sessionId)}/threads/${threadId}/comments`, {
        method: 'POST',
        body: JSON.stringify({ body }),
    });
}

export async function patchThread(sessionId: string, threadId: string, status: ThreadStatus): Promise<Thread> {
    return request(`${apiBase(sessionId)}/threads/${threadId}`, {
        method: 'PATCH',
        body: JSON.stringify({ status }),
    });
}

export async function patchFile(sessionId: string, path: string, status: FileReviewStatus): Promise<void> {
    return request(`${apiBase(sessionId)}/files?path=${encodeURIComponent(path)}`, {
        method: 'PATCH',
        body: JSON.stringify({ status }),
    });
}

export async function submitToAgent(sessionId: string): Promise<{ pending: boolean }> {
    return request(`${apiBase(sessionId)}/submit`, { method: 'POST' });
}

export async function getSubmitStatus(sessionId: string): Promise<{ pending: boolean }> {
    return request(`${apiBase(sessionId)}/submit`);
}

// New: session management
export async function listSessions(): Promise<Session[]> {
    return request('/api/sessions');
}

export async function createSession(repoPath: string, base: string): Promise<Session> {
    return request('/api/sessions', {
        method: 'POST',
        body: JSON.stringify({ repo_path: repoPath, base }),
    });
}

export async function deleteSession(sessionId: string): Promise<void> {
    return request(`/api/sessions/${sessionId}`, { method: 'DELETE' });
}
```

- [ ] **Step 3: Update WebSocket to include session_id in URL**

In `packages/web/src/lib/ws.ts`:

```typescript
export function createWsClient(
    sessionId: string,
    onMessage: (msg: WsMessage) => void,
    onResync: () => void,
): { stop: () => void } {
    const protocol = location.protocol === 'https:' ? 'wss:' : 'ws:';
    const url = `${protocol}//${location.host}/api/sessions/${sessionId}/ws`;
    // ... rest stays the same, just use the new url
}
```

- [ ] **Step 4: Verify TypeScript compiles**

Run: `cd packages/web && npm run check`
Expected: type errors in components (they still use old API signatures) — expected, fixed in next task

- [ ] **Step 5: Commit**

```bash
git add -A && git commit -m "feat(web): update types, API, and WebSocket for multi-session"
```

### Task 13: Create Shell and TabBar components

**Files:**
- Create: `packages/web/src/lib/components/Shell.svelte`
- Create: `packages/web/src/lib/components/TabBar.svelte`
- Create: `packages/web/src/lib/stores/sessions.ts`
- Modify: `packages/web/src/main.ts`

- [ ] **Step 1: Create sessions store**

`packages/web/src/lib/stores/sessions.ts`:

```typescript
import { writable } from 'svelte/store';
import type { Session } from '../types';

export const sessions = writable<Session[]>([]);
export const activeSessionId = writable<string | null>(null);
```

- [ ] **Step 2: Create TabBar component**

`packages/web/src/lib/components/TabBar.svelte`:

```svelte
<script lang="ts">
    import type { Session } from '../types';
    import { computeStats } from '../stats';

    interface Props {
        sessions: Session[];
        activeId: string | null;
        onselect: (id: string) => void;
        onclose: (id: string) => void;
    }

    let { sessions, activeId, onselect, onclose }: Props = $props();

    function label(session: Session): string {
        const parts = session.repo_path.split('/');
        const repo = parts[parts.length - 1] || 'unknown';
        return `${repo} / ${session.head}`;
    }

    function openCount(session: Session): number {
        return session.threads.filter(t => t.status === 'open').length;
    }
</script>

<div class="tab-bar">
    {#each sessions as session (session.id)}
        <button
            class="tab"
            class:active={session.id === activeId}
            onclick={() => onselect(session.id)}
        >
            <span class="tab-label">{label(session)}</span>
            {#if openCount(session) > 0}
                <span class="badge">{openCount(session)}</span>
            {/if}
            <button
                class="tab-close"
                onclick={(e: MouseEvent) => { e.stopPropagation(); onclose(session.id); }}
            >×</button>
        </button>
    {/each}
</div>

<style>
    .tab-bar {
        display: flex;
        background: #1e1e2e;
        border-bottom: 1px solid #313244;
        overflow-x: auto;
        flex-shrink: 0;
    }
    .tab {
        display: flex;
        align-items: center;
        gap: 6px;
        padding: 8px 12px;
        background: transparent;
        border: none;
        border-bottom: 2px solid transparent;
        color: #6c7086;
        cursor: pointer;
        font-size: 13px;
        white-space: nowrap;
    }
    .tab:hover { color: #cdd6f4; }
    .tab.active {
        color: #cdd6f4;
        border-bottom-color: #89b4fa;
    }
    .badge {
        background: #f38ba8;
        color: #1e1e2e;
        border-radius: 10px;
        padding: 1px 6px;
        font-size: 11px;
        font-weight: 600;
    }
    .tab-close {
        background: none;
        border: none;
        color: inherit;
        cursor: pointer;
        padding: 0 2px;
        font-size: 14px;
        opacity: 0.5;
    }
    .tab-close:hover { opacity: 1; }
</style>
```

- [ ] **Step 3: Create Shell component**

`packages/web/src/lib/components/Shell.svelte`:

```svelte
<script lang="ts">
    import { onMount } from 'svelte';
    import TabBar from './TabBar.svelte';
    import App from './App.svelte';
    import { listSessions } from '../api';
    import type { Session } from '../types';

    let sessions = $state<Session[]>([]);
    let activeSessionId = $state<string | null>(null);

    onMount(async () => {
        sessions = await listSessions();
        if (sessions.length > 0 && !activeSessionId) {
            activeSessionId = sessions[0].id;
        }
    });

    function handleSelect(id: string) {
        activeSessionId = id;
    }

    function handleClose(id: string) {
        sessions = sessions.filter(s => s.id !== id);
        if (activeSessionId === id) {
            activeSessionId = sessions.length > 0 ? sessions[0].id : null;
        }
    }

    // Listen for new sessions being created (via CLI)
    // Poll /api/sessions periodically to detect new sessions
    onMount(() => {
        const interval = setInterval(async () => {
            const latest = await listSessions();
            // Add any new sessions
            for (const s of latest) {
                if (!sessions.find(existing => existing.id === s.id)) {
                    sessions = [...sessions, s];
                    activeSessionId = s.id; // Focus new tab
                }
            }
        }, 2000);
        return () => clearInterval(interval);
    });
</script>

<div class="shell">
    {#if sessions.length > 0}
        <TabBar
            sessions={sessions}
            activeId={activeSessionId}
            onselect={handleSelect}
            onclose={handleClose}
        />
    {/if}

    {#if activeSessionId}
        {#each sessions as session (session.id)}
            <div class="tab-content" class:hidden={session.id !== activeSessionId}>
                <App sessionId={session.id} />
            </div>
        {/each}
    {:else}
        <div class="empty">
            <p>No active review sessions.</p>
            <p>Run <code>lgtm start</code> in a repo to begin.</p>
        </div>
    {/if}
</div>

<style>
    .shell {
        display: flex;
        flex-direction: column;
        height: 100vh;
    }
    .tab-content {
        flex: 1;
        overflow: hidden;
    }
    .tab-content.hidden {
        display: none;
    }
    .empty {
        display: flex;
        flex-direction: column;
        align-items: center;
        justify-content: center;
        height: 100%;
        color: #6c7086;
    }
    .empty code {
        background: #313244;
        padding: 2px 6px;
        border-radius: 4px;
    }
</style>
```

- [ ] **Step 4: Move and update App.svelte to accept sessionId prop**

First move: `mv packages/web/src/App.svelte packages/web/src/lib/components/App.svelte`

Then modify `packages/web/src/lib/components/App.svelte`:
- Add `interface Props { sessionId: string }` and `let { sessionId }: Props = $props()`
- Pass `sessionId` to all API calls and `createWsClient(sessionId, ...)`
- Remove global store usage, use local state per instance

- [ ] **Step 5: Update main.ts to mount Shell**

`packages/web/src/main.ts`:
```typescript
import Shell from './lib/components/Shell.svelte';
import { mount } from 'svelte';

mount(Shell, { target: document.getElementById('app')! });
```

- [ ] **Step 6: Add Vite proxy for development**

In `packages/web/vite.config.ts`, add a proxy so dev mode (`npm run dev` on port 5173) forwards API calls to the Axum server:

```typescript
server: {
    proxy: {
        '/api': {
            target: 'http://127.0.0.1:4567', // Read from env or lockfile in practice
            changeOrigin: true,
            ws: true, // Proxy WebSocket too
        },
    },
},
```

During development, read the port from `~/.lgtm/server.json` or use an env var.

- [ ] **Step 7: Verify frontend builds**

Run: `cd packages/web && npm run build`
Expected: builds successfully

- [ ] **Step 8: Commit**

```bash
git add -A && git commit -m "feat(web): add Shell with TabBar for multi-session UI"
```

---

## Chunk 7: Cleanup & Integration

### Task 14: Remove lgtm-assets crate

**Files:**
- Remove: `crates/lgtm-assets/`
- Modify: `Cargo.toml` (remove from workspace deps)
- Modify: `crates/lgtm-server/Cargo.toml` (remove dep)

- [ ] **Step 1: Remove the crate directory**

```bash
rm -rf crates/lgtm-assets
```

- [ ] **Step 2: Remove references from workspace Cargo.toml**

Remove `lgtm-assets = { path = "crates/lgtm-assets" }` from `[workspace.dependencies]`.

- [ ] **Step 3: Remove from lgtm-server Cargo.toml**

Remove `lgtm-assets` from `[dependencies]` in `crates/lgtm-server/Cargo.toml`.

- [ ] **Step 4: Verify asset serving code was removed from server**

`assets.rs` and `session.rs` should have been removed in Task 5. Verify that `mod assets` and the fallback route are gone from `create_router()`. Also remove `pub mod test_helpers;` from `crates/lgtm-server/src/lib.rs` (old test helpers were rewritten in Task 4).

- [ ] **Step 5: Verify workspace builds**

Run: `cargo check --workspace`
Expected: compiles

- [ ] **Step 6: Commit**

```bash
git add -A && git commit -m "chore: remove lgtm-assets crate (Tauri bundles frontend)"
```

### Task 15: Remove old session file I/O and lock from lgtm-session

**Files:**
- Modify: `crates/lgtm-session/src/lib.rs`

- [ ] **Step 1: Remove deprecated functions**

Remove from `crates/lgtm-session/src/lib.rs`:
- `read_session()` function
- `write_session()` function
- `write_session_atomic()` function
- `acquire_lock()` function
- `LockGuard` struct and its `Drop` impl
- Related `SessionError` variants that are no longer used (e.g., `LockTimeout`, `LockHeld`)

Keep:
- `Session` struct and its `new()` method
- All type definitions (Thread, Comment, etc.)
- `compute_stats()`
- `SessionStore` (in store.rs)

- [ ] **Step 2: Verify workspace builds**

Run: `cargo check --workspace`
Expected: compiles (no callers of removed functions should remain after CLI rewrite)

- [ ] **Step 3: Commit**

```bash
git add -A && git commit -m "chore(session): remove deprecated file I/O and advisory lock"
```

### Task 16: Update /lgtm skill for new CLI

**Files:**
- Modify: `plugins/lgtm/skills/lgtm/SKILL.md`

- [ ] **Step 1: Update skill documentation**

The skill loop stays the same but update any references to:
- Remove mentions of `.review/` directory
- Remove mentions of port conflicts
- Note that `lgtm start` now talks to the Tauri app
- Add `lgtm approve` and `lgtm abandon` commands
- Add `lgtm clean` command

- [ ] **Step 2: Commit**

```bash
git add -A && git commit -m "docs: update lgtm skill for Tauri app CLI"
```

### Task 17: End-to-end smoke test

- [ ] **Step 1: Build the full workspace**

```bash
cargo build --workspace
```

- [ ] **Step 2: Build frontend**

```bash
cd packages/web && npm run build
```

- [ ] **Step 3: Manual smoke test**

1. Run `lgtm-app` — verify window opens, lockfile created at `~/.lgtm/server.json`
2. In a git repo, run `lgtm start --base main` — verify session created, tab appears in app
3. In another repo, run `lgtm start --base main` — verify second tab appears
4. Run `lgtm status --json` — verify session JSON returned
5. Click on a line in the UI, add a comment, click "Submit to agent"
6. In another terminal: `lgtm fetch` — verify it unblocks and prints session
7. `lgtm reply <thread_id> "fixed"` — verify comment appears in UI
8. Mark all files reviewed, resolve all threads, click Approve
9. Close app — verify lockfile removed
10. Run `lgtm start` again — verify app relaunches

- [ ] **Step 4: Commit any fixes from smoke test**

```bash
git add -A && git commit -m "fix: address issues found in smoke testing"
```
