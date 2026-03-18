# lgtm — local code review tool for AI-assisted development

## Vision

**lgtm** is a local code review tool that gives developers a GitHub-like review experience for branch changes on their machine, with a bidirectional conversation loop between the developer and an AI coding agent (Claude Code).

The developer must own every line of code before it reaches a pull request. lgtm enforces this by making the review-fix-verify cycle fast, local, and conversational.

## Core workflow

```
┌─────────────┐     ┌──────────────────┐     ┌─────────────┐
│  Claude Code │     │  .review/session │     │  Developer   │
│  (agent)     │◄───►│  .json           │◄───►│  (browser)   │
└──────┬───────┘     └──────────────────┘     └──────┬───────┘
       │                                             │
       │         ┌──────────────────┐                │
       └────────►│  git working tree │◄───────────────┘
                 └──────────────────┘
```

1. Developer runs `lgtm start --base main`
2. Browser opens with a diff view of `HEAD` vs `base`
3. Developer reviews file by file, leaves threaded comments on specific lines
4. Developer signals "ready for agent" (via UI or CLI)
5. Claude Code reads `.review/session.json`, addresses each open thread
6. Claude Code writes code fixes and replies to threads in the same file
7. UI auto-refreshes — developer sees new diff + agent responses
8. Repeat until all threads are resolved
9. Developer runs `lgtm approve` — session closes, branch is ready for PR

---

## 1. Session file — the contract

The session file is the single source of truth. Both the web UI and Claude Code read and write it. No database, no API — just a JSON file on disk.

### Location

```
<repo-root>/.review/session.json
```

The `.review/` directory is always at the repository root. It should be added to `.gitignore`.

### Schema

```jsonc
{
  // Session metadata
  "version": 1,
  "created_at": "2026-03-18T14:00:00Z",
  "updated_at": "2026-03-18T14:35:00Z",
  "status": "in_progress",          // "in_progress" | "approved" | "abandoned"

  // Branch context
  "base": "main",                    // base ref to diff against
  "head": "feature/TASK-123",        // current branch (auto-detected)
  "merge_base": "abc1234",           // computed merge-base commit hash

  // Review state
  "threads": [
    {
      "id": "t_01J8XYZABC",          // ULID — sortable, unique
      "origin": "developer",         // "developer" | "agent" — who created this thread
      "severity": null,              // null for developer threads; "critical" | "warning" | "info" for agent threads
      "status": "open",              // "open" | "resolved" | "wontfix" | "dismissed"
      "file": "src/services/auth.py",
      "line_start": 42,              // 1-indexed, refers to the NEW file
      "line_end": 48,                // inclusive; same as line_start for single-line
      "diff_side": "right",          // "left" (deletion) | "right" (addition/context)
      "anchor_context": "def retry_request(self, url, max_retries=10):",  // line content for re-anchoring after code changes
      "comments": [
        {
          "id": "c_01J8XYZ001",
          "author": "developer",     // "developer" | "agent"
          "body": "This retry logic has no backoff — it will hammer the service under failure conditions.",
          "timestamp": "2026-03-18T14:22:00Z"
        },
        {
          "id": "c_01J8XYZ002",
          "author": "agent",
          "body": "Added exponential backoff with jitter (base 1s, max 30s). See lines 42-67.",
          "timestamp": "2026-03-18T14:22:35Z",
          "diff_snapshot": "abc1234..def5678"  // commit range this reply refers to
        },
        {
          "id": "c_01J8XYZ003",
          "author": "developer",
          "body": "Good. Resolve.",
          "timestamp": "2026-03-18T14:23:10Z"
        }
      ]
    },
    {
      "id": "t_01J8XYZ999",
      "origin": "agent",
      "severity": "warning",
      "status": "open",
      "file": "src/services/auth.py",
      "line_start": 71,
      "line_end": 71,
      "diff_side": "right",
      "anchor_context": "API_KEY = \"sk-prod-abc123\"",
      "comments": [
        {
          "id": "c_01J8XYZ990",
          "author": "agent",
          "body": "Hardcoded API key detected. This should be loaded from environment variables or a secrets manager.",
          "timestamp": "2026-03-18T14:22:40Z",
          "diff_snapshot": "abc1234..def5678"
        }
      ]
    }
  ],

  // Summary counters (computed, for quick status display)
  "stats": {
    "total_threads": 12,
    "open": 3,
    "resolved": 8,
    "wontfix": 1,
    "dismissed": 0,
    "agent_initiated": 2,
    "files_reviewed": 7,
    "files_total": 11
  },

  // File-level tracking (v1 — essential for large diffs)
  "files": {
    "src/services/auth.py": {
      "status": "reviewed",           // "pending" | "reviewed"
      "reviewed_at": "2026-03-18T14:20:00Z",
      "reviewed_hash": "a1b2c3d"      // blob hash when marked reviewed; if file changes, UI resets to "pending"
    },
    "src/models/user.py": {
      "status": "pending"
    }
  }
}
```

