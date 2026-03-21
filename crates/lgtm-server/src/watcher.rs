use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use notify_debouncer_mini::{new_debouncer, DebouncedEventKind};
use ulid::Ulid;

use crate::AppState;
use crate::ws::WsMessage;

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
