pub mod types;
pub mod cli_provider;

pub use types::*;

#[derive(Debug, thiserror::Error)]
pub enum GitError {
    #[error("git error: {0}")]
    Git(String),
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("ref not found: {0}")]
    RefNotFound(String),
}

/// Trait for computing diffs from a git repository.
pub trait DiffProvider: Send + Sync {
    fn merge_base(&self, head: &str, base: &str) -> Result<String, GitError>;
    fn diff_files(&self, from: &str, to: &str) -> Result<Vec<DiffFile>, GitError>;
    fn diff_file(&self, from: &str, to: &str, path: &str) -> Result<Option<DiffFile>, GitError>;
    fn head_ref(&self) -> Result<String, GitError>;
    fn head_commit(&self) -> Result<String, GitError>;
}