### Design decisions

**Why ULID for IDs?** Sortable by creation time, no coordination needed. Both the UI and agent can generate IDs independently without collision risk.

**Why `anchor_context`?** When Claude Code modifies a file, line numbers shift. The UI needs to re-anchor threads to the correct lines after a code change. This is the same problem GitHub solves with "outdated diff" markers — we solve it by storing a snippet of the original line content and performing fuzzy re-anchoring.

**Why `diff_snapshot` on agent replies?** So the UI can show which version of the code the agent was responding to, and detect when a reply is outdated relative to the current diff.

**Why `files` tracking?** Developer needs to see at a glance which files they haven't looked at yet. This supports a "mark as reviewed" workflow — similar to GitHub's file-level checkboxes. The `reviewed_hash` field stores the git blob hash at the time the developer marked the file reviewed. If the agent subsequently modifies the file, the blob hash changes and the UI automatically resets the file to "pending" — forcing the developer to re-review. This is critical: the developer must see every change, including fixes made after their initial pass.

**Why agent-initiated threads?** While the developer is the primary reviewer, the agent may discover real issues while fixing other threads — a hardcoded secret, a missing null check, a broken import. These are tagged with `origin: "agent"` and a `severity` level so the UI can visually distinguish them (e.g., with a robot icon and a severity badge). The developer can `dismiss` agent-initiated threads (equivalent to "I've seen this, it's fine") or `resolve` them (if they agree and want the agent to fix it). Agent-initiated threads do not block `lgtm approve` if dismissed.

**Why "dismissed" status?** Only applies to agent-initiated threads. The developer acknowledges the observation but disagrees or considers it out of scope. Different from "wontfix" which means "I agree this is an issue but won't fix it now." Dismissed threads are hidden by default in the UI.

**Why let the agent decide commit granularity?** Different review threads have different scopes. A typo fix and an architectural refactor shouldn't be forced into the same commit strategy. The agent should commit per-thread when changes are independent and self-contained, and batch when multiple threads touch the same code or are logically related. The skill file provides guidance but not a hard rule.

### Concurrency rules

Both the UI server and Claude Code may write to the session file. Conflicts are handled by:

1. **Atomic writes** — write to `.review/session.json.tmp`, then rename.
2. **Append-only threads** — neither side deletes or reorders comments. New comments are appended. New threads are appended to the `threads` array. Status changes are the only mutations on existing data.
3. **Last-writer-wins for status fields** — `thread.status`, `file.status`, `session.status`. In practice: developer resolves/dismisses threads and marks files reviewed; agent creates threads and adds comments. Minimal conflict surface.
4. **Lock file** — `.review/.lock` with PID, for advisory locking during writes. Both sides acquire before writing, release after.

---

## 2. CLI

### Commands

```
lgtm start [--base <ref>] [--port <port>]
```

Starts a review session:
- Computes merge-base between `HEAD` and `<base>` (default: auto-detect main/master)
- Creates `.review/session.json` if not exists, or resumes existing session
- Starts the local web server
- Opens browser
- Starts file watcher on `.review/session.json` and the working tree

```
lgtm status
```

Prints summary to terminal:
```
lgtm: reviewing feature/TASK-123 against main
  12 threads: 3 open, 8 resolved, 1 wontfix
  7/11 files reviewed
  Session started 35 minutes ago
```

```
lgtm approve
```

Sets `session.status = "approved"`. Requires:
- All developer-initiated threads are resolved or wontfix
- All agent-initiated threads are resolved or dismissed
- All files are marked as reviewed

If conditions are not met, prints what's remaining.

```
lgtm abandon
```

Closes the session without approval. Sets `session.status = "abandoned"`.

```
lgtm clean
```

Removes `.review/` directory entirely.

```
lgtm diff [--stat]
```

Prints the raw diff to terminal (for piping to other tools). `--stat` shows files-changed summary.

### Global flags

| Flag | Default | Description |
|------|---------|-------------|
| `--base <ref>` | auto | Base branch or commit to diff against |
| `--port <port>` | 4567 | Web server port |
| `--host <host>` | 127.0.0.1 | Bind address |
| `--no-open` | false | Don't open browser automatically |

