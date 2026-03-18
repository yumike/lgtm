use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiffFile {
    pub path: String,
    pub status: FileChangeKind,
    pub old_path: Option<String>,
    pub hunks: Vec<Hunk>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FileChangeKind {
    Added,
    Modified,
    Deleted,
    Renamed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Hunk {
    pub old_start: u32,
    pub old_count: u32,
    pub new_start: u32,
    pub new_count: u32,
    pub lines: Vec<DiffLine>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiffLine {
    pub kind: LineKind,
    pub content: String,
    pub old_lineno: Option<u32>,
    pub new_lineno: Option<u32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LineKind {
    Context,
    Add,
    Delete,
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    #[test]
    fn test_diff_file_serializes() {
        let file = DiffFile {
            path: "src/main.rs".into(),
            status: FileChangeKind::Modified,
            old_path: None,
            hunks: vec![Hunk {
                old_start: 1,
                old_count: 3,
                new_start: 1,
                new_count: 5,
                lines: vec![
                    DiffLine {
                        kind: LineKind::Context,
                        content: "use std::io;".into(),
                        old_lineno: Some(1),
                        new_lineno: Some(1),
                    },
                    DiffLine {
                        kind: LineKind::Add,
                        content: "use std::fs;".into(),
                        old_lineno: None,
                        new_lineno: Some(2),
                    },
                ],
            }],
        };
        let json = serde_json::to_string(&file).unwrap();
        let deserialized: DiffFile = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.path, "src/main.rs");
        assert_eq!(deserialized.hunks[0].lines.len(), 2);
    }
}
