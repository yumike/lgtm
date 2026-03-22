# Playwright E2E Tests Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Automated Playwright tests that verify the core review workflow through the lgtm UI.

**Architecture:** Tests run against the Axum HTTP server in headless mode (no Tauri window). A Playwright fixture creates a temp git repo with branches, starts the server, and provides the base URL. Tests exercise the full workflow: create session, view diff, comment, submit, reply, resolve, approve.

**Tech Stack:** Playwright, TypeScript, Axum (headless mode via `--headless` flag on `lgtm-app`)

---

## File Structure

### New files
- `crates/lgtm-app/src/main.rs` — modify to add `--headless` flag with `LGTM_SESSIONS_DIR` / `LGTM_ASSETS_DIR` env var support
- `packages/web/e2e/review-workflow.spec.ts` — the test file with all 8 scenarios
- `packages/web/e2e/fixtures.ts` — `lgtmServer` fixture: temp git repo, server lifecycle, helpers
- `packages/web/playwright.config.ts` — Playwright configuration

### Modified files
- `packages/web/package.json` — add `@playwright/test` devDependency and `e2e` script
- `Makefile` — add `e2e` target

---

## Chunk 1: Headless Server Mode

### Task 1: Add --headless flag to lgtm-app

**Files:**
- Modify: `crates/lgtm-app/Cargo.toml`
- Modify: `crates/lgtm-app/src/main.rs`

- [ ] **Step 1: Add clap dependency to lgtm-app**

In `crates/lgtm-app/Cargo.toml`, add:
```toml
clap = { version = "4", features = ["derive"] }
```

- [ ] **Step 2: Add --headless flag and env var support**

Rewrite `crates/lgtm-app/src/main.rs`:

```rust
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

fn run_headless(state: Arc<lgtm_server::AppState>, assets_dir: std::path::PathBuf) {
    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async {
        // Re-register diff providers for restored sessions
        for session in state.store.list() {
            let provider = lgtm_git::cli_provider::CliDiffProvider::new(&session.repo_path);
            state.register_session(session.id, Box::new(provider));
            let _ = lgtm_server::watcher::start_watchers(
                state.clone(),
                session.id,
                session.repo_path.clone(),
            );
        }

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
```

- [ ] **Step 3: Verify it compiles**

Run: `cargo build -p lgtm-app`
Expected: compiles

- [ ] **Step 4: Test headless mode manually**

```bash
cd packages/web && npm run build && cd ../..
./target/debug/lgtm-app --headless
```
Expected: prints a port number to stdout and server stays running. Ctrl+C to stop.

- [ ] **Step 5: Commit**

```bash
git add crates/lgtm-app/ && git commit -m "feat(app): add --headless flag for running server without Tauri window"
```

---

## Chunk 2: Playwright Setup and Fixture

### Task 2: Install Playwright and create config

**Files:**
- Modify: `packages/web/package.json`
- Create: `packages/web/playwright.config.ts`

- [ ] **Step 1: Install Playwright**

```bash
cd packages/web && npm install -D @playwright/test && npx playwright install chromium
```

- [ ] **Step 2: Add e2e script to package.json**

Add to `scripts` in `packages/web/package.json`:
```json
"e2e": "playwright test"
```

- [ ] **Step 3: Create playwright.config.ts**

Create `packages/web/playwright.config.ts`:

```typescript
import { defineConfig } from '@playwright/test';

export default defineConfig({
  testDir: './e2e',
  timeout: 30_000,
  retries: 0,
  use: {
    headless: true,
    screenshot: 'only-on-failure',
  },
});
```

- [ ] **Step 4: Commit**

```bash
git add packages/web/package.json packages/web/package-lock.json packages/web/playwright.config.ts
git commit -m "chore(web): add Playwright and config for e2e tests"
```

### Task 3: Create test fixture

**Files:**
- Create: `packages/web/e2e/fixtures.ts`

- [ ] **Step 1: Create the fixture file**

Create `packages/web/e2e/fixtures.ts`:

```typescript
import { test as base, type Page } from '@playwright/test';
import { spawn, execSync, type ChildProcess } from 'child_process';
import { mkdtempSync, writeFileSync, rmSync } from 'fs';
import { join } from 'path';
import { tmpdir } from 'os';

interface LgtmServer {
  baseURL: string;
  port: number;
  /** Create a session for the test repo and return the session id */
  createSession(): Promise<string>;
  /** POST a reply to a thread as the agent */
  agentReply(sessionId: string, threadId: string, body: string): Promise<void>;
}

function createTestRepo(): string {
  const dir = mkdtempSync(join(tmpdir(), 'lgtm-e2e-'));

  execSync('git init', { cwd: dir });
  execSync('git checkout -b main', { cwd: dir });

  writeFileSync(join(dir, 'hello.py'), 'def greet():\n    return "hello"\n');
  execSync('git add -A && git commit -m "initial"', { cwd: dir });

  execSync('git checkout -b feature', { cwd: dir });
  writeFileSync(
    join(dir, 'hello.py'),
    'def greet(name: str) -> str:\n    return f"hello {name}"\n',
  );
  writeFileSync(join(dir, 'utils.py'), 'def helper():\n    pass\n');
  execSync('git add -A && git commit -m "add feature"', { cwd: dir });

  return dir;
}

async function waitForServer(port: number, timeoutMs = 10_000): Promise<void> {
  const start = Date.now();
  while (Date.now() - start < timeoutMs) {
    try {
      const resp = await fetch(`http://127.0.0.1:${port}/api/sessions`);
      if (resp.ok) return;
    } catch {
      // not ready yet
    }
    await new Promise((r) => setTimeout(r, 200));
  }
  throw new Error(`Server did not start within ${timeoutMs}ms`);
}