### Exit codes

| Code | Meaning |
|------|---------|
| 0 | Success |
| 1 | General error |
| 2 | Session not found (for `status`, `approve`) |
| 3 | Approval blocked — open threads or unreviewed files remain |

---

## 3. Web UI

### Layout

```
┌─────────────────────────────────────────────────────────┐
│  lgtm  ·  feature/TASK-123 → main  ·  3 open · 8 done  │
├──────────────┬──────────────────────────────────────────┤
│              │                                          │
│  File tree   │  Diff view                               │
│              │                                          │
│  ☐ auth.py   │  @@ -40,6 +40,12 @@                     │
│  ☑ user.py   │   def retry_request(self, ...):          │
│  ☐ config.py │  +    backoff = min(2**attempt, 30)      │
│  ...         │  +    sleep(backoff + random())           │
│              │                                          │
│              │  ┌─ Thread ──────────────────────┐       │
│              │  │ 👤 No backoff, will hammer     │       │
│              │  │ 🤖 Added exponential backoff   │       │
│              │  │ 👤 Good. Resolve.              │       │
│              │  │ Status: ✅ resolved            │       │
│              │  └────────────────────────────────┘       │
│              │                                          │
├──────────────┴──────────────────────────────────────────┤
│  Ready for agent  ·  Approve session                    │
└─────────────────────────────────────────────────────────┘
```

### File tree (left panel)

- Shows all files in the diff, grouped by directory
- Checkbox per file: ☐ pending, ☑ reviewed
- Badge showing thread count per file (e.g., `auth.py (3)`)
- Color coding: red dot = has open threads, green = all resolved, gray = no threads
- Click to navigate to file diff

### Diff view (main panel)

- Split (side-by-side) or unified diff, togglable
- Syntax highlighting (Prism.js or Shiki)
- Click on line number gutter to start a new comment (single line)
- Click-drag across line numbers to select a range
- Inline thread display: threads render between diff hunks at their anchor point
- Collapsed/expanded toggle per thread
- Auto-collapse resolved threads
- "Outdated" badge on threads whose `line_start` was re-anchored after a code change

### Thread interaction

- **New comment**: click line gutter → text input opens → submit writes to session file
- **Reply**: click "Reply" on existing thread → appends comment with `author: "developer"`
- **Resolve**: button on thread → sets `thread.status = "resolved"`
- **Wontfix**: button on thread → sets `thread.status = "wontfix"` (developer explicitly accepts the current state)
- **Dismiss**: button on agent-initiated threads only → sets `thread.status = "dismissed"` (developer acknowledges but disagrees)
- **Reopen**: button on resolved/wontfix/dismissed thread → sets back to `"open"`

### Agent-initiated threads

Threads with `origin: "agent"` render with distinct visual treatment:
- Robot icon (🤖) instead of user icon in the thread header
- Severity badge: 🔴 critical, 🟡 warning, 🔵 info
- "Dismiss" button (not available on developer-initiated threads)
- Grouped separately in file tree badges: e.g., `auth.py (2 + 1🤖)`
- Dismissed threads are hidden by default, togglable via filter

### Status bar (bottom)

- Thread summary: `3 open · 8 resolved · 1 wontfix · 1 dismissed`
- **"Ready for agent"** button: no-op in the tool itself — visual indicator to developer that they can now switch to Claude Code. Optionally writes a `.review/ready` marker file that Claude Code skill can watch for.
- **"Approve session"** button: equivalent to `lgtm approve`. Enabled only when all developer-initiated threads are resolved or wontfix, all agent-initiated threads are resolved or dismissed, and all files are reviewed.

### Real-time updates

- WebSocket connection between browser and local server
- Server watches `.review/session.json` via `chokidar` / `notify`
- On change: server re-reads file, pushes delta to browser
- Server also watches git working tree — on file change, recomputes diff and pushes updated hunks
- Debounce: 300ms for session file changes, 500ms for git tree changes

---

## 4. Claude Code integration protocol

### Skill file (CLAUDE.md or .review/AGENT.md)

Claude Code discovers the review protocol via a skill file in the repo. This file teaches Claude Code how to participate in the review loop.

