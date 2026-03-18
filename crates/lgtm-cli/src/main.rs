use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{Context, Result, bail};
use clap::Parser;
use tokio::sync::RwLock;
use tracing_subscriber::EnvFilter;

use lgtm_git::DiffProvider;
use lgtm_git::cli_provider::CliDiffProvider;
use lgtm_session::{Session, SessionStatus};

#[derive(Parser)]
#[command(name = "lgtm", about = "Local code review tool")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(clap::Subcommand)]
enum Commands {
    /// Start a review session
    Start {
        /// Base branch or commit to diff against
        #[arg(long, default_value = "main")]
        base: String,

        /// Web server port
        #[arg(long, default_value = "4567")]
        port: u16,

        /// Bind address
        #[arg(long, default_value = "127.0.0.1")]
        host: String,

        /// Don't open browser automatically
        #[arg(long)]
        no_open: bool,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    let cli = Cli::parse();

    match cli.command {
        Commands::Start {
            base,
            port,
            host,
            no_open,
        } => start(base, port, host, no_open).await?,
    }

    Ok(())
}

async fn start(base: String, port: u16, host: String, no_open: bool) -> Result<()> {
    let repo_path = find_repo_root()?;
    let session_path = repo_path.join(".review").join("session.json");

    let provider = CliDiffProvider::new(&repo_path);

    let head = provider.head_ref().context("Failed to detect HEAD branch")?;

    let session = if session_path.exists() {
        let existing = lgtm_session::read_session(&session_path)
            .context("Failed to read existing session")?;
        match existing.status {
            SessionStatus::InProgress => {
                tracing::info!("Resuming existing review session");
                let merge_base = provider
                    .merge_base(&head, &base)
                    .context("Failed to compute merge-base")?;
                let mut session = existing;
                session.merge_base = merge_base;
                session.updated_at = chrono::Utc::now();
                session
            }
            _ => {
                bail!(
                    "Session exists with status {:?}. Run `lgtm clean` first.",
                    existing.status
                );
            }
        }
    } else {
        let merge_base = provider
            .merge_base(&head, &base)
            .context("Failed to compute merge-base")?;
        let session = Session::new(&base, &head, &merge_base);
        lgtm_session::write_session(&session_path, &session)?;
        session
    };

    let (broadcast_tx, _) = tokio::sync::broadcast::channel(64);

    let state = Arc::new(lgtm_server::AppState {
        session: RwLock::new(session),
        session_path,
        diff_provider: Box::new(provider),
        repo_path: repo_path.clone(),
        broadcast_tx,
    });

    // Start file watchers
    lgtm_server::watcher::start_watchers(state.clone())
        .map_err(|e| anyhow::anyhow!("{e}"))?;

    let app = lgtm_server::create_router(state);
    let addr = format!("{host}:{port}");
    let listener = tokio::net::TcpListener::bind(&addr).await?;

    println!("lgtm server running at http://{addr}");

    if !no_open {
        let _ = open::that(format!("http://{addr}"));
    }

    axum::serve(listener, app).await?;

    Ok(())
}

fn find_repo_root() -> Result<PathBuf> {
    let output = std::process::Command::new("git")
        .args(["rev-parse", "--show-toplevel"])
        .output()
        .context("Failed to run git")?;

    if !output.status.success() {
        bail!("Not in a git repository");
    }

    let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
    Ok(PathBuf::from(path))
}
