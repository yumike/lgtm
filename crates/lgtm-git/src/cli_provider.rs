use std::path::{Path, PathBuf};
use std::process::Command;

use crate::{DiffFile, DiffLine, DiffProvider, FileChangeKind, GitError, Hunk, LineKind};

pub struct CliDiffProvider {
    repo_path: PathBuf,
}

impl CliDiffProvider {
    pub fn new(repo_path: &Path) -> Self {
        Self {
            repo_path: repo_path.to_path_buf(),
        }
    }

    fn git(&self, args: &[&str]) -> Result<String, GitError> {
        let output = Command::new("git")
            .args(args)
            .current_dir(&self.repo_path)
            .output()
            .map_err(|e| GitError::Io(e))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(GitError::Git(stderr.trim().to_string()));
        }

        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    }
}

impl DiffProvider for CliDiffProvider {
    fn merge_base(&self, head: &str, base: &str) -> Result<String, GitError> {
        self.git(&["merge-base", head, base])
    }

    fn diff_files(&self, from: &str, to: &str) -> Result<Vec<DiffFile>, GitError> {
        let output = self.git(&["diff", "--name-status", &format!("{from}..{to}")])?;
        let mut files = Vec::new();
        for line in output.lines() {
            if line.is_empty() {
                continue;
            }
            let parts: Vec<&str> = line.splitn(2, '\t').collect();
            if parts.len() < 2 {
                continue;
            }
            let (status_char, path) = (parts[0], parts[1]);
            let (status, old_path) = match status_char.chars().next() {
                Some('A') => (FileChangeKind::Added, None),
                Some('M') => (FileChangeKind::Modified, None),
                Some('D') => (FileChangeKind::Deleted, None),
                Some('R') => {
                    let rename_parts: Vec<&str> = path.splitn(2, '\t').collect();
                    if rename_parts.len() == 2 {
                        (FileChangeKind::Renamed, Some(rename_parts[0].to_string()))
                    } else {
                        (FileChangeKind::Modified, None)
                    }
                }
                _ => (FileChangeKind::Modified, None),
            };
            let final_path = if status == FileChangeKind::Renamed {
                old_path.as_ref().map_or(path, |_| {
                    path.splitn(2, '\t').nth(1).unwrap_or(path)
                })
            } else {
                path
            };
            files.push(DiffFile {
                path: final_path.to_string(),
                status,
                old_path,
                hunks: vec![],
            });
        }
        Ok(files)
    }

    fn diff_file(&self, from: &str, to: &str, path: &str) -> Result<Option<DiffFile>, GitError> {
        let output = self.git(&[
            "diff",
            "--unified=3",
            &format!("{from}..{to}"),
            "--",
            path,
        ])?;

        if output.is_empty() {
            return Ok(None);
        }

        let hunks = parse_unified_diff(&output);
        let status = if output.contains("new file mode") {
            FileChangeKind::Added
        } else if output.contains("deleted file mode") {
            FileChangeKind::Deleted
        } else {
            FileChangeKind::Modified
        };

        Ok(Some(DiffFile {
            path: path.to_string(),
            status,
            old_path: None,
            hunks,
        }))
    }

    fn head_ref(&self) -> Result<String, GitError> {
        self.git(&["rev-parse", "--abbrev-ref", "HEAD"])
    }

    fn head_commit(&self) -> Result<String, GitError> {
        self.git(&["rev-parse", "--short", "HEAD"])
    }
}

fn parse_unified_diff(diff_output: &str) -> Vec<Hunk> {
    let mut hunks = Vec::new();
    let mut current_hunk: Option<Hunk> = None;
    let mut old_line: u32 = 0;
    let mut new_line: u32 = 0;

    for line in diff_output.lines() {
        if line.starts_with("@@") {
            if let Some(hunk) = current_hunk.take() {
                hunks.push(hunk);
            }
            if let Some((old_start, old_count, new_start, new_count)) = parse_hunk_header(line) {
                old_line = old_start;
                new_line = new_start;
                current_hunk = Some(Hunk {
                    old_start,
                    old_count,
                    new_start,
                    new_count,
                    lines: Vec::new(),
                });
            }
        } else if let Some(ref mut hunk) = current_hunk {
            if let Some(rest) = line.strip_prefix('+') {
                hunk.lines.push(DiffLine {
                    kind: LineKind::Add,
                    content: rest.to_string(),
                    old_lineno: None,
                    new_lineno: Some(new_line),
                });
                new_line += 1;
            } else if let Some(rest) = line.strip_prefix('-') {
                hunk.lines.push(DiffLine {
                    kind: LineKind::Delete,
                    content: rest.to_string(),
                    old_lineno: Some(old_line),
                    new_lineno: None,
                });
                old_line += 1;
            } else if line.starts_with(' ') || line.is_empty() {
                let content = if line.starts_with(' ') { &line[1..] } else { "" };
                hunk.lines.push(DiffLine {
                    kind: LineKind::Context,
                    content: content.to_string(),
                    old_lineno: Some(old_line),
                    new_lineno: Some(new_line),
                });
                old_line += 1;
                new_line += 1;
            }
        }
    }

    if let Some(hunk) = current_hunk {
        hunks.push(hunk);
    }

    hunks
}

