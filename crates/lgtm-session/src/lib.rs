use std::collections::HashMap;
use std::path::Path;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    pub version: u32,
    pub status: SessionStatus,
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
}

impl Session {
    pub fn new(base: &str, head: &str, merge_base: &str) -> Self {
        let now = Utc::now();
        Self {
            version: 1,
            status: SessionStatus::InProgress,
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

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    #[test]
    fn test_session_roundtrip() {
        let session = Session {
            version: 1,
            status: SessionStatus::InProgress,
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
        let session = Session::new("main", "feature/test", "abc1234");
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
        write_session(&path, &Session::new("main", "feature/test", "abc1234")).unwrap();
        assert!(path.exists());
    }

    #[test]
    fn test_read_nonexistent_returns_error() {
        let result = read_session(std::path::Path::new("/nonexistent/session.json"));
        assert!(result.is_err());
    }

    #[test]
    fn test_create_new_session() {
        let session = Session::new("main", "feature/test", "abc1234");
        assert_eq!(session.version, 1);
        assert_eq!(session.status, SessionStatus::InProgress);
        assert!(session.threads.is_empty());
        assert!(session.files.is_empty());
    }
}
