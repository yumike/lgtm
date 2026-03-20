use assert_cmd::Command;
use tempfile::TempDir;

/// Set up a temp git repo with an initial commit and a source file.
/// Returns the TempDir (keep alive to prevent cleanup).
fn setup_repo() -> TempDir {
    let dir = TempDir::new().unwrap();
    let d = dir.path();

    let git = |args: &[&str]| {
        std::process::Command::new("git")
            .args(args)
            .current_dir(d)
            .output()
            .unwrap();
    };

    git(&["init", "-b", "main"]);
    git(&[
        "-c", "user.name=Test",
        "-c", "user.email=t@t.com",
        "commit", "--allow-empty", "-m", "init",
    ]);
    std::fs::write(d.join("main.rs"), "fn main() {\n    println!(\"hello\");\n}\n").unwrap();
    git(&["add", "."]);
    git(&[
        "-c", "user.name=Test",
        "-c", "user.email=t@t.com",
        "commit", "-m", "add main.rs",
    ]);

    dir
}

/// Write a minimal session.json into .review/
fn write_session(dir: &std::path::Path, json: &str) {
    let review_dir = dir.join(".review");
    std::fs::create_dir_all(&review_dir).unwrap();
    std::fs::write(review_dir.join("session.json"), json).unwrap();
}

fn session_json_with_thread() -> String {
    r#"{
        "version": 1,
        "status": "in_progress",
        "base": "main",
        "head": "main",
        "merge_base": "abc1234",
        "created_at": "2026-03-18T14:00:00Z",
        "updated_at": "2026-03-18T14:00:00Z",
        "threads": [
            {
                "id": "t_TEST001",
                "origin": "developer",
                "status": "open",
                "file": "main.rs",
                "line_start": 1,
                "line_end": 1,
                "diff_side": "right",
                "anchor_context": "fn main() {",
                "comments": [
                    {
                        "id": "c_TEST001",
                        "author": "developer",
                        "body": "Add error handling",
                        "timestamp": "2026-03-18T14:22:00Z"
                    }
                ]
            }
        ],
        "files": {}
    }"#.to_string()
}

fn lgtm() -> Command {
    Command::cargo_bin("lgtm").unwrap()
}

// --- status tests ---

#[test]
fn status_no_session_exits_2() {
    let dir = setup_repo();
    lgtm()
        .args(["status", "--json"])
        .current_dir(dir.path())
        .assert()
        .code(2);
}

#[test]
fn status_json_returns_session_data() {
    let dir = setup_repo();
    write_session(dir.path(), &session_json_with_thread());

    let output = lgtm()
        .args(["status", "--json"])
        .current_dir(dir.path())
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let json: serde_json::Value = serde_json::from_slice(&output).unwrap();
    assert_eq!(json["session_status"], "in_progress");
    assert_eq!(json["stats"]["total_threads"], 1);
    assert_eq!(json["stats"]["open"], 1);
    assert_eq!(json["open_threads"].as_array().unwrap().len(), 1);
}

#[test]
fn status_human_readable() {
    let dir = setup_repo();
    write_session(dir.path(), &session_json_with_thread());

    let output = lgtm()
        .arg("status")
        .current_dir(dir.path())
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let text = String::from_utf8(output).unwrap();
    assert!(text.contains("reviewing main against main"));
    assert!(text.contains("1 open"));
    assert!(text.contains("0/0 files reviewed"));
}

// --- reply tests ---

#[test]
fn reply_appends_agent_comment() {
    let dir = setup_repo();
    write_session(dir.path(), &session_json_with_thread());

    lgtm()
        .args(["reply", "t_TEST001", "Fixed the error handling"])
        .current_dir(dir.path())
        .assert()
        .success();

    let session_path = dir.path().join(".review/session.json");
    let session: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(session_path).unwrap()).unwrap();

    let comments = session["threads"][0]["comments"].as_array().unwrap();
    assert_eq!(comments.len(), 2);
    assert_eq!(comments[1]["author"], "agent");
    assert_eq!(comments[1]["body"], "Fixed the error handling");
    assert!(comments[1]["diff_snapshot"].is_string());
}