fn parse_hunk_header(line: &str) -> Option<(u32, u32, u32, u32)> {
    let line = line.strip_prefix("@@ -")?;
    let at_pos = line.find(" @@")?;
    let header = &line[..at_pos];
    let parts: Vec<&str> = header.split(' ').collect();
    if parts.len() < 2 {
        return None;
    }
    let old_parts: Vec<&str> = parts[0].split(',').collect();
    let new_part = parts[1].strip_prefix('+')?;
    let new_parts: Vec<&str> = new_part.split(',').collect();

    let old_start = old_parts[0].parse().ok()?;
    let old_count = old_parts.get(1).and_then(|s| s.parse().ok()).unwrap_or(1);
    let new_start = new_parts[0].parse().ok()?;
    let new_count = new_parts.get(1).and_then(|s| s.parse().ok()).unwrap_or(1);

    Some((old_start, old_count, new_start, new_count))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;
    use tempfile::TempDir;

    fn setup_test_repo() -> TempDir {
        let dir = TempDir::new().unwrap();
        let d = dir.path();
        let run = |args: &[&str]| {
            Command::new("git")
                .args(args)
                .current_dir(d)
                .output()
                .unwrap();
        };
        run(&["init", "-b", "main"]);
        run(&["-c", "user.name=Test", "-c", "user.email=t@t.com", "commit", "--allow-empty", "-m", "init"]);
        std::fs::write(d.join("hello.txt"), "hello\nworld\n").unwrap();
        run(&["add", "hello.txt"]);
        run(&["-c", "user.name=Test", "-c", "user.email=t@t.com", "commit", "-m", "add hello"]);
        run(&["checkout", "-b", "feature"]);
        std::fs::write(d.join("hello.txt"), "hello\nrust\nworld\n").unwrap();
        std::fs::write(d.join("new.txt"), "new file\n").unwrap();
        run(&["add", "."]);
        run(&["-c", "user.name=Test", "-c", "user.email=t@t.com", "commit", "-m", "modify hello, add new"]);
        dir
    }

    #[test]
    fn test_merge_base() {
        let dir = setup_test_repo();
        let provider = CliDiffProvider::new(dir.path());
        let mb = provider.merge_base("feature", "main").unwrap();
        assert!(!mb.is_empty());
    }

    #[test]
    fn test_diff_files_lists_changes() {
        let dir = setup_test_repo();
        let provider = CliDiffProvider::new(dir.path());
        let mb = provider.merge_base("feature", "main").unwrap();
        let files = provider.diff_files(&mb, "feature").unwrap();
        let paths: Vec<&str> = files.iter().map(|f| f.path.as_str()).collect();
        assert!(paths.contains(&"hello.txt"));
        assert!(paths.contains(&"new.txt"));
    }

    #[test]
    fn test_diff_file_returns_hunks() {
        let dir = setup_test_repo();
        let provider = CliDiffProvider::new(dir.path());
        let mb = provider.merge_base("feature", "main").unwrap();
        let file = provider.diff_file(&mb, "feature", "hello.txt").unwrap().unwrap();
        assert_eq!(file.path, "hello.txt");
        assert!(!file.hunks.is_empty());
        let has_add = file.hunks.iter()
            .flat_map(|h| &h.lines)
            .any(|l| l.kind == LineKind::Add && l.content.contains("rust"));
        assert!(has_add);
    }

    #[test]
    fn test_head_ref() {
        let dir = setup_test_repo();
        let provider = CliDiffProvider::new(dir.path());
        let head = provider.head_ref().unwrap();
        assert_eq!(head, "feature");
    }
}
