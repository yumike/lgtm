use clap::Parser;
use tracing_subscriber::EnvFilter;

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
    },
    /// Show review session status
    Status {
        /// Output as JSON
        #[arg(long)]
        json: bool,
    },
    /// Wait for developer to submit review comments, then print open threads
    Fetch {
        /// Timeout in seconds (default: wait indefinitely)
        #[arg(long)]
        timeout: Option<u64>,
    },
    /// Reply to a review thread
    Reply {
        /// Thread ID
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
    /// Approve the current review session
    Approve,
    /// Abandon the current review session
    Abandon,
    /// Show the diff for the current session
    Diff {
        /// Show diffstat summary only
        #[arg(long)]
        stat: bool,
    },
    /// Delete the current review session
    Clean,
}

fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    let cli = Cli::parse();

    let result = match cli.command {
        Commands::Start { base } => cmd_start(base),
        Commands::Status { json } => cmd_status(json),
        Commands::Fetch { timeout } => cmd_fetch(timeout),
        Commands::Reply { thread_id, body, stdin } => cmd_reply(thread_id, body, stdin),
        Commands::Thread { file, line, line_end, severity, body, stdin } => {
            cmd_thread(file, line, line_end, severity, body, stdin)
        }
        Commands::Approve => cmd_approve(),
        Commands::Abandon => cmd_abandon(),
        Commands::Diff { stat } => cmd_diff(stat),
        Commands::Clean => cmd_clean(),
    };

    if let Err(msg) = result {
        eprintln!("Error: {msg}");
        std::process::exit(1);
    }
}

// ---------------------------------------------------------------------------
// Helper functions
// ---------------------------------------------------------------------------