#[test]
fn reply_thread_not_found_exits_4() {
    let dir = setup_repo();
    write_session(dir.path(), &session_json_with_thread());

    lgtm()
        .args(["reply", "t_NONEXISTENT", "hello"])
        .current_dir(dir.path())
        .assert()
        .code(4);
}

#[test]
fn reply_no_session_exits_2() {
    let dir = setup_repo();

    lgtm()
        .args(["reply", "t_TEST001", "hello"])
        .current_dir(dir.path())
        .assert()
        .code(2);
}

#[test]
fn reply_stdin() {
    let dir = setup_repo();
    write_session(dir.path(), &session_json_with_thread());

    lgtm()
        .args(["reply", "t_TEST001", "--stdin"])
        .write_stdin("Comment from stdin")
        .current_dir(dir.path())
        .assert()
        .success();

    let session_path = dir.path().join(".review/session.json");
    let session: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(session_path).unwrap()).unwrap();

    let comments = session["threads"][0]["comments"].as_array().unwrap();
    assert_eq!(comments[1]["body"], "Comment from stdin");
}

// --- thread tests ---

#[test]
fn thread_creates_agent_thread() {
    let dir = setup_repo();
    write_session(dir.path(), &session_json_with_thread());

    let output = lgtm()
        .args([
            "thread",
            "--file", "main.rs",
            "--line", "1",
            "--severity", "warning",
            "Missing return type",
        ])
        .current_dir(dir.path())
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    // Should print thread ID to stdout
    let thread_id = String::from_utf8(output).unwrap().trim().to_string();
    assert!(!thread_id.is_empty());

    let session_path = dir.path().join(".review/session.json");
    let session: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(session_path).unwrap()).unwrap();

    let threads = session["threads"].as_array().unwrap();
    assert_eq!(threads.len(), 2);

    let new_thread = &threads[1];
    assert_eq!(new_thread["id"], thread_id);
    assert_eq!(new_thread["origin"], "agent");
    assert_eq!(new_thread["severity"], "warning");
    assert_eq!(new_thread["file"], "main.rs");
    assert_eq!(new_thread["anchor_context"], "fn main() {");
    assert_eq!(new_thread["comments"][0]["body"], "Missing return type");
}

#[test]
fn thread_file_not_found_exits_5() {
    let dir = setup_repo();
    write_session(dir.path(), &session_json_with_thread());

    lgtm()
        .args([
            "thread",
            "--file", "nonexistent.rs",
            "--line", "1",
            "--severity", "info",
            "test",
        ])
        .current_dir(dir.path())
        .assert()
        .code(5);
}

#[test]
fn thread_line_out_of_range_exits_5() {
    let dir = setup_repo();
    write_session(dir.path(), &session_json_with_thread());

    lgtm()
        .args([
            "thread",
            "--file", "main.rs",
            "--line", "999",
            "--severity", "info",
            "test",
        ])
        .current_dir(dir.path())
        .assert()
        .code(5);
}

#[test]
fn thread_invalid_severity_exits_1() {
    let dir = setup_repo();
    write_session(dir.path(), &session_json_with_thread());

    lgtm()
        .args([
            "thread",
            "--file", "main.rs",
            "--line", "1",
            "--severity", "extreme",
            "test",
        ])
        .current_dir(dir.path())
        .assert()
        .code(1);
}

// --- fetch tests ---

#[test]
fn fetch_no_session_exits_2() {
    let dir = setup_repo();
    lgtm()
        .arg("fetch")
        .current_dir(dir.path())
        .assert()
        .code(2);
}

