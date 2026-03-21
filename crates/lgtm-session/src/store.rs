use crate::{Session, SessionError, SessionStatus};
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

    /// Scan the store directory for .json files and load in-progress sessions.
    pub fn load(&self) -> Result<(), SessionError> {
        std::fs::create_dir_all(&self.dir)?;
        let mut sessions = self
            .sessions
            .write()
            .map_err(|e| SessionError::Lock(e.to_string()))?;

        for entry in std::fs::read_dir(&self.dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) == Some("json") {
                let contents = std::fs::read_to_string(&path)?;
                let session: Session = serde_json::from_str(&contents)?;
                if session.status == SessionStatus::InProgress {
                    sessions.insert(session.id, session);
                }
            }
        }

        Ok(())
    }

    /// Create a new session. If an in-progress session already exists for the
    /// same repo_path and head, return the existing one instead.
    pub fn create(
        &self,
        base: &str,
        head: &str,
        merge_base: &str,
        repo_path: PathBuf,
    ) -> Result<Session, SessionError> {
        let mut sessions = self
            .sessions
            .write()
            .map_err(|e| SessionError::Lock(e.to_string()))?;

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
        let sessions = self
            .sessions
            .read()
            .map_err(|e| SessionError::Lock(e.to_string()))?;

        sessions
            .get(&id)
            .cloned()
            .ok_or_else(|| SessionError::NotFound(format!("Session {} not found", id)))
    }

    pub fn find_by_repo_and_head(
        &self,
        repo_path: &Path,
        head: &str,
    ) -> Result<Option<Session>, SessionError> {
        let sessions = self
            .sessions
            .read()
            .map_err(|e| SessionError::Lock(e.to_string()))?;

        Ok(sessions.values().find(|s| {
            s.repo_path == repo_path
                && s.head == head
                && s.status == SessionStatus::InProgress
        }).cloned())
    }

    pub fn list(&self) -> Vec<Session> {
        self.sessions
            .read()
            .map(|sessions| sessions.values().cloned().collect())
            .unwrap_or_default()
    }

    pub fn update<F: FnOnce(&mut Session)>(
        &self,
        id: Ulid,
        f: F,
    ) -> Result<Session, SessionError> {
        let mut sessions = self
            .sessions
            .write()
            .map_err(|e| SessionError::Lock(e.to_string()))?;

        let session = sessions
            .get_mut(&id)
            .ok_or_else(|| SessionError::NotFound(format!("Session {} not found", id)))?;

        f(session);
        session.updated_at = chrono::Utc::now();
        let updated = session.clone();
        self.persist(&updated)?;
        Ok(updated)
    }

    pub fn remove(&self, id: Ulid) -> Result<(), SessionError> {
        let mut sessions = self
            .sessions
            .write()
            .map_err(|e| SessionError::Lock(e.to_string()))?;

        if sessions.remove(&id).is_none() {
            return Err(SessionError::NotFound(format!(
                "Session {} not found",
                id
            )));
        }

        let path = self.dir.join(format!("{}.json", id));
        if path.exists() {
            std::fs::remove_file(&path)?;
        }

        Ok(())
    }

    fn persist(&self, session: &Session) -> Result<(), SessionError> {
        std::fs::create_dir_all(&self.dir)?;
        let path = self.dir.join(format!("{}.json", session.id));
        let tmp_path = path.with_extension("json.tmp");
        let json = serde_json::to_string_pretty(session)?;
        std::fs::write(&tmp_path, json)?;
        std::fs::rename(&tmp_path, &path)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    #[test]
    fn test_create_and_get_session() {
        let dir = tempfile::TempDir::new().unwrap();
        let store = SessionStore::new(dir.path().to_path_buf());

        let session = store
            .create("main", "feature/foo", "abc123", PathBuf::from("/tmp/repo"))
            .unwrap();
        let retrieved = store.get(session.id).unwrap();

        assert_eq!(retrieved.id, session.id);
        assert_eq!(retrieved.repo_path, PathBuf::from("/tmp/repo"));
        assert_eq!(retrieved.head, "feature/foo");
        assert_eq!(retrieved.base, "main");
    }

    #[test]
    fn test_find_by_repo_and_branch() {
        let dir = tempfile::TempDir::new().unwrap();
        let store = SessionStore::new(dir.path().to_path_buf());

        let session = store
            .create("main", "feature/bar", "def456", PathBuf::from("/tmp/repo2"))
            .unwrap();

        let found = store
            .find_by_repo_and_head(Path::new("/tmp/repo2"), "feature/bar")
            .unwrap();
        assert!(found.is_some());
        let found = found.unwrap();
        assert_eq!(found.id, session.id);
        assert_eq!(found.head, "feature/bar");

        // Non-matching should return None
        let not_found = store
            .find_by_repo_and_head(Path::new("/tmp/other"), "feature/bar")
            .unwrap();
        assert!(not_found.is_none());
    }

    #[test]
    fn test_create_returns_existing_for_same_repo_branch() {
        let dir = tempfile::TempDir::new().unwrap();
        let store = SessionStore::new(dir.path().to_path_buf());

        let first = store
            .create("main", "feature/x", "aaa", PathBuf::from("/tmp/repo"))
            .unwrap();
        let second = store
            .create("main", "feature/x", "bbb", PathBuf::from("/tmp/repo"))
            .unwrap();

        assert_eq!(first.id, second.id);
    }

    #[test]
    fn test_list_sessions() {
        let dir = tempfile::TempDir::new().unwrap();
        let store = SessionStore::new(dir.path().to_path_buf());

        store
            .create("main", "feature/a", "aaa", PathBuf::from("/tmp/repo1"))
            .unwrap();
        store
            .create("main", "feature/b", "bbb", PathBuf::from("/tmp/repo2"))
            .unwrap();

        let sessions = store.list();
        assert_eq!(sessions.len(), 2);
    }

    #[test]
    fn test_remove_session() {
        let dir = tempfile::TempDir::new().unwrap();
        let store = SessionStore::new(dir.path().to_path_buf());

        let session = store
            .create("main", "feature/rm", "ccc", PathBuf::from("/tmp/repo"))
            .unwrap();
        let id = session.id;

        store.remove(id).unwrap();

        let result = store.get(id);
        assert!(result.is_err());
    }

    #[test]
    fn test_persistence_to_disk() {
        let dir = tempfile::TempDir::new().unwrap();
        let dir_path = dir.path().to_path_buf();

        let session = {
            let store = SessionStore::new(dir_path.clone());
            store
                .create("main", "feature/persist", "ddd", PathBuf::from("/tmp/repo"))
                .unwrap()
        };

        // Create a new store from the same directory and load
        let store2 = SessionStore::new(dir_path);
        store2.load().unwrap();

        let loaded = store2.get(session.id).unwrap();
        assert_eq!(loaded.id, session.id);
        assert_eq!(loaded.head, "feature/persist");
        assert_eq!(loaded.repo_path, PathBuf::from("/tmp/repo"));
    }

    #[test]
    fn test_update_session() {
        let dir = tempfile::TempDir::new().unwrap();
        let store = SessionStore::new(dir.path().to_path_buf());

        let session = store
            .create("main", "feature/upd", "eee", PathBuf::from("/tmp/repo"))
            .unwrap();
        assert_eq!(session.status, SessionStatus::InProgress);

        let updated = store
            .update(session.id, |s| {
                s.status = SessionStatus::Approved;
            })
            .unwrap();

        assert_eq!(updated.status, SessionStatus::Approved);
        assert!(updated.updated_at >= session.updated_at);

        // Verify via get
        let fetched = store.get(session.id).unwrap();
        assert_eq!(fetched.status, SessionStatus::Approved);
    }
}
