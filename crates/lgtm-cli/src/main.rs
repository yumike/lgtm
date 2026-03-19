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
    /// Show review session status
    Status {
        /// Output as JSON (required for now)
        #[arg(long)]
        json: bool,
    },

    /// Reply to a review thread
    Reply {
        /// Thread ID (e.g., t_01J8XYZABC)
        thread_id: String,
        /// Comment body (omit to read from --stdin)
        body: Option<String>,
        /// Read body from stdin
        #[arg(long)]
        stdin: bool,
    },

    /// Create an agent-initiated review thread
    Thread {
        /// File path relative to repo root
        #[arg(long)]
        file: String,
        /// Start line number (1-indexed)
        #[arg(long)]
        line: u32,
        /// End line number (defaults to --line)
        #[arg(long)]
        line_end: Option<u32>,
        /// Severity: critical, warning, or info
        #[arg(long)]
        severity: String,
        /// Observation body (omit to read from --stdin)
        body: Option<String>,
        /// Read body from stdin
        #[arg(long)]
        stdin: bool,
    },

    /// Start a review session
    Start {
        /// Base branch or commit to diff against (auto-detects main/master if omitted)
        #[arg(long)]
        base: Option<String>,

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
        Commands::Status { json } => status(json)?,
        Commands::Reply { thread_id, body, stdin } => reply(thread_id, body, stdin)?,
        Commands::Thread { file, line, line_end, severity, body, stdin } => {
            create_thread(file, line, line_end, severity, body, stdin)?
        }
        Commands::Start { base, port, host, no_open } => {
            start(base, port, host, no_open).await?
        }
    }

    Ok(())
}

