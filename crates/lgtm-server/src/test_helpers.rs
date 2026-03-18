#[cfg(test)]
use std::sync::Arc;

#[cfg(test)]
use axum_test::TestServer;
#[cfg(test)]
use tokio::sync::RwLock;

#[cfg(test)]
use crate::AppState;

#[cfg(test)]
pub async fn create_test_app() -> TestServer {
    let session = lgtm_session::Session::new("main", "feature/test", "abc1234");
    let (broadcast_tx, _) = tokio::sync::broadcast::channel(32);
    let state = Arc::new(AppState {
        session: RwLock::new(session),
        session_path: std::path::PathBuf::from("/tmp/test-session.json"),
        diff_provider: Box::new(MockDiffProvider),
        repo_path: std::path::PathBuf::from("/tmp"),
        broadcast_tx,
    });
    let app = crate::create_router(state);
    TestServer::new(app).unwrap()
}

#[cfg(test)]
struct MockDiffProvider;

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
