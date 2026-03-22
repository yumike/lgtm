# Playwright E2E Tests

## Problem

No automated tests verify the full user workflow through the lgtm UI. Manual testing is slow and error-prone.

## Solution

Playwright tests that run against the Axum HTTP server directly — no Tauri/WebDriver needed. The Svelte frontend is just a web app served by Axum.

## Architecture

Tests live in `packages/web/e2e/`. A Playwright fixture starts the Axum server on a random port with a temp session store, creates a git repo with branches/changes to produce a diff, and provides the base URL to tests.

### Headless server mode

Add a `--headless` flag to `lgtm-app` that skips Tauri and runs only the Axum server. This avoids needing a separate binary and is useful for development too.

When `--headless` is passed:
- Skip `tauri::Builder` entirely
- Run the Axum server on the main thread (blocking)
- Still write the lockfile and serve static assets
- Print the port to stdout for the test fixture to capture

### Test fixture (`lgtmServer`)

Setup:
1. Create a temp directory
2. Init a git repo with `main` branch, add a file, commit
3. Create `feature` branch, modify the file, commit
4. Set session store dir to a temp directory
5. Start `lgtm-app --headless` with env vars pointing to the temp dirs
6. Wait for server ready (poll `/api/sessions`)
7. Create a session via `POST /api/sessions`

Teardown:
- Kill the server process
- Remove temp directories

### Test scenarios

Single test file, sequential steps (each depends on previous state):

1. **Empty state** — navigate to `/`, see "No active review sessions"
2. **Create session** — POST session via API, reload, see tab with branch name
3. **View diff** — click file in sidebar, see diff lines rendered
4. **Add comment** — click a diff line, type comment, submit, see thread appear
5. **Submit to agent** — click "Submit to agent", button shows "Waiting for agent..."
6. **Agent reply** — POST reply via API, verify comment appears in thread
7. **Resolve thread** — click Resolve on thread, verify status changes
8. **Approve session** — mark file reviewed, click Approve, see "Session approved"

### Dependencies

- `@playwright/test` added to `packages/web/package.json` devDependencies
- `playwright.config.ts` in `packages/web/`

### Makefile

Add `make e2e` target that builds the server, builds the frontend, and runs Playwright.