```markdown
# lgtm review protocol

When a file `.review/session.json` exists in the repository root,
there is an active code review session. Follow this protocol:

## Reading the review

1. Read `.review/session.json`
2. Find all threads where `status == "open"`
3. For each open thread, read the full comment history to understand the conversation

## Addressing comments

For each open thread:

1. Read the referenced file and lines (`file`, `line_start`, `line_end`)
2. Understand what the developer is asking for
3. Make the code change
4. Add a reply comment to the thread:

   ```json
   {
     "id": "<generate ULID>",
     "author": "agent",
     "body": "<explain what you changed and why>",
     "timestamp": "<ISO 8601>",
     "diff_snapshot": "<current HEAD commit hash>"
   }
   ```

5. Append the comment to the thread's `comments` array
6. Do NOT change the thread's `status` — only the developer resolves threads
7. Update `session.updated_at` and `stats`

## Raising observations

If you discover an issue while fixing a thread — a bug, security problem,
missing error handling, or anything the developer should know about — create
a new thread:

```json
{
  "id": "<generate ULID>",
  "origin": "agent",
  "severity": "critical | warning | info",
  "status": "open",
  "file": "<file path>",
  "line_start": <line>,
  "line_end": <line>,
  "diff_side": "right",
  "anchor_context": "<content of the line>",
  "comments": [
    {
      "id": "<generate ULID>",
      "author": "agent",
      "body": "<describe the issue clearly>",
      "timestamp": "<ISO 8601>",
      "diff_snapshot": "<current HEAD commit hash>"
    }
  ]
}
```

Severity guide:
- **critical**: Security issues, data loss risk, broken functionality
- **warning**: Performance problems, missing error handling, code smells
- **info**: Style suggestions, minor improvements, questions

Do NOT fix agent-initiated issues yourself unless the developer asks.
Raise the thread, describe the problem, and let the developer decide.

## Commit strategy

Use your judgment based on the scope of changes:
- **Independent, self-contained fixes** → commit per thread
  (e.g., "fix: add retry backoff per review thread t_01J8XYZABC")
- **Multiple threads touching the same code** → batch into one commit
  (e.g., "fix: address auth service review comments")
- **Trivial fixes** (typos, imports, formatting) → batch together
- Include `[lgtm]` prefix in commit messages for traceability

## Writing the session file

1. Read the current session file
2. Modify only: add comments to threads, add new agent-initiated threads,
   update `updated_at` and `stats`
3. Write to `.review/session.json.tmp`
4. Rename to `.review/session.json`

## Rules

- Never mark a thread as resolved or dismissed — that is the developer's decision
- Never delete or edit existing comments
- Never modify the `base`, `head`, or `merge_base` fields
- If a thread references lines that no longer exist, explain in your reply
  that the code has moved and reference the new location
- Keep replies concise — state what changed and where, not lengthy explanations
- If you disagree with a comment, explain your reasoning — the developer
  will decide whether to resolve or continue the discussion
- Only create agent-initiated threads for genuine issues — not style
  preferences or trivial observations
- After addressing all open threads, stop and wait for the developer
  to review your changes in the lgtm UI
```

### Agent workflow

Claude Code's interaction follows this cycle:

```
1. Developer says: "address the review comments"
   (or agent checks for .review/ready marker)

2. Agent reads .review/session.json

3. For each open thread:
   a. Read the file + lines referenced
   b. Read the full thread conversation
   c. Make code changes (normal file editing)
   d. Append a reply comment to the thread

4. If agent spots additional issues while fixing:
   a. Create new agent-initiated threads with appropriate severity
   b. Do NOT fix these — just flag them for the developer

5. Decide commit strategy:
   - Independent fixes → commit per thread with [lgtm] prefix
   - Related changes → batch commit
   - Trivial fixes → batch together

6. Write updated session.json atomically

7. Report summary: "Addressed 3 threads in 2 files.
   Raised 1 new observation (warning). 
   Please review in the lgtm UI."

8. Wait for developer to review and respond
```

### What the agent must NOT do

- Resolve, dismiss, or close threads (developer ownership)
- Delete or edit existing comments from the developer
- Change session metadata (base, head, merge_base)
- Remove the session file
- Fix issues it flagged as agent-initiated (unless developer asks)
- Spam trivial observations — only raise threads for genuine issues

---

## 5. Diff computation

### Merge-base strategy

```
merge_base = $(git merge-base HEAD <base>)
diff = $(git diff <merge_base>..HEAD)
```

This matches how GitHub computes PR diffs — showing only the changes introduced by the branch, not changes from the base that haven't been merged in.

### Re-anchoring after code changes

When the agent modifies code, line numbers shift. The re-anchoring algorithm:

1. For each thread, take `anchor_context` (the original line content)
2. Search the new version of `thread.file` for that content
3. If exact match: update `line_start` and `line_end` based on the offset
4. If fuzzy match (Levenshtein distance < 3): update with "approximate" flag, show "outdated" badge in UI
5. If no match: mark thread as "orphaned" — show in UI with warning, developer must manually re-anchor or resolve

