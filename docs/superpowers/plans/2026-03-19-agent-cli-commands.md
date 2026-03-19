# Agent-Facing CLI Commands Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add `lgtm reply`, `lgtm thread`, and `lgtm status --json` CLI commands so the AI agent can participate in review sessions without hand-editing JSON.

**Architecture:** Extend `lgtm-session` with `Stats`, `diff_snapshot` on `Comment`, atomic writes with advisory locking, and a `compute_stats` function. Extend `lgtm-cli` with three new subcommands that operate directly on `.review/session.json`. No server required.

**Tech Stack:** Rust 2024 edition, clap 4, serde, chrono, ulid. Existing workspace dependencies — no new crates needed.

**Spec:** `docs/superpowers/specs/2026-03-19-agent-cli-commands-design.md`

---

## File Structure

### Modified files

```
crates/lgtm-session/Cargo.toml      # No changes needed (already has all deps)
crates/lgtm-session/src/lib.rs       # Add Stats, diff_snapshot, compute_stats, atomic write, lock
crates/lgtm-cli/Cargo.toml           # Add ulid, serde_json deps
crates/lgtm-cli/src/main.rs          # Add Reply, Thread, Status subcommands
```

---

## Chunk 1: Prerequisite Type Changes in lgtm-session

### Task 1: Add `diff_snapshot` to `Comment`

**Files:**
- Modify: `crates/lgtm-session/src/lib.rs:76-82`

- [ ] **Step 1: Write test for diff_snapshot serialization**

Add to the existing `tests` module in `lib.rs`:

```rust
#[test]
fn test_comment_diff_snapshot_roundtrip() {
    let comment = Comment {
        id: "c_test".into(),
        author: Author::Agent,
        body: "Fixed it".into(),
        timestamp: chrono::Utc::now(),
        diff_snapshot: Some("abc1234".into()),
    };
    let json = serde_json::to_string(&comment).unwrap();
    assert!(json.contains("\"diff_snapshot\":\"abc1234\""));
    let deserialized: Comment = serde_json::from_str(&json).unwrap();
    assert_eq!(deserialized.diff_snapshot, Some("abc1234".into()));
}

#[test]
fn test_comment_without_diff_snapshot_defaults_to_none() {
    let json = r#"{
        "id": "c_test",
        "author": "developer",
        "body": "hello",
        "timestamp": "2026-03-18T14:22:00Z"
    }"#;
    let comment: Comment = serde_json::from_str(json).unwrap();
    assert_eq!(comment.diff_snapshot, None);
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p lgtm-session test_comment_diff_snapshot`
Expected: Compilation error — `diff_snapshot` field doesn't exist

- [ ] **Step 3: Add `diff_snapshot` field to `Comment`**

Change the `Comment` struct to:

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Comment {
    pub id: String,
    pub author: Author,
    pub body: String,
    pub timestamp: DateTime<Utc>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub diff_snapshot: Option<String>,
}
```

- [ ] **Step 4: Fix existing code that constructs `Comment`**

The server's `threads.rs` constructs `Comment` in two places. Add `diff_snapshot: None` to both:

In `crates/lgtm-server/src/routes/threads.rs` `create_thread` function (around line 47-52):
```rust
comments: vec![Comment {
    id: ulid::Ulid::new().to_string(),
    author,
    body: body.body,
    timestamp: now,
    diff_snapshot: None,
}],
```

In `add_comment` function (around line 82-87):
```rust
let comment = Comment {
    id: ulid::Ulid::new().to_string(),
    author: Author::Developer,
    body: body.body,
    timestamp: now,
    diff_snapshot: None,
};
```

- [ ] **Step 5: Run all tests to verify they pass**

Run: `cargo test -p lgtm-session && cargo test -p lgtm-server`
Expected: All pass

- [ ] **Step 6: Commit**

```bash
git add crates/lgtm-session/src/lib.rs crates/lgtm-server/src/routes/threads.rs
git commit -m "feat(session): add diff_snapshot field to Comment"
```

### Task 2: Add `Stats` struct and `compute_stats`

**Files:**
- Modify: `crates/lgtm-session/src/lib.rs`

- [ ] **Step 1: Write test for compute_stats**

```rust
#[test]
fn test_compute_stats_empty() {
    let session = Session::new("main", "feature/test", "abc1234");
    let stats = compute_stats(&session);
    assert_eq!(stats.total_threads, 0);
    assert_eq!(stats.open, 0);
    assert_eq!(stats.resolved, 0);
    assert_eq!(stats.wontfix, 0);
    assert_eq!(stats.dismissed, 0);
    assert_eq!(stats.agent_initiated, 0);
}

