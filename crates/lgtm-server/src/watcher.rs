use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use notify_debouncer_mini::{new_debouncer, DebouncedEventKind};

use crate::AppState;
use crate::ws::WsMessage;

pub fn start_watchers(state: Arc<AppState>) -> Result<(), Box<dyn std::error::Error>> {
    let session_path = state.session_path.clone();
    let repo_path = state.repo_path.clone();
    let tx = state.broadcast_tx.clone();
    let rt = tokio::runtime::Handle::current();

    // Session file watcher (300ms debounce)
    let state_for_session = state.clone();
    let tx_for_session = tx.clone();
    let review_dir = session_path.parent().unwrap().to_path_buf();
    let rt_session = rt.clone();
    std::thread::spawn(move || {
        let rt = rt_session;
        let mut debouncer = new_debouncer(
            Duration::from_millis(300),
            move |events: Result<Vec<notify_debouncer_mini::DebouncedEvent>, notify::Error>| {
                if let Ok(events) = events {
                    let has_session_change = events
                        .iter()
                        .any(|e| e.kind == DebouncedEventKind::Any && e.path == session_path);
                    if has_session_change {
                        let state = state_for_session.clone();
                        let tx = tx_for_session.clone();
                        rt.spawn(async move {
                            if let Ok(session) = lgtm_session::read_session(&state.session_path) {
                                *state.session.write().await = session.clone();
                                let _ = tx.send(WsMessage::SessionUpdated(session));
                            }
                        });
                    }
                }
            },
        )
        .expect("failed to create session watcher");

        // Ensure .review directory exists before watching
        let _ = std::fs::create_dir_all(&review_dir);
        if let Err(e) = debouncer
            .watcher()
            .watch(
                review_dir.as_ref(),
                notify::RecursiveMode::NonRecursive,
            )
        {
            tracing::warn!("failed to watch .review directory: {e}");
            return;
        }

        // Keep thread alive
        std::thread::park();
    });

    // Working tree watcher (500ms debounce)
    let state_for_tree = state.clone();
    let repo_path_for_watch = repo_path.clone();
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
                        let tx = tx.clone();
                        let repo = repo_path.clone();
                        rt.spawn(async move {
                            let session = state.session.read().await;
                            let merge_base = session.merge_base.clone();
                            let head = session.head.clone();
                            drop(session);

                            let mut updated_files = Vec::new();
                            for path in &changed_paths {
                                if let Ok(rel) = path.strip_prefix(&repo) {
                                    let rel_str = rel.to_string_lossy();
                                    if let Ok(Some(file)) = state.diff_provider.diff_file(
                                        &merge_base,
                                        &head,
                                        &rel_str,
                                    ) {
                                        updated_files.push(file);
                                    }
                                }
                            }
                            if !updated_files.is_empty() {
                                let _ = tx.send(WsMessage::DiffUpdated(updated_files));
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

    Ok(())
}