export const test = base.extend<{ lgtm: LgtmServer }>({
  lgtm: async ({}, use) => {
    const repoDir = createTestRepo();
    const sessionsDir = mkdtempSync(join(tmpdir(), 'lgtm-sessions-'));

    // Path to the built binary
    const binPath = join(__dirname, '../../../target/debug/lgtm-app');
    const assetsDir = join(__dirname, '../dist');

    const proc: ChildProcess = spawn(binPath, ['--headless'], {
      env: {
        ...process.env,
        LGTM_SESSIONS_DIR: sessionsDir,
        LGTM_ASSETS_DIR: assetsDir,
      },
      stdio: ['pipe', 'pipe', 'pipe'],
    });

    // Read port from stdout (first line)
    const port = await new Promise<number>((resolve, reject) => {
      let stdout = '';
      proc.stdout!.on('data', (chunk: Buffer) => {
        stdout += chunk.toString();
        const lines = stdout.split('\n');
        if (lines.length > 0 && lines[0].trim()) {
          const p = parseInt(lines[0].trim(), 10);
          if (!isNaN(p)) resolve(p);
        }
      });
      proc.on('error', reject);
      setTimeout(() => reject(new Error('Timeout reading port')), 10_000);
    });

    await waitForServer(port);

    const baseURL = `http://127.0.0.1:${port}`;

    const server: LgtmServer = {
      baseURL,
      port,

      async createSession() {
        const resp = await fetch(`${baseURL}/api/sessions`, {
          method: 'POST',
          headers: { 'Content-Type': 'application/json' },
          body: JSON.stringify({ repo_path: repoDir, base: 'main' }),
        });
        if (!resp.ok) throw new Error(`Create session failed: ${resp.status}`);
        const session = await resp.json();
        return session.id;
      },

      async agentReply(sessionId: string, threadId: string, body: string) {
        const resp = await fetch(
          `${baseURL}/api/sessions/${sessionId}/threads/${threadId}/comments`,
          {
            method: 'POST',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify({ body, author: 'agent' }),
          },
        );
        if (!resp.ok) throw new Error(`Agent reply failed: ${resp.status}`);
      },
    };

    await use(server);

    // Teardown
    proc.kill('SIGTERM');
    rmSync(repoDir, { recursive: true, force: true });
    rmSync(sessionsDir, { recursive: true, force: true });
  },
});

export { expect } from '@playwright/test';
```

- [ ] **Step 2: Verify TypeScript compiles**

Run: `cd packages/web && npx tsc --noEmit -p tsconfig.node.json` (may need minor tsconfig adjustments — fixture uses Node APIs)

- [ ] **Step 3: Commit**

```bash
git add packages/web/e2e/fixtures.ts && git commit -m "test(e2e): add Playwright fixture with test repo and server lifecycle"
```

---

## Chunk 3: E2E Test Scenarios

### Task 4: Write the review workflow test

**Files:**
- Create: `packages/web/e2e/review-workflow.spec.ts`

- [ ] **Step 1: Create the test file**

Create `packages/web/e2e/review-workflow.spec.ts`:

```typescript
import { test, expect } from './fixtures';

