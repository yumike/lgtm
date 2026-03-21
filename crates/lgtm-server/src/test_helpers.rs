#[cfg(test)]
use std::sync::Arc;

#[cfg(test)]
use lgtm_session::SessionStore;

#[cfg(test)]
use crate::AppState;

#[cfg(test)]
pub fn test_state() -> Arc<AppState> {
    let dir = tempfile::TempDir::new().unwrap();
    let path = dir.path().to_path_buf();
    std::mem::forget(dir);
    let store = Arc::new(SessionStore::new(path));
    Arc::new(AppState::new(store))
}

#[cfg(test)]
pub struct MockDiffProvider;

#[cfg(test)]
impl lgtm_git::DiffProvider for MockDiffProvider {
    fn merge_base(&self, _head: &str, _base: &str) -> Result<String, lgtm_git::GitError> {
        Ok("abc1234".into())
    }
    fn diff_files(&self, _from: &str, _to: &str) -> Result<Vec<lgtm_git::DiffFile>, lgtm_git::GitError> {
        Ok(vec![])
    }
    fn diff_file(&self, _from: &str, _to: &str, _path: &str) -> Result<Option<lgtm_git::DiffFile>, lgtm_git::GitError> {
        Ok(None)
    }
    fn head_ref(&self) -> Result<String, lgtm_git::GitError> {
        Ok("feature/test".into())
    }
    fn head_commit(&self) -> Result<String, lgtm_git::GitError> {
        Ok("abc1234".into())
    }
}