#[test]
fn fetch_abandoned_session_exits_6() {
    let dir = setup_repo();
    let json = r#"{
        "version": 1,
        "status": "abandoned",
        "base": "main",
        "head": "feature/test",
        "merge_base": "abc1234",
        "created_at": "2026-03-18T14:00:00Z",
        "updated_at": "2026-03-18T14:00:00Z",
        "threads": [],
        "files": {}
    }"#;
    write_session(dir.path(), json);
    lgtm()
        .arg("fetch")
        .current_dir(dir.path())
        .assert()
        .code(6);
}

#[test]
fn fetch_approved_session_exits_6() {
    let dir = setup_repo();
    let json = r#"{
        "version": 1,
        "status": "approved",
        "base": "main",
        "head": "feature/test",
        "merge_base": "abc1234",
        "created_at": "2026-03-18T14:00:00Z",
        "updated_at": "2026-03-18T14:00:00Z",
        "threads": [],
        "files": {}
    }"#;
    write_session(dir.path(), json);
    lgtm()
        .arg("fetch")
        .current_dir(dir.path())
        .assert()
        .code(6);
}

#[test]
fn fetch_returns_immediately_when_marker_exists() {
    let dir = setup_repo();
    write_session(dir.path(), &session_json_with_thread());
    // Create the submit marker
    std::fs::write(dir.path().join(".review/.submit"), "").unwrap();

    let output = lgtm()
        .arg("fetch")
        .current_dir(dir.path())
        .assert()
        .code(0)
        .get_output()
        .stdout
        .clone();

    let result: serde_json::Value = serde_json::from_slice(&output).unwrap();
    assert_eq!(result["session_status"], "in_progress");
    assert!(!result["open_threads"].as_array().unwrap().is_empty());
    // Marker should be deleted
    assert!(!dir.path().join(".review/.submit").exists());
}

#[test]
fn fetch_timeout_returns_timed_out() {
    let dir = setup_repo();
    write_session(dir.path(), &session_json_with_thread());

    let output = lgtm()
        .args(["fetch", "--timeout", "1"])
        .current_dir(dir.path())
        .assert()
        .code(0)
        .get_output()
        .stdout
        .clone();

    let result: serde_json::Value = serde_json::from_slice(&output).unwrap();
    assert_eq!(result["timed_out"], true);
    assert!(result["open_threads"].as_array().unwrap().is_empty());
}

#[test]
fn fetch_unblocks_when_marker_created() {
    let dir = setup_repo();
    write_session(dir.path(), &session_json_with_thread());

    let review_dir = dir.path().join(".review");
    let submit_path = review_dir.join(".submit");

    // Spawn a thread that creates the marker after 500ms
    let submit_path_clone = submit_path.clone();
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(500));
        std::fs::write(&submit_path_clone, "").unwrap();
    });

    let output = lgtm()
        .args(["fetch", "--timeout", "5"])
        .current_dir(dir.path())
        .assert()
        .code(0)
        .get_output()
        .stdout
        .clone();

    let result: serde_json::Value = serde_json::from_slice(&output).unwrap();
    assert_eq!(result["session_status"], "in_progress");
    assert!(!result["open_threads"].as_array().unwrap().is_empty());
    // Marker should be deleted
    assert!(!submit_path.exists());
}

// --- abandoned session tests ---

#[test]
fn reply_to_abandoned_session_exits_6() {
    let dir = setup_repo();
    let json = session_json_with_thread().replace("in_progress", "abandoned");
    write_session(dir.path(), &json);

    lgtm()
        .args(["reply", "t_TEST001", "hello"])
        .current_dir(dir.path())
        .assert()
        .code(6);
}

#[test]
fn thread_on_abandoned_session_exits_6() {
    let dir = setup_repo();
    let json = session_json_with_thread().replace("in_progress", "abandoned");
    write_session(dir.path(), &json);

    lgtm()
        .args([
            "thread",
            "--file", "main.rs",
            "--line", "1",
            "--severity", "info",
            "test",
        ])
        .current_dir(dir.path())
        .assert()
        .code(6);
}