### Incremental diff updates

When the file watcher detects changes in the working tree:

1. Re-run `git diff <merge_base>..HEAD` for changed files only
2. Diff the new hunks against the previous hunks to compute a delta
3. Push only changed hunks over WebSocket
4. Re-anchor all threads in affected files

---

## 6. File watching

### Watched paths

| Path | Events | Action |
|------|--------|--------|
| `.review/session.json` | modify | Re-read session, push to UI via WebSocket |
| `<tracked files in diff>` | modify, create, delete | Recompute diff for affected files, re-anchor threads, push to UI |
| `.review/ready` | create | (optional) Notify agent that developer has finished commenting |

### Debouncing

- Session file: 300ms debounce (agent may write multiple comments rapidly)
- Working tree: 500ms debounce (editor autosave, formatter, etc.)
- Git index: 1s debounce (staging operations)

### Ignored paths

- `.git/` internals (except HEAD, refs — for branch detection)
- `node_modules/`, `__pycache__/`, build directories
- Files matching `.gitignore` patterns

---

## 7. Tech stack

### Option A: Rust (recommended for distribution)

| Component | Crate |
|-----------|-------|
| CLI | `clap` |
| Web server | `axum` |
| WebSocket | `axum` built-in + `tokio-tungstenite` |
| Git operations | `git2` (libgit2 bindings) |
| File watching | `notify` |
| Diff parsing | `similar` or shell out to `git diff` |
| Template/frontend | Serve static SPA (React or Svelte, built separately) |
| ULID generation | `ulid` |
| JSON handling | `serde` + `serde_json` |

Single binary, no runtime dependencies. Distribute via `cargo install lgtm` or direct download.

### Option B: Node/TypeScript (faster to prototype)

| Component | Package |
|-----------|---------|
| CLI | `commander` |
| Web server | `express` |
| WebSocket | `ws` |
| Git operations | `simple-git` |
| File watching | `chokidar` |
| Diff parsing | `diff2html` |
| Frontend | React + Vite + Tailwind |
| ULID generation | `ulid` |

Distribute via `npx lgtm` or `npm install -g lgtm-dev`.

---

## 8. Security considerations

- Server binds to `127.0.0.1` by default — not accessible from network
- No authentication (local tool, single user)
- Session file contains no secrets — only file paths, line numbers, review comments
- `.review/` directory should be in `.gitignore` to prevent accidental commit of review sessions
- Agent skill file should be committed to repo — it's part of the team's development protocol

---

## 9. Future extensions (out of scope for v1)

- **MCP server**: Expose review state as MCP tools (`get_open_threads`, `reply_to_thread`, `get_diff`) for tighter agent integration
- **Multi-reviewer**: Support multiple developer names, not just "developer"
- **Review templates**: Pre-defined checklists (security review, performance review) that auto-create threads
- **Gitea/GitHub integration**: Push approved reviews as PR comments after `lgtm approve`
- **Metrics**: Track review cycle time, threads per session, agent accuracy
- **VS Code extension**: Sidebar panel instead of browser UI
- **Thread categories**: `bug`, `style`, `question`, `suggestion` — matching GitHub's review comment types

---

## 10. Resolved decisions

1. **Commit granularity**: Agent decides based on scope. Independent fixes get per-thread commits, related changes are batched. Commit messages include `[lgtm]` prefix for traceability.

2. **Agent-initiated threads**: Yes. Agent can create threads tagged with `origin: "agent"` and a severity level. Developer can dismiss (instead of resolve) if they disagree. Agent does NOT fix its own observations unless asked.

3. **File-level tracking**: Yes for v1. Per-file "reviewed" checkbox with `reviewed_hash` for automatic reset when the file changes after review. All files must be reviewed before `lgtm approve` succeeds.

## 11. Open questions

1. **Session persistence across rebases**: If the developer rebases the branch mid-review, all commit hashes change. Should lgtm detect this and attempt to migrate the session, or require a fresh start?

2. **Comment syntax**: Should lgtm support markdown in comments? Or keep it plain text for simplicity and to avoid rendering complexity?

3. **Thread ordering in UI**: Should agent-initiated threads be interleaved with developer threads (by position in file) or grouped separately (e.g., an "Agent observations" section at the bottom of each file)?

4. **Batch dismiss**: Should the UI offer "dismiss all agent observations" for cases where the developer trusts the code and wants to move fast? Risk: defeats the purpose of the review.
