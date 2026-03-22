#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use lgtm_server::lockfile;
use lgtm_session::SessionStore;
use std::sync::Arc;

fn main() {
    tracing_subscriber::fmt::init();

    let store_dir = lockfile::sessions_dir();
    let store = Arc::new(SessionStore::new(store_dir));
    store.load().expect("failed to load sessions");

    let state = Arc::new(lgtm_server::AppState::new(store));

    // Channel to communicate port from server thread to main thread
    let (port_tx, port_rx) = std::sync::mpsc::channel();

    // Start Axum server on dynamic port in background thread
    let state_clone = state.clone();
    std::thread::spawn(move || {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            // Serve frontend from the web dist directory
            let assets_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../../packages/web/dist");
            // Re-register diff providers and watchers for restored sessions
            for session in state_clone.store.list() {
                let provider = lgtm_git::cli_provider::CliDiffProvider::new(&session.repo_path);
                state_clone.register_session(session.id, Box::new(provider));
                let _ = lgtm_server::watcher::start_watchers(
                    state_clone.clone(),
                    session.id,
                    session.repo_path.clone(),
                );
            }

            let app = lgtm_server::create_router_with_assets(state_clone, Some(assets_dir));
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let port = listener.local_addr().unwrap().port();

            // Write lockfile
            let lockfile_path = lockfile::lockfile_path();
            lockfile::write_lockfile(&lockfile_path, std::process::id(), port)
                .expect("failed to write lockfile");

            tracing::info!("Server listening on 127.0.0.1:{}", port);

            // Send port to main thread before starting to serve
            port_tx.send(port).unwrap();

            axum::serve(listener, app).await.unwrap();
        });
    });

    // Wait for server to be ready and get the port
    let port = port_rx.recv().expect("failed to get server port");

    // Run Tauri app — setup() navigates webview to the Axum server
    tauri::Builder::default()
        .setup(move |app| {
            use tauri::Manager;
            let window = app.get_webview_window("main").unwrap();
            window.navigate(
                url::Url::parse(&format!("http://127.0.0.1:{}", port)).unwrap(),
            )?;
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");

    // Cleanup lockfile on exit
    let _ = lockfile::remove_lockfile(&lockfile::lockfile_path());
}
