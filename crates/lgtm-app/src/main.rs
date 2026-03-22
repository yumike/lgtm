#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use clap::Parser;
use lgtm_server::lockfile;
use lgtm_session::SessionStore;
use std::sync::Arc;

#[derive(Parser)]
#[command(name = "lgtm-app")]
struct Args {
    /// Run in headless mode (HTTP server only, no window)
    #[arg(long)]
    headless: bool,
}

fn main() {
    let args = Args::parse();
    tracing_subscriber::fmt::init();

    // Allow overriding sessions dir via env var (for tests)
    let store_dir = std::env::var("LGTM_SESSIONS_DIR")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| lockfile::sessions_dir());

    let store = Arc::new(SessionStore::new(store_dir));
    store.load().expect("failed to load sessions");

    let state = Arc::new(lgtm_server::AppState::new(store));

    // Allow overriding assets dir via env var (for tests)
    let assets_dir = std::env::var("LGTM_ASSETS_DIR")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| {
            std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../../packages/web/dist")
        });

    if args.headless {
        run_headless(state, assets_dir);
    } else {
        run_with_tauri(state, assets_dir);
    }
}

fn restore_sessions(state: &Arc<lgtm_server::AppState>) {
    for session in state.store.list() {
        let provider = lgtm_git::cli_provider::CliDiffProvider::new(&session.repo_path);
        state.register_session(session.id, Box::new(provider));
        let _ = lgtm_server::watcher::start_watchers(
            state.clone(),
            session.id,
            session.repo_path.clone(),
        );
    }
}

fn run_headless(state: Arc<lgtm_server::AppState>, assets_dir: std::path::PathBuf) {
    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async {
        restore_sessions(&state);

        let app = lgtm_server::create_router_with_assets(state, Some(assets_dir));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();

        let lockfile_path = lockfile::lockfile_path();
        lockfile::write_lockfile(&lockfile_path, std::process::id(), port)
            .expect("failed to write lockfile");

        // Print port to stdout so test fixtures can capture it
        println!("{}", port);

        axum::serve(listener, app).await.unwrap();
    });
}

fn run_with_tauri(state: Arc<lgtm_server::AppState>, assets_dir: std::path::PathBuf) {
    let (port_tx, port_rx) = std::sync::mpsc::channel();

    let state_clone = state.clone();
    std::thread::spawn(move || {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            restore_sessions(&state_clone);

            let app = lgtm_server::create_router_with_assets(state_clone, Some(assets_dir));
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let port = listener.local_addr().unwrap().port();

            let lockfile_path = lockfile::lockfile_path();
            lockfile::write_lockfile(&lockfile_path, std::process::id(), port)
                .expect("failed to write lockfile");

            tracing::info!("Server listening on 127.0.0.1:{}", port);
            port_tx.send(port).unwrap();

            axum::serve(listener, app).await.unwrap();
        });
    });

    let port = port_rx.recv().expect("failed to get server port");

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

    let _ = lockfile::remove_lockfile(&lockfile::lockfile_path());
}
