pub mod store;
pub use store::SessionStore;

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    pub id: ulid::Ulid,
    pub version: u32,
    pub status: SessionStatus,
    pub repo_path: PathBuf,
    pub base: String,
    pub head: String,
    pub merge_base: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub threads: Vec<Thread>,
    pub files: HashMap<String, FileReviewStatus>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionStatus {
    InProgress,
    Approved,
    Abandoned,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Thread {
    pub id: String,
    #[serde(default)]
    pub origin: Origin,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub severity: Option<Severity>,
    pub status: ThreadStatus,
    pub file: String,
    pub line_start: u32,
    pub line_end: u32,
    pub diff_side: DiffSide,
    pub anchor_context: String,
    pub comments: Vec<Comment>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum Origin {
    #[default]
    Developer,
    Agent,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    Critical,
    Warning,
    Info,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ThreadStatus {
    Open,
    Resolved,
    Wontfix,
    Dismissed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DiffSide {
    Left,
    Right,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Comment {
    pub id: String,
    pub author: Author,
    pub body: String,
    pub timestamp: DateTime<Utc>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub diff_snapshot: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Author {
    Developer,
    Agent,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FileReviewStatus {
    Pending,
    Reviewed,
}

#[derive(Debug, thiserror::Error)]
pub enum SessionError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("Lock error: {0}")]
    Lock(String),
    #[error("Not found: {0}")]
    NotFound(String),
}

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

impl Session {
    pub fn new(base: &str, head: &str, merge_base: &str, repo_path: PathBuf) -> Self {
        let now = Utc::now();
        Self {
            id: ulid::Ulid::new(),
            version: 1,
            status: SessionStatus::InProgress,
            repo_path,
            base: base.into(),
            head: head.into(),
            merge_base: merge_base.into(),
            created_at: now,
            updated_at: now,
            threads: vec![],
            files: HashMap::new(),
        }
    }
}

pub fn read_session(path: &Path) -> Result<Session, SessionError> {
    let contents = std::fs::read_to_string(path)?;
    let session = serde_json::from_str(&contents)?;
    Ok(session)
}

pub fn write_session(path: &Path, session: &Session) -> Result<(), SessionError> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let json = serde_json::to_string_pretty(session)?;
    std::fs::write(path, json)?;
    Ok(())
}

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
        if lock_path.exists() {
            if let Ok(contents) = std::fs::read_to_string(lock_path) {
                if let Ok(pid) = contents.trim().parse::<u32>() {
                    let alive = unsafe { libc::kill(pid as i32, 0) } == 0;
                    if !alive {
                        let _ = std::fs::remove_file(lock_path);
                    }
                }
            }
        }

        match std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(lock_path)
        {
            Ok(mut f) => {
                use std::io::Write;
                let _ = write!(f, "{}", std::process::id());
                return Ok(LockGuard {
                    path: lock_path.to_path_buf(),
                });
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

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    #[test]
    fn test_session_roundtrip() {
        let session = Session {
            id: ulid::Ulid::new(),
            version: 1,
            status: SessionStatus::InProgress,
            repo_path: PathBuf::from("/tmp/test"),
            base: "main".into(),
            head: "feature/test".into(),
            merge_base: "abc1234".into(),
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
            threads: vec![],
            files: std::collections::HashMap::new(),
        };
        let json = serde_json::to_string_pretty(&session).unwrap();
        let deserialized: Session = serde_json::from_str(&json).unwrap();
        assert_eq!(session.version, deserialized.version);
        assert_eq!(session.status, deserialized.status);
        assert_eq!(session.base, deserialized.base);
    }

    #[test]
    fn test_session_status_serializes_as_snake_case() {
        let json = serde_json::to_string(&SessionStatus::InProgress).unwrap();
        assert_eq!(json, "\"in_progress\"");
    }

    #[test]
    fn test_thread_status_includes_dismissed() {
        let json = serde_json::to_string(&ThreadStatus::Dismissed).unwrap();
        assert_eq!(json, "\"dismissed\"");
    }

    #[test]
    fn test_file_status_serializes() {
        let json = serde_json::to_string(&FileReviewStatus::Pending).unwrap();
        assert_eq!(json, "\"pending\"");
        let json = serde_json::to_string(&FileReviewStatus::Reviewed).unwrap();
        assert_eq!(json, "\"reviewed\"");
    }

    #[test]
    fn test_thread_with_comments_roundtrip() {
        let thread = Thread {
            id: ulid::Ulid::new().to_string(),
            origin: Origin::Developer,
            severity: None,
            status: ThreadStatus::Open,
            file: "src/main.rs".into(),
            line_start: 10,
            line_end: 15,
            diff_side: DiffSide::Right,
            anchor_context: "fn main() {".into(),
            comments: vec![
                Comment {
                    id: ulid::Ulid::new().to_string(),
                    author: Author::Developer,
                    body: "This needs error handling".into(),
                    timestamp: chrono::Utc::now(),
                    diff_snapshot: None,
                },
            ],
        };
        let json = serde_json::to_string(&thread).unwrap();
        let deserialized: Thread = serde_json::from_str(&json).unwrap();
        assert_eq!(thread.id, deserialized.id);
        assert_eq!(thread.comments.len(), 1);
        assert_eq!(deserialized.comments[0].author, Author::Developer);
        assert_eq!(deserialized.origin, Origin::Developer);
        assert_eq!(deserialized.severity, None);
    }

    #[test]
    fn test_agent_thread_with_severity() {
        let thread = Thread {
            id: ulid::Ulid::new().to_string(),
            origin: Origin::Agent,
            severity: Some(Severity::Warning),
            status: ThreadStatus::Open,
            file: "src/main.rs".into(),
            line_start: 5,
            line_end: 5,
            diff_side: DiffSide::Right,
            anchor_context: "API_KEY = \"secret\"".into(),
            comments: vec![],
        };
        let json = serde_json::to_string(&thread).unwrap();
        assert!(json.contains("\"origin\":\"agent\""));
        assert!(json.contains("\"severity\":\"warning\""));
        let deserialized: Thread = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.origin, Origin::Agent);
        assert_eq!(deserialized.severity, Some(Severity::Warning));
    }

    #[test]
    fn test_thread_without_origin_defaults_to_developer() {
        // Simulates reading an old session.json that lacks origin/severity
        let json = r#"{
            "id": "test",
            "status": "open",
            "file": "foo.rs",
            "line_start": 1,
            "line_end": 1,
            "diff_side": "right",
            "anchor_context": "fn foo()",
            "comments": []
        }"#;
        let thread: Thread = serde_json::from_str(json).unwrap();
        assert_eq!(thread.origin, Origin::Developer);
        assert_eq!(thread.severity, None);
    }

    // I/O tests (Task 3)
    #[test]
    fn test_write_and_read_session() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join(".review").join("session.json");
        let session = Session::new("main", "feature/test", "abc1234", PathBuf::from("/tmp/test"));
        write_session(&path, &session).unwrap();
        let loaded = read_session(&path).unwrap();
        assert_eq!(loaded.version, 1);
        assert_eq!(loaded.base, "main");
    }

    #[test]
    fn test_write_creates_parent_directory() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join(".review").join("session.json");
        assert!(!path.parent().unwrap().exists());
        write_session(&path, &Session::new("main", "feature/test", "abc1234", PathBuf::from("/tmp/test"))).unwrap();
        assert!(path.exists());
    }

    #[test]
    fn test_read_nonexistent_returns_error() {
        let result = read_session(std::path::Path::new("/nonexistent/session.json"));
        assert!(result.is_err());
    }

    #[test]
    fn test_create_new_session() {
        let session = Session::new("main", "feature/test", "abc1234", PathBuf::from("/tmp/test"));
        assert_eq!(session.version, 1);
        assert_eq!(session.status, SessionStatus::InProgress);
        assert!(session.threads.is_empty());
        assert!(session.files.is_empty());
    }

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

    #[test]
    fn test_compute_stats_empty() {
        let session = Session::new("main", "feature/test", "abc1234", PathBuf::from("/tmp/test"));
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
        let mut session = Session::new("main", "feature/test", "abc1234", PathBuf::from("/tmp/test"));
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

    #[test]
    fn test_write_session_atomic_creates_file() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join(".review").join("session.json");
        let session = Session::new("main", "feature/test", "abc1234", PathBuf::from("/tmp/test"));
        write_session_atomic(&path, &session).unwrap();
        assert!(path.exists());
        let loaded = read_session(&path).unwrap();
        assert_eq!(loaded.base, "main");
    }

    #[test]
    fn test_write_session_atomic_replaces_existing() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join(".review").join("session.json");
        let session1 = Session::new("main", "feature/a", "abc", PathBuf::from("/tmp/test"));
        write_session_atomic(&path, &session1).unwrap();
        let session2 = Session::new("main", "feature/b", "def", PathBuf::from("/tmp/test"));
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
    fn test_session_has_id_and_repo_path() {
        let session = Session::new("main", "feature/foo", "abc123", PathBuf::from("/tmp/repo"));
        assert!(!session.id.to_string().is_empty());
        assert_eq!(session.repo_path, PathBuf::from("/tmp/repo"));
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
}