async fn start(base: Option<String>, port: u16, host: String, no_open: bool) -> Result<()> {
    let repo_path = find_repo_root()?;
    let base = match base {
        Some(b) => b,
        None => detect_base_branch(&repo_path)?,
    };
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

fn status(json: bool) -> Result<()> {
    let repo_path = find_repo_root()?;
    let session_path = repo_path.join(".review").join("session.json");

    if !session_path.exists() {
        std::process::exit(2);
    }

    let session = lgtm_session::read_session(&session_path)
        .context("Failed to read session")?;

    let stats = lgtm_session::compute_stats(&session);

    if json {
        let open_threads: Vec<&lgtm_session::Thread> = session
            .threads
            .iter()
            .filter(|t| t.status == lgtm_session::ThreadStatus::Open)
            .collect();

        let output = serde_json::json!({
            "session_status": session.status,
            "base": session.base,
            "head": session.head,
            "stats": stats,
            "open_threads": open_threads,
        });

        println!("{}", serde_json::to_string_pretty(&output)?);
    } else {
        let files_reviewed = session
            .files
            .values()
            .filter(|s| **s == lgtm_session::FileReviewStatus::Reviewed)
            .count();
        let files_total = session.files.len();

        let elapsed = chrono::Utc::now() - session.created_at;
        let elapsed_str = if elapsed.num_hours() > 0 {
            format!("{} hours ago", elapsed.num_hours())
        } else {
            format!("{} minutes ago", elapsed.num_minutes())
        };

        println!("lgtm: reviewing {} against {}", session.head, session.base);

        let mut parts = Vec::new();
        if stats.open > 0 {
            parts.push(format!("{} open", stats.open));
        }
        if stats.resolved > 0 {
            parts.push(format!("{} resolved", stats.resolved));
        }
        if stats.wontfix > 0 {
            parts.push(format!("{} wontfix", stats.wontfix));
        }
        if stats.dismissed > 0 {
            parts.push(format!("{} dismissed", stats.dismissed));
        }

        if stats.total_threads > 0 {
            println!("  {} threads: {}", stats.total_threads, parts.join(", "));
        } else {
            println!("  No threads yet");
        }

        println!("  {}/{} files reviewed", files_reviewed, files_total);
        println!("  Session started {}", elapsed_str);
    }

    Ok(())
}

fn read_body(body: Option<String>, stdin: bool) -> Result<String> {
    if stdin {
        use std::io::Read;
        let mut buf = String::new();
        std::io::stdin().read_to_string(&mut buf)?;
        Ok(buf.trim().to_string())
    } else if let Some(body) = body {
        Ok(body)
    } else {
        bail!("Provide body as argument or use --stdin");
    }
}

fn git_head(repo_path: &std::path::Path) -> Result<String> {
    let output = std::process::Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(repo_path)
        .output()
        .context("Failed to run git rev-parse HEAD")?;
    if !output.status.success() {
        bail!("git rev-parse HEAD failed");
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

fn reply(thread_id: String, body: Option<String>, stdin: bool) -> Result<()> {
    let body = read_body(body, stdin)?;

    let repo_path = find_repo_root()?;
    let session_path = repo_path.join(".review").join("session.json");
    let lock_path = repo_path.join(".review").join(".lock");

    if !session_path.exists() {
        std::process::exit(2);
    }

    let _lock = lgtm_session::acquire_lock(&lock_path)
        .context("Failed to acquire lock")?;

    let mut session = lgtm_session::read_session(&session_path)
        .context("Failed to read session")?;

    if session.status != SessionStatus::InProgress {
        eprintln!("Error: session is not active (status: {:?})", session.status);
        std::process::exit(6);
    }

    let thread = session.threads.iter_mut().find(|t| t.id == thread_id);
    let Some(thread) = thread else {
        eprintln!("Error: thread not found: {thread_id}");
        std::process::exit(4);
    };

    let head = git_head(&repo_path)?;

    let comment = lgtm_session::Comment {
        id: ulid::Ulid::new().to_string(),
        author: lgtm_session::Author::Agent,
        body,
        timestamp: chrono::Utc::now(),
        diff_snapshot: Some(head),
    };

    thread.comments.push(comment);
    session.updated_at = chrono::Utc::now();

    lgtm_session::write_session_atomic(&session_path, &session)
        .context("Failed to write session")?;

    Ok(())
}

fn create_thread(
    file: String,
    line: u32,
    line_end: Option<u32>,
    severity: String,
    body: Option<String>,
    stdin: bool,
) -> Result<()> {
    let body = read_body(body, stdin)?;
    let line_end = line_end.unwrap_or(line);

    let severity = match severity.as_str() {
        "critical" => lgtm_session::Severity::Critical,
        "warning" => lgtm_session::Severity::Warning,
        "info" => lgtm_session::Severity::Info,
        other => bail!("Invalid severity: {other}. Must be critical, warning, or info"),
    };

    let repo_path = find_repo_root()?;
    let session_path = repo_path.join(".review").join("session.json");
    let lock_path = repo_path.join(".review").join(".lock");

    if !session_path.exists() {
        std::process::exit(2);
    }

    let _lock = lgtm_session::acquire_lock(&lock_path)
        .context("Failed to acquire lock")?;

    let mut session = lgtm_session::read_session(&session_path)
        .context("Failed to read session")?;

    if session.status != SessionStatus::InProgress {
        eprintln!("Error: session is not active (status: {:?})", session.status);
        std::process::exit(6);
    }

    let file_path = repo_path.join(&file);
    if !file_path.exists() {
        eprintln!("Error: file not found: {file}");
        std::process::exit(5);
    }

    let contents = std::fs::read_to_string(&file_path)
        .context("Failed to read file")?;
    let lines: Vec<&str> = contents.lines().collect();

    if line == 0 || line as usize > lines.len() {
        eprintln!("Error: line {line} out of range (file has {} lines)", lines.len());
        std::process::exit(5);
    }

    let anchor_context = lines[(line - 1) as usize].to_string();
    let head = git_head(&repo_path)?;
    let thread_id = ulid::Ulid::new().to_string();

    let thread = lgtm_session::Thread {
        id: thread_id.clone(),
        origin: lgtm_session::Origin::Agent,
        severity: Some(severity),
        status: lgtm_session::ThreadStatus::Open,
        file,
        line_start: line,
        line_end,
        diff_side: lgtm_session::DiffSide::Right,
        anchor_context,
        comments: vec![lgtm_session::Comment {
            id: ulid::Ulid::new().to_string(),
            author: lgtm_session::Author::Agent,
            body,
            timestamp: chrono::Utc::now(),
            diff_snapshot: Some(head),
        }],
    };

    session.threads.push(thread);
    session.updated_at = chrono::Utc::now();

    lgtm_session::write_session_atomic(&session_path, &session)
        .context("Failed to write session")?;

    println!("{thread_id}");
    Ok(())
}

fn detect_base_branch(repo_path: &std::path::Path) -> Result<String> {
    for branch in ["main", "master"] {
        let output = std::process::Command::new("git")
            .args(["rev-parse", "--verify", &format!("refs/heads/{branch}")])
            .current_dir(repo_path)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .context("Failed to run git")?;
        if output.success() {
            return Ok(branch.to_string());
        }
    }
    bail!("Could not detect base branch: neither 'main' nor 'master' exists. Use --base to specify.")
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