#[test]
fn test_compute_stats_mixed_threads() {
    let mut session = Session::new("main", "feature/test", "abc1234");
    let make_thread = |status: ThreadStatus, origin: Origin| Thread {
        id: ulid::Ulid::new().to_string(),
        origin,
        severity: if origin == Origin::Agent { Some(Severity::Warning) } else { None },
        status,
        file: "test.rs".into(),
        line_start: 1,
        line_end: 1,
        diff_side: DiffSide::Right,
        anchor_context: "test".into(),
        comments: vec![],
    };
    session.threads.push(make_thread(ThreadStatus::Open, Origin::Developer));
    session.threads.push(make_thread(ThreadStatus::Resolved, Origin::Developer));
    session.threads.push(make_thread(ThreadStatus::Wontfix, Origin::Developer));
    session.threads.push(make_thread(ThreadStatus::Open, Origin::Agent));
    session.threads.push(make_thread(ThreadStatus::Dismissed, Origin::Agent));

    let stats = compute_stats(&session);
    assert_eq!(stats.total_threads, 5);
    assert_eq!(stats.open, 2);
    assert_eq!(stats.resolved, 1);
    assert_eq!(stats.wontfix, 1);
    assert_eq!(stats.dismissed, 1);
    assert_eq!(stats.agent_initiated, 2);
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p lgtm-session test_compute_stats`
Expected: Compilation error — `Stats` and `compute_stats` don't exist

- [ ] **Step 3: Implement Stats and compute_stats**

Add above the `impl Session` block:

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Stats {
    pub total_threads: usize,
    pub open: usize,
    pub resolved: usize,
    pub wontfix: usize,
    pub dismissed: usize,
    pub agent_initiated: usize,
}

pub fn compute_stats(session: &Session) -> Stats {
    let mut stats = Stats {
        total_threads: session.threads.len(),
        open: 0,
        resolved: 0,
        wontfix: 0,
        dismissed: 0,
        agent_initiated: 0,
    };
    for thread in &session.threads {
        match thread.status {
            ThreadStatus::Open => stats.open += 1,
            ThreadStatus::Resolved => stats.resolved += 1,
            ThreadStatus::Wontfix => stats.wontfix += 1,
            ThreadStatus::Dismissed => stats.dismissed += 1,
        }
        if thread.origin == Origin::Agent {
            stats.agent_initiated += 1;
        }
    }
    stats
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p lgtm-session test_compute_stats`
Expected: All pass

- [ ] **Step 5: Commit**

```bash
git add crates/lgtm-session/src/lib.rs
git commit -m "feat(session): add Stats struct and compute_stats function"
```

### Task 3: Atomic writes with advisory locking

**Files:**
- Modify: `crates/lgtm-session/src/lib.rs`

- [ ] **Step 1: Write tests for atomic write and locking**

```rust
#[test]
fn test_write_session_atomic_creates_file() {
    let dir = tempfile::TempDir::new().unwrap();
    let path = dir.path().join(".review").join("session.json");
    let session = Session::new("main", "feature/test", "abc1234");
    write_session_atomic(&path, &session).unwrap();
    assert!(path.exists());
    // tmp file should not remain
    assert!(!path.with_extension("json.tmp").exists());
    let loaded = read_session(&path).unwrap();
    assert_eq!(loaded.base, "main");
}

#[test]
fn test_write_session_atomic_replaces_existing() {
    let dir = tempfile::TempDir::new().unwrap();
    let path = dir.path().join(".review").join("session.json");
    let session1 = Session::new("main", "feature/a", "abc");
    write_session_atomic(&path, &session1).unwrap();
    let session2 = Session::new("main", "feature/b", "def");
    write_session_atomic(&path, &session2).unwrap();
    let loaded = read_session(&path).unwrap();
    assert_eq!(loaded.head, "feature/b");
}

#[test]
fn test_lock_and_unlock() {
    let dir = tempfile::TempDir::new().unwrap();
    let lock_path = dir.path().join(".review").join(".lock");
    std::fs::create_dir_all(lock_path.parent().unwrap()).unwrap();
    let guard = acquire_lock(&lock_path).unwrap();
    assert!(lock_path.exists());
    drop(guard);
    assert!(!lock_path.exists());
}

#[test]
fn test_stale_lock_is_stolen() {
    let dir = tempfile::TempDir::new().unwrap();
    let lock_path = dir.path().join(".review").join(".lock");
    std::fs::create_dir_all(lock_path.parent().unwrap()).unwrap();
    // Write a lock with a PID that doesn't exist
    std::fs::write(&lock_path, "999999999").unwrap();
    let guard = acquire_lock(&lock_path).unwrap();
    assert!(lock_path.exists());
    drop(guard);
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p lgtm-session test_write_session_atomic test_lock`
Expected: Compilation error — functions don't exist

- [ ] **Step 3: Implement locking and atomic write**

Add a `SessionError` variant for lock failure:

```rust
#[derive(Debug, thiserror::Error)]
pub enum SessionError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("Lock error: {0}")]
    Lock(String),
}
```

Add lock guard struct and functions:

```rust
pub struct LockGuard {
    path: std::path::PathBuf,
}

impl Drop for LockGuard {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

pub fn acquire_lock(lock_path: &Path) -> Result<LockGuard, SessionError> {
    let max_attempts = 40; // 40 * 50ms = 2s
    for _ in 0..max_attempts {
        // Try to detect stale lock
        if lock_path.exists() {
            if let Ok(contents) = std::fs::read_to_string(lock_path) {
                if let Ok(pid) = contents.trim().parse::<u32>() {
                    // Check if process is alive (kill 0 = check existence)
                    let alive = unsafe { libc::kill(pid as i32, 0) } == 0;
                    if !alive {
                        // Stale lock, steal it
                        let _ = std::fs::remove_file(lock_path);
                    }
                }
            }
        }

        // Try to create lock file exclusively
        match std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(lock_path)
        {
            Ok(mut f) => {
                use std::io::Write;
                let _ = write!(f, "{}", std::process::id());
                return Ok(LockGuard { path: lock_path.to_path_buf() });
            }
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                std::thread::sleep(std::time::Duration::from_millis(50));
            }
            Err(e) => return Err(SessionError::Io(e)),
        }
    }
    Err(SessionError::Lock("Timed out waiting for lock".into()))
}

pub fn write_session_atomic(path: &Path, session: &Session) -> Result<(), SessionError> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let tmp_path = path.with_extension("json.tmp");
    let json = serde_json::to_string_pretty(session)?;
    std::fs::write(&tmp_path, json)?;
    std::fs::rename(&tmp_path, path)?;
    Ok(())
}
```

- [ ] **Step 4: Add `libc` dependency to lgtm-session**

In `Cargo.toml` for workspace root, add:
```toml
libc = "0.2"
```

In `crates/lgtm-session/Cargo.toml`, add under `[dependencies]`:
```toml
libc = { workspace = true }
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test -p lgtm-session`
Expected: All pass (including all existing tests)

- [ ] **Step 6: Commit**

```bash
git add Cargo.toml crates/lgtm-session/Cargo.toml crates/lgtm-session/src/lib.rs
git commit -m "feat(session): add atomic writes with advisory locking"
```

---

## Chunk 2: CLI Subcommands

### Task 4: Add `lgtm status --json`

**Files:**
- Modify: `crates/lgtm-cli/Cargo.toml`
- Modify: `crates/lgtm-cli/src/main.rs`

- [ ] **Step 1: Add dependencies to lgtm-cli**

In `crates/lgtm-cli/Cargo.toml`, add to `[dependencies]`:
```toml
serde_json = { workspace = true }
```

- [ ] **Step 2: Add Status subcommand to Commands enum**

In `main.rs`, update the `Commands` enum:

```rust
#[derive(clap::Subcommand)]
enum Commands {
    /// Start a review session
    Start {
        #[arg(long, default_value = "main")]
        base: String,
        #[arg(long, default_value = "4567")]
        port: u16,
        #[arg(long, default_value = "127.0.0.1")]
        host: String,
        #[arg(long)]
        no_open: bool,
    },
    /// Show review session status
    Status {
        /// Output as JSON (required for now)
        #[arg(long)]
        json: bool,
    },
}
```

Update the `match` in `main`:

```rust
match cli.command {
    Commands::Start { base, port, host, no_open } => start(base, port, host, no_open).await?,
    Commands::Status { json } => status(json)?,
}
```

- [ ] **Step 3: Implement `status` function**

Add to `main.rs`. The output struct is local to the CLI — it's a view over `Session`, not a domain type:

```rust
fn status(json: bool) -> Result<()> {
    if !json {
        bail!("Only --json output is currently supported. Usage: lgtm status --json");
    }

    let repo_path = find_repo_root()?;
    let session_path = repo_path.join(".review").join("session.json");

    if !session_path.exists() {
        std::process::exit(2);
    }

    let session = lgtm_session::read_session(&session_path)
        .context("Failed to read session")?;

    let stats = lgtm_session::compute_stats(&session);

    let open_threads: Vec<&lgtm_session::Thread> = session
        .threads
        .iter()
        .filter(|t| t.status == lgtm_session::ThreadStatus::Open)
        .collect();

    let output = serde_json::json!({
        "session_status": session.status,
        "base": session.base,
        "head": session.head,
        "stats": stats,
        "open_threads": open_threads,
    });

    println!("{}", serde_json::to_string_pretty(&output)?);
    Ok(())
}
```

- [ ] **Step 4: Verify it compiles**

Run: `cargo build -p lgtm`
Expected: Compiles without errors

- [ ] **Step 5: Commit**

```bash
git add crates/lgtm-cli/Cargo.toml crates/lgtm-cli/src/main.rs
git commit -m "feat(cli): add lgtm status --json command"
```

### Task 5: Add `lgtm reply`

**Files:**
- Modify: `crates/lgtm-cli/src/main.rs`

- [ ] **Step 1: Add Reply subcommand to Commands enum**

```rust
/// Reply to a review thread
Reply {
    /// Thread ID (e.g., t_01J8XYZABC)
    thread_id: String,
    /// Comment body (omit to read from --stdin)
    body: Option<String>,
    /// Read body from stdin
    #[arg(long)]
    stdin: bool,
},
```

Update the `match`:

```rust
Commands::Reply { thread_id, body, stdin } => reply(thread_id, body, stdin)?,
```

- [ ] **Step 2: Implement `reply` function**

```rust
fn reply(thread_id: String, body: Option<String>, stdin: bool) -> Result<()> {
    let body = read_body(body, stdin)?;

    let repo_path = find_repo_root()?;
    let session_path = repo_path.join(".review").join("session.json");
    let lock_path = repo_path.join(".review").join(".lock");

    if !session_path.exists() {
        std::process::exit(2);
    }

    let _lock = lgtm_session::acquire_lock(&lock_path)
        .context("Failed to acquire lock")?;

    let mut session = lgtm_session::read_session(&session_path)
        .context("Failed to read session")?;

    if session.status != SessionStatus::InProgress {
        eprintln!("Error: session is not active (status: {:?})", session.status);
        std::process::exit(6);
    }

    let thread = session.threads.iter_mut().find(|t| t.id == thread_id);
    let Some(thread) = thread else {
        eprintln!("Error: thread not found: {thread_id}");
        std::process::exit(4);
    };

    let head = git_head(&repo_path)?;

    let comment = lgtm_session::Comment {
        id: ulid::Ulid::new().to_string(),
        author: lgtm_session::Author::Agent,
        body,
        timestamp: chrono::Utc::now(),
        diff_snapshot: Some(head),
    };

    thread.comments.push(comment);
    session.updated_at = chrono::Utc::now();

    lgtm_session::write_session_atomic(&session_path, &session)
        .context("Failed to write session")?;

    Ok(())
}
```

- [ ] **Step 3: Add helper functions**

```rust
fn read_body(body: Option<String>, stdin: bool) -> Result<String> {
    if stdin {
        use std::io::Read;
        let mut buf = String::new();
        std::io::stdin().read_to_string(&mut buf)?;
        Ok(buf.trim().to_string())
    } else if let Some(body) = body {
        Ok(body)
    } else {
        bail!("Provide body as argument or use --stdin");
    }
}

fn git_head(repo_path: &std::path::Path) -> Result<String> {
    let output = std::process::Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(repo_path)
        .output()
        .context("Failed to run git rev-parse HEAD")?;
    if !output.status.success() {
        bail!("git rev-parse HEAD failed");
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}
```

- [ ] **Step 4: Add `ulid` dependency to lgtm-cli**

In `crates/lgtm-cli/Cargo.toml`, add:
```toml
ulid = { workspace = true }
```

- [ ] **Step 5: Verify it compiles**

Run: `cargo build -p lgtm`
Expected: Compiles without errors

- [ ] **Step 6: Commit**

```bash
git add crates/lgtm-cli/Cargo.toml crates/lgtm-cli/src/main.rs
git commit -m "feat(cli): add lgtm reply command"
```

### Task 6: Add `lgtm thread`

**Files:**
- Modify: `crates/lgtm-cli/src/main.rs`

- [ ] **Step 1: Add Thread subcommand to Commands enum**

```rust
/// Create an agent-initiated review thread
Thread {
    /// File path relative to repo root
    #[arg(long)]
    file: String,
    /// Start line number (1-indexed)
    #[arg(long)]
    line: u32,
    /// End line number (defaults to --line)
    #[arg(long)]
    line_end: Option<u32>,
    /// Severity: critical, warning, or info
    #[arg(long)]
    severity: String,
    /// Observation body (omit to read from --stdin)
    body: Option<String>,
    /// Read body from stdin
    #[arg(long)]
    stdin: bool,
},
```

Update the `match`:

```rust
Commands::Thread { file, line, line_end, severity, body, stdin } => {
    create_thread(file, line, line_end, severity, body, stdin)?
}
```

- [ ] **Step 2: Implement `create_thread` function**

```rust
fn create_thread(
    file: String,
    line: u32,
    line_end: Option<u32>,
    severity: String,
    body: Option<String>,
    stdin: bool,
) -> Result<()> {
    let body = read_body(body, stdin)?;
    let line_end = line_end.unwrap_or(line);

    let severity = match severity.as_str() {
        "critical" => lgtm_session::Severity::Critical,
        "warning" => lgtm_session::Severity::Warning,
        "info" => lgtm_session::Severity::Info,
        other => bail!("Invalid severity: {other}. Must be critical, warning, or info"),
    };

    let repo_path = find_repo_root()?;
    let session_path = repo_path.join(".review").join("session.json");
    let lock_path = repo_path.join(".review").join(".lock");

    if !session_path.exists() {
        std::process::exit(2);
    }

    let _lock = lgtm_session::acquire_lock(&lock_path)
        .context("Failed to acquire lock")?;

    let mut session = lgtm_session::read_session(&session_path)
        .context("Failed to read session")?;

    if session.status != SessionStatus::InProgress {
        eprintln!("Error: session is not active (status: {:?})", session.status);
        std::process::exit(6);
    }

    // Read anchor context from the actual file
    let file_path = repo_path.join(&file);
    if !file_path.exists() {
        eprintln!("Error: file not found: {file}");
        std::process::exit(5);
    }

    let contents = std::fs::read_to_string(&file_path)
        .context("Failed to read file")?;
    let lines: Vec<&str> = contents.lines().collect();

    if line == 0 || line as usize > lines.len() {
        eprintln!("Error: line {line} out of range (file has {} lines)", lines.len());
        std::process::exit(5);
    }

    let anchor_context = lines[(line - 1) as usize].to_string();
    let head = git_head(&repo_path)?;
    let thread_id = ulid::Ulid::new().to_string();

    let thread = lgtm_session::Thread {
        id: thread_id.clone(),
        origin: lgtm_session::Origin::Agent,
        severity: Some(severity),
        status: lgtm_session::ThreadStatus::Open,
        file,
        line_start: line,
        line_end,
        diff_side: lgtm_session::DiffSide::Right,
        anchor_context,
        comments: vec![lgtm_session::Comment {
            id: ulid::Ulid::new().to_string(),
            author: lgtm_session::Author::Agent,
            body,
            timestamp: chrono::Utc::now(),
            diff_snapshot: Some(head),
        }],
    };

    session.threads.push(thread);
    session.updated_at = chrono::Utc::now();

    lgtm_session::write_session_atomic(&session_path, &session)
        .context("Failed to write session")?;

    println!("{thread_id}");
    Ok(())
}
```

- [ ] **Step 3: Verify it compiles**

Run: `cargo build -p lgtm`
Expected: Compiles without errors

- [ ] **Step 4: Commit**

```bash
git add crates/lgtm-cli/src/main.rs
git commit -m "feat(cli): add lgtm thread command"
```

---

## Chunk 3: Integration Testing

### Task 7: Manual smoke test

- [ ] **Step 1: Run all unit tests**

Run: `cargo test`
Expected: All pass

- [ ] **Step 2: Create a test session**

```bash
mkdir -p /tmp/lgtm-test && cd /tmp/lgtm-test
git init && echo "fn main() { println!(\"hello\"); }" > main.rs && git add . && git commit -m "init"
mkdir -p .review
# Create a session with one open thread
cat > .review/session.json << 'EOF'
{
  "version": 1,
  "status": "in_progress",
  "base": "main",
  "head": "main",
  "merge_base": "abc1234",
  "created_at": "2026-03-18T14:00:00Z",
  "updated_at": "2026-03-18T14:00:00Z",
  "threads": [
    {
      "id": "t_TEST001",
      "origin": "developer",
      "status": "open",
      "file": "main.rs",
      "line_start": 1,
      "line_end": 1,
      "diff_side": "right",
      "anchor_context": "fn main() {",
      "comments": [
        {
          "id": "c_TEST001",
          "author": "developer",
          "body": "Add error handling",
          "timestamp": "2026-03-18T14:22:00Z"
        }
      ]
    }
  ],
  "files": {}
}
EOF
```

- [ ] **Step 3: Test `lgtm status --json`**

Run from the test repo: `cargo run -p lgtm -- status --json`
Expected: JSON output with `session_status`, `stats`, and 1 open thread

- [ ] **Step 4: Test `lgtm reply`**

Run: `cargo run -p lgtm -- reply t_TEST001 "Added error handling with anyhow"`
Then: `cat .review/session.json | python3 -m json.tool`
Expected: Thread t_TEST001 now has 2 comments. Second comment has `author: "agent"` and a `diff_snapshot`.

- [ ] **Step 5: Test `lgtm thread`**

Run: `cargo run -p lgtm -- thread --file main.rs --line 1 --severity warning "Missing return type annotation"`
Expected: Prints a new thread ID to stdout. `session.json` has 2 threads.

- [ ] **Step 6: Test error cases**

```bash
# No session
cd /tmp && cargo run -p lgtm -- status --json
# Expected: exit code 2

# Thread not found
cd /tmp/lgtm-test && cargo run -p lgtm -- reply nonexistent "test"
# Expected: exit code 4

# File not found
cargo run -p lgtm -- thread --file nonexistent.rs --line 1 --severity info "test"
# Expected: exit code 5

# Line out of range
cargo run -p lgtm -- thread --file main.rs --line 999 --severity info "test"
# Expected: exit code 5
```

- [ ] **Step 7: Fix any issues found and commit**

```bash
git add -A
git commit -m "fix(cli): address issues found in smoke testing"
```