fn find_repo_root() -> Result<String, String> {
    let output = std::process::Command::new("git")
        .args(["rev-parse", "--show-toplevel"])
        .output()
        .map_err(|e| format!("git failed: {}", e))?;
    if !output.status.success() {
        return Err("not a git repository".into());
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

fn git_head_ref(repo_path: &str) -> Result<String, String> {
    let output = std::process::Command::new("git")
        .args(["symbolic-ref", "--short", "HEAD"])
        .current_dir(repo_path)
        .output()
        .map_err(|e| format!("git failed: {}", e))?;
    if !output.status.success() {
        return Err("failed to get HEAD ref".into());
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

fn discover_server() -> Result<lgtm_server::lockfile::ServerInfo, String> {
    let path = lgtm_server::lockfile::lockfile_path();
    match lgtm_server::lockfile::read_lockfile(&path) {
        Ok(Some(info)) => {
            if lgtm_server::lockfile::is_pid_alive(info.pid) {
                Ok(info)
            } else {
                let _ = lgtm_server::lockfile::remove_lockfile(&path);
                Err("lgtm app not running (stale lockfile cleaned up)".into())
            }
        }
        Ok(None) => Err("lgtm app not running. Launch lgtm-app first.".into()),
        Err(e) => Err(format!("failed to read lockfile: {}", e)),
    }
}

fn launch_app() -> Result<lgtm_server::lockfile::ServerInfo, String> {
    let app_path = which::which("lgtm-app")
        .map_err(|_| "lgtm-app not installed".to_string())?;
    std::process::Command::new(app_path)
        .spawn()
        .map_err(|e| format!("failed to launch: {}", e))?;
    let start = std::time::Instant::now();
    loop {
        if start.elapsed() > std::time::Duration::from_secs(10) {
            return Err("timed out waiting for lgtm app".into());
        }
        std::thread::sleep(std::time::Duration::from_millis(200));
        if let Ok(Some(info)) =
            lgtm_server::lockfile::read_lockfile(&lgtm_server::lockfile::lockfile_path())
        {
            if lgtm_server::lockfile::is_pid_alive(info.pid) {
                return Ok(info);
            }
        }
    }
}

fn discover_or_launch() -> Result<lgtm_server::lockfile::ServerInfo, String> {
    match discover_server() {
        Ok(info) => Ok(info),
        Err(_) => launch_app(),
    }
}

fn base_url(info: &lgtm_server::lockfile::ServerInfo) -> String {
    format!("http://127.0.0.1:{}", info.port)
}

fn resolve_session(
    client: &reqwest::blocking::Client,
    base: &str,
) -> Result<lgtm_session::Session, String> {
    let repo_path = find_repo_root()?;
    let head = git_head_ref(&repo_path)?;
    let resp = client
        .get(format!("{}/api/sessions", base))
        .query(&[("repo_path", &repo_path), ("head", &head)])
        .send()
        .map_err(|e| format!("failed to connect to lgtm app: {}", e))?;
    if !resp.status().is_success() {
        return Err(format!("server returned {}", resp.status()));
    }
    let sessions: Vec<lgtm_session::Session> =
        resp.json().map_err(|e| format!("bad response: {}", e))?;
    sessions
        .into_iter()
        .next()
        .ok_or_else(|| "no active session for this repo/branch".into())
}

fn read_body(body: Option<String>, stdin: bool) -> Result<String, String> {
    if stdin {
        let mut buf = String::new();
        std::io::Read::read_to_string(&mut std::io::stdin(), &mut buf)
            .map_err(|e| format!("stdin read failed: {}", e))?;
        Ok(buf)
    } else {
        body.ok_or_else(|| "body required (provide as arg or use --stdin)".into())
    }
}

// ---------------------------------------------------------------------------
// Command implementations
// ---------------------------------------------------------------------------

fn cmd_start(base_branch: String) -> Result<(), String> {
    let info = discover_or_launch()?;
    let base = base_url(&info);
    let repo_path = find_repo_root()?;

    let client = reqwest::blocking::Client::new();
    let resp = client
        .post(format!("{}/api/sessions", base))
        .json(&serde_json::json!({
            "repo_path": repo_path,
            "base": base_branch,
        }))
        .send()
        .map_err(|e| format!("failed to connect to lgtm app: {}", e))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().unwrap_or_default();
        return Err(format!("server returned {}: {}", status, body));
    }

    let session: lgtm_session::Session =
        resp.json().map_err(|e| format!("bad response: {}", e))?;
    println!("Session started: {}", session.id);
    println!("  {} -> {}", session.base, session.head);
    Ok(())
}

fn cmd_status(json: bool) -> Result<(), String> {
    let info = discover_server()?;
    let base = base_url(&info);
    let client = reqwest::blocking::Client::new();
    let session = resolve_session(&client, &base)?;

    if json {
        let stats = lgtm_session::compute_stats(&session);
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
        println!("{}", serde_json::to_string_pretty(&output).unwrap());
    } else {
        let stats = lgtm_session::compute_stats(&session);
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

        let files_reviewed = session
            .files
            .values()
            .filter(|s| **s == lgtm_session::FileReviewStatus::Reviewed)
            .count();
        let files_total = session.files.len();
        println!("  {}/{} files reviewed", files_reviewed, files_total);
    }

    Ok(())
}

fn cmd_fetch(timeout: Option<u64>) -> Result<(), String> {
    let info = discover_server()?;
    let base = base_url(&info);
    let client = reqwest::blocking::Client::new();
    let session = resolve_session(&client, &base)?;

    let ws_url = format!(
        "ws://127.0.0.1:{}/ws/{}",
        info.port, session.id
    );

    let (mut socket, _response) = tungstenite::connect(&ws_url)
        .map_err(|e| format!("websocket connect failed: {}", e))?;

    let deadline = timeout.map(|s| std::time::Instant::now() + std::time::Duration::from_secs(s));

    loop {
        // Check timeout
        if let Some(deadline) = deadline {
            if std::time::Instant::now() >= deadline {
                let output = serde_json::json!({
                    "timed_out": true,
                    "session_status": session.status,
                    "base": session.base,
                    "head": session.head,
                    "merge_base": session.merge_base,
                    "open_threads": [],
                });
                println!("{}", serde_json::to_string_pretty(&output).unwrap());
                let _ = socket.close(None);
                return Ok(());
            }
        }

        let msg = socket.read();
        match msg {
            Ok(tungstenite::Message::Text(text)) => {
                if let Ok(ws_msg) = serde_json::from_str::<lgtm_server::ws::WsMessage>(&text) {
                    match ws_msg {
                        lgtm_server::ws::WsMessage::SubmitStatus(data) if data.pending => {
                            // Re-fetch session to get latest state
                            let session = resolve_session(&client, &base)?;
                            let open_threads: Vec<&lgtm_session::Thread> = session
                                .threads
                                .iter()
                                .filter(|t| {
                                    t.status == lgtm_session::ThreadStatus::Open
                                })
                                .collect();
                            let output = serde_json::json!({
                                "session_status": session.status,
                                "base": session.base,
                                "head": session.head,
                                "merge_base": session.merge_base,
                                "open_threads": open_threads,
                            });
                            println!("{}", serde_json::to_string_pretty(&output).unwrap());
                            let _ = socket.close(None);
                            return Ok(());
                        }
                        lgtm_server::ws::WsMessage::SessionUpdated(updated_session) => {
                            if updated_session.status != lgtm_session::SessionStatus::InProgress {
                                let output = serde_json::json!({
                                    "session_status": updated_session.status,
                                    "base": updated_session.base,
                                    "head": updated_session.head,
                                    "merge_base": updated_session.merge_base,
                                    "open_threads": [],
                                });
                                println!(
                                    "{}",
                                    serde_json::to_string_pretty(&output).unwrap()
                                );
                                let _ = socket.close(None);
                                return Ok(());
                            }
                        }
                        _ => {}
                    }
                }
            }
            Ok(tungstenite::Message::Close(_)) => {
                return Err("websocket closed by server".into());
            }
            Err(e) => {
                return Err(format!("websocket error: {}", e));
            }
            _ => {}
        }
    }
}

fn cmd_reply(thread_id: String, body: Option<String>, stdin: bool) -> Result<(), String> {
    let body = read_body(body, stdin)?;
    let info = discover_server()?;
    let base = base_url(&info);
    let client = reqwest::blocking::Client::new();
    let session = resolve_session(&client, &base)?;

    let resp = client
        .post(format!(
            "{}/api/sessions/{}/threads/{}/comments",
            base, session.id, thread_id
        ))
        .json(&serde_json::json!({ "body": body }))
        .send()
        .map_err(|e| format!("failed to connect to lgtm app: {}", e))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().unwrap_or_default();
        return Err(format!("server returned {}: {}", status, body));
    }

    Ok(())
}

fn cmd_thread(
    file: String,
    line: u32,
    line_end: Option<u32>,
    severity: String,
    body: Option<String>,
    stdin: bool,
) -> Result<(), String> {
    let body = read_body(body, stdin)?;
    let line_end = line_end.unwrap_or(line);

    // Validate severity
    match severity.as_str() {
        "critical" | "warning" | "info" => {}
        other => return Err(format!("invalid severity: {other}. Must be critical, warning, or info")),
    }

    let repo_path = find_repo_root()?;
    let info = discover_server()?;
    let base = base_url(&info);
    let client = reqwest::blocking::Client::new();
    let session = resolve_session(&client, &base)?;

    // Read anchor context from the file
    let file_path = format!("{}/{}", repo_path, file);
    let contents = std::fs::read_to_string(&file_path)
        .map_err(|e| format!("failed to read file {}: {}", file, e))?;
    let lines: Vec<&str> = contents.lines().collect();
    if line == 0 || line as usize > lines.len() {
        return Err(format!(
            "line {} out of range (file has {} lines)",
            line,
            lines.len()
        ));
    }
    let anchor_context = lines[(line - 1) as usize].to_string();

    let resp = client
        .post(format!("{}/api/sessions/{}/threads", base, session.id))
        .json(&serde_json::json!({
            "file": file,
            "line_start": line,
            "line_end": line_end,
            "diff_side": "right",
            "anchor_context": anchor_context,
            "body": body,
            "origin": "agent",
            "severity": severity,
        }))
        .send()
        .map_err(|e| format!("failed to connect to lgtm app: {}", e))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().unwrap_or_default();
        return Err(format!("server returned {}: {}", status, body));
    }

    let thread: lgtm_session::Thread =
        resp.json().map_err(|e| format!("bad response: {}", e))?;
    println!("{}", thread.id);
    Ok(())
}

fn cmd_approve() -> Result<(), String> {
    let info = discover_server()?;
    let base = base_url(&info);
    let client = reqwest::blocking::Client::new();
    let session = resolve_session(&client, &base)?;

    let resp = client
        .patch(format!("{}/api/sessions/{}", base, session.id))
        .json(&serde_json::json!({ "status": "approved" }))
        .send()
        .map_err(|e| format!("failed to connect to lgtm app: {}", e))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().unwrap_or_default();
        return Err(format!("server returned {}: {}", status, body));
    }

    println!("Session approved");
    Ok(())
}

