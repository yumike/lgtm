use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, RwLock};
use std::time::Duration;

use notify_debouncer_mini::{new_debouncer, DebouncedEventKind};
use ulid::Ulid;

use crate::AppState;
use crate::ws::WsMessage;

struct WatcherEntry {
    session_ids: Vec<Ulid>,
    _handle: Option<notify_debouncer_mini::Debouncer<notify::RecommendedWatcher>>,
}

pub struct WatcherRegistry {
    watchers: RwLock<HashMap<PathBuf, WatcherEntry>>,
}

impl WatcherRegistry {
    pub fn new() -> Self {
        Self {
            watchers: RwLock::new(HashMap::new()),
        }
    }

    pub fn register(&self, repo_path: PathBuf, session_id: Ulid) {
        let mut watchers = self.watchers.write().unwrap();
        let entry = watchers.entry(repo_path).or_insert_with(|| WatcherEntry {
            session_ids: Vec::new(),
            _handle: None,
        });
        if !entry.session_ids.contains(&session_id) {
            entry.session_ids.push(session_id);
        }
    }

    pub fn unregister(&self, repo_path: &PathBuf, session_id: Ulid) {
        let mut watchers = self.watchers.write().unwrap();
        let should_remove = if let Some(entry) = watchers.get_mut(repo_path) {
            entry.session_ids.retain(|&id| id != session_id);
            entry.session_ids.is_empty()
        } else {
            false
        };
        if should_remove {
            watchers.remove(repo_path);
        }
    }

    pub fn repo_count(&self) -> usize {
        self.watchers.read().unwrap().len()
    }

    pub fn session_ids_for_repo(&self, repo_path: &PathBuf) -> Vec<Ulid> {
        self.watchers
            .read()
            .unwrap()
            .get(repo_path)
            .map(|entry| entry.session_ids.clone())
            .unwrap_or_default()
    }
}

/// Start file watchers for a specific session.
pub fn start_watchers(
    state: Arc<AppState>,
    session_id: Ulid,
    repo_path: PathBuf,
) -> Result<(), Box<dyn std::error::Error>> {
    let rt = tokio::runtime::Handle::current();

    // Working tree watcher (500ms debounce)
    let state_for_tree = state.clone();
    let repo_path_for_watch = repo_path.clone();
    let repo_path_for_closure = repo_path.clone();
    let rt_tree = rt.clone();
    std::thread::spawn(move || {
        let rt = rt_tree;
        let mut debouncer = new_debouncer(
            Duration::from_millis(500),
            move |events: Result<Vec<notify_debouncer_mini::DebouncedEvent>, notify::Error>| {
                if let Ok(events) = events {
                    let changed_paths: Vec<PathBuf> = events
                        .into_iter()
                        .filter(|e| e.kind == DebouncedEventKind::Any)
                        .map(|e| e.path)
                        .filter(|p| {
                            let s = p.to_string_lossy();
                            !s.contains(".git/")
                                && !s.contains("node_modules/")
                                && !s.contains("__pycache__/")
                                && !s.contains("/target/")
                                && !s.contains(".review/")
                        })
                        .collect();

                    if !changed_paths.is_empty() {
                        let state = state_for_tree.clone();
                        let repo = repo_path.clone();
                        let sid = session_id;
                        rt.spawn(async move {
                            let session = match state.store.get(sid) {
                                Ok(s) => s,
                                Err(_) => return,
                            };
                            let merge_base = session.merge_base.clone();
                            let head = session.head.clone();

                            let providers = state.diff_providers.read().unwrap();
                            let Some(provider) = providers.get(&sid) else {
                                return;
                            };

                            let mut updated_files = Vec::new();
                            for path in &changed_paths {
                                if let Ok(rel) = path.strip_prefix(&repo) {
                                    let rel_str = rel.to_string_lossy();
                                    if let Ok(Some(file)) = provider.diff_file(
                                        &merge_base,
                                        &head,
                                        &rel_str,
                                    ) {
                                        updated_files.push(file);
                                    }
                                }
                            }
                            drop(providers);
                            if !updated_files.is_empty() {
                                state.broadcast(sid, WsMessage::DiffUpdated(updated_files));
                            }
                        });
                    }
                }
            },
        )
        .expect("failed to create tree watcher");

        debouncer
            .watcher()
            .watch(repo_path_for_watch.as_ref(), notify::RecursiveMode::Recursive)
            .expect("failed to watch repo");

        std::thread::park();
    });

    // Store directory watcher (300ms debounce) for session file changes
    let store_dir = {
        // Access the store's directory indirectly through session persistence
        // The store persists to its own dir, so we watch that via session updates
        // For now, session changes go through the store which handles persistence
        let _ = state;
        let _ = repo_path_for_closure;
    };
    let _ = store_dir;

    Ok(())
}

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
        assert_eq!(registry.repo_count(), 1);

        registry.unregister(&repo, id1);
        assert_eq!(registry.repo_count(), 1);

        registry.unregister(&repo, id2);
        assert_eq!(registry.repo_count(), 0);
    }

    #[test]
    fn test_watcher_registry_multiple_repos() {
        let registry = WatcherRegistry::new();
        let id1 = Ulid::new();
        let id2 = Ulid::new();
        let repo_a = PathBuf::from("/tmp/repo-a");
        let repo_b = PathBuf::from("/tmp/repo-b");

        registry.register(repo_a.clone(), id1);
        registry.register(repo_b.clone(), id2);
        assert_eq!(registry.repo_count(), 2);

        registry.unregister(&repo_a, id1);
        assert_eq!(registry.repo_count(), 1);

        registry.unregister(&repo_b, id2);
        assert_eq!(registry.repo_count(), 0);
    }

    #[test]
    fn test_watcher_registry_session_ids_for_repo() {
        let registry = WatcherRegistry::new();
        let id1 = Ulid::new();
        let id2 = Ulid::new();
        let repo = PathBuf::from("/tmp/repo");

        registry.register(repo.clone(), id1);
        registry.register(repo.clone(), id2);

        let ids = registry.session_ids_for_repo(&repo);
        assert_eq!(ids.len(), 2);
        assert!(ids.contains(&id1));
        assert!(ids.contains(&id2));
    }

    #[test]
    fn test_watcher_registry_duplicate_register() {
        let registry = WatcherRegistry::new();
        let id1 = Ulid::new();
        let repo = PathBuf::from("/tmp/repo");

        registry.register(repo.clone(), id1);
        registry.register(repo.clone(), id1);

        let ids = registry.session_ids_for_repo(&repo);
        assert_eq!(ids.len(), 1);
    }

    #[test]
    fn test_watcher_registry_unregister_nonexistent() {
        let registry = WatcherRegistry::new();
        let id1 = Ulid::new();
        let repo = PathBuf::from("/tmp/repo");

        // Should not panic
        registry.unregister(&repo, id1);
        assert_eq!(registry.repo_count(), 0);
    }

    #[test]
    fn test_watcher_registry_session_ids_for_unknown_repo() {
        let registry = WatcherRegistry::new();
        let repo = PathBuf::from("/tmp/unknown");

        let ids = registry.session_ids_for_repo(&repo);
        assert!(ids.is_empty());
    }
}