test.describe.serial('Review workflow', () => {
  let sessionId: string;
  let threadId: string;

  test('shows empty state', async ({ page, lgtm }) => {
    await page.goto(lgtm.baseURL);
    await expect(page.getByText('No active review sessions')).toBeVisible();
  });

  test('creates session and shows tab', async ({ page, lgtm }) => {
    sessionId = await lgtm.createSession();
    await page.goto(lgtm.baseURL);
    // Tab should show repo name and branch
    await expect(page.locator('.tab')).toBeVisible();
    await expect(page.locator('.tab')).toContainText('feature');
  });

  test('shows diff when clicking file', async ({ page, lgtm }) => {
    await page.goto(lgtm.baseURL);
    // Wait for file tree to load
    await expect(page.locator('.file-item')).toHaveCount(2, { timeout: 5000 });
    // Click first file
    await page.locator('.file-item').first().click();
    // Should see diff lines
    await expect(page.locator('.diff-line')).not.toHaveCount(0);
  });

  test('adds a comment', async ({ page, lgtm }) => {
    await page.goto(lgtm.baseURL);
    await expect(page.locator('.file-item')).toHaveCount(2, { timeout: 5000 });
    await page.locator('.file-item').first().click();
    await expect(page.locator('.diff-line')).not.toHaveCount(0);

    // Click on an "add" diff line to open the comment form
    await page.locator('.diff-line.add').first().click();
    await expect(page.locator('.new-comment')).toBeVisible();

    // Type and submit
    await page.locator('.new-comment textarea').fill('Please fix this');
    await page.locator('.btn-submit').click();

    // Thread should appear
    await expect(page.locator('.thread')).toBeVisible({ timeout: 5000 });
    await expect(page.locator('.thread')).toContainText('Please fix this');

    // Capture the thread ID for later tests
    const sessionResp = await fetch(`${lgtm.baseURL}/api/sessions/${sessionId}`);
    const session = await sessionResp.json();
    threadId = session.threads[0].id;
  });

  test('submits to agent', async ({ page, lgtm }) => {
    await page.goto(lgtm.baseURL);
    // Click "Submit to agent"
    const submitBtn = page.locator('button', { hasText: 'Submit to agent' });
    await expect(submitBtn).toBeVisible({ timeout: 5000 });
    await submitBtn.click();
    // Button should change text
    await expect(page.getByText('Waiting for agent...')).toBeVisible({ timeout: 5000 });
  });

  test('shows agent reply', async ({ page, lgtm }) => {
    // Post reply via API
    await lgtm.agentReply(sessionId, threadId, 'Fixed the issue');
    await page.goto(lgtm.baseURL);
    await expect(page.locator('.file-item')).toHaveCount(2, { timeout: 5000 });
    await page.locator('.file-item').first().click();
    // Agent comment should appear in thread
    await expect(page.getByText('Fixed the issue')).toBeVisible({ timeout: 5000 });
  });

  test('resolves thread', async ({ page, lgtm }) => {
    await page.goto(lgtm.baseURL);
    await expect(page.locator('.file-item')).toHaveCount(2, { timeout: 5000 });
    await page.locator('.file-item').first().click();
    await expect(page.locator('.thread')).toBeVisible({ timeout: 5000 });

    // Click Resolve
    const resolveBtn = page.locator('button', { hasText: 'Resolve' });
    await resolveBtn.click();

    // Thread should show resolved status
    await expect(page.getByText('resolved')).toBeVisible({ timeout: 5000 });
  });

  test('approves session', async ({ page, lgtm }) => {
    await page.goto(lgtm.baseURL);

    // Mark all files as reviewed
    const checkboxes = page.locator('.file-item input[type="checkbox"]');
    const count = await checkboxes.count();
    for (let i = 0; i < count; i++) {
      await checkboxes.nth(i).click();
    }

    // Click Approve
    const approveBtn = page.locator('button', { hasText: 'Approve session' });
    await expect(approveBtn).toBeEnabled({ timeout: 5000 });
    await approveBtn.click();

    // Should show approved state
    await expect(page.getByText('Session approved')).toBeVisible({ timeout: 5000 });
  });
});
```

- [ ] **Step 2: Run tests (expect failures initially — this verifies the setup)**

```bash
cd packages/web && npm run build && cd ../..
cargo build -p lgtm-app
cd packages/web && npx playwright test
```

Examine failures. The fixture should start the server and the first test (empty state) should pass. Other tests may need selector adjustments based on the actual rendered HTML.

- [ ] **Step 3: Fix selectors based on actual DOM**

Run tests with headed mode to debug:
```bash
cd packages/web && npx playwright test --headed --debug
```

Adjust selectors in `review-workflow.spec.ts` to match the actual DOM structure. Common adjustments:
- `.diff-line.add` may need a different selector
- Click target for creating comments may differ
- Thread/resolve button selectors may differ

- [ ] **Step 4: Verify all 8 tests pass**

```bash
cd packages/web && npx playwright test
```
Expected: 8 passing

- [ ] **Step 5: Commit**

```bash
git add packages/web/e2e/ && git commit -m "test(e2e): add review workflow Playwright tests"
```

---

## Chunk 4: Makefile and Cleanup

### Task 5: Add e2e target to Makefile

**Files:**
- Modify: `Makefile`

- [ ] **Step 1: Add e2e target**

Add to `Makefile`:

```makefile
# Run e2e tests
e2e: build-web
	cargo build -p lgtm-app
	cd packages/web && npx playwright test
```

Update `.PHONY` line to include `e2e`.

- [ ] **Step 2: Verify**

```bash
make e2e
```
Expected: builds and runs all e2e tests, all pass.

- [ ] **Step 3: Commit**

```bash
git add Makefile && git commit -m "chore: add make e2e target for Playwright tests"
```