fn cmd_abandon() -> Result<(), String> {
    let info = discover_server()?;
    let base = base_url(&info);
    let client = reqwest::blocking::Client::new();
    let session = resolve_session(&client, &base)?;

    let resp = client
        .patch(format!("{}/api/sessions/{}", base, session.id))
        .json(&serde_json::json!({ "status": "abandoned" }))
        .send()
        .map_err(|e| format!("failed to connect to lgtm app: {}", e))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().unwrap_or_default();
        return Err(format!("server returned {}: {}", status, body));
    }

    println!("Session abandoned");
    Ok(())
}

fn cmd_diff(stat: bool) -> Result<(), String> {
    let info = discover_server()?;
    let base = base_url(&info);
    let client = reqwest::blocking::Client::new();
    let session = resolve_session(&client, &base)?;

    let resp = client
        .get(format!("{}/api/sessions/{}/diff", base, session.id))
        .send()
        .map_err(|e| format!("failed to connect to lgtm app: {}", e))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().unwrap_or_default();
        return Err(format!("server returned {}: {}", status, body));
    }

    let files: Vec<lgtm_git::DiffFile> =
        resp.json().map_err(|e| format!("bad response: {}", e))?;

    if stat {
        for file in &files {
            println!("{}\t{:?}", file.path, file.status);
        }
        println!("{} files changed", files.len());
    } else {
        println!("{}", serde_json::to_string_pretty(&files).unwrap());
    }

    Ok(())
}

fn cmd_clean() -> Result<(), String> {
    let info = discover_server()?;
    let base = base_url(&info);
    let client = reqwest::blocking::Client::new();
    let session = resolve_session(&client, &base)?;

    let resp = client
        .delete(format!("{}/api/sessions/{}", base, session.id))
        .send()
        .map_err(|e| format!("failed to connect to lgtm app: {}", e))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().unwrap_or_default();
        return Err(format!("server returned {}: {}", status, body));
    }

    println!("Session cleaned up");
    Ok(())
}
