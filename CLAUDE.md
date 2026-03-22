# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What is lgtm

A local code review tool providing a GitHub-like review experience for branch changes. Enables a bidirectional conversation loop between a developer and an AI coding agent (Claude Code) for local code review-fix-verify cycles before code reaches a pull request.

## Build & Development Commands

```bash
make dev          # Tauri dev mode with hot reload (frontend + backend)
make build        # Full production build (web + app + cli)
make check        # cargo check --workspace + TypeScript check
make test         # cargo test --workspace
make e2e          # Build web + app, then run Playwright tests
```

Single crate test: `cargo test -p lgtm-session`
Single e2e test: `cd packages/web && npx playwright test -g "test name"`

Frontend dev server (standalone): `cd packages/web && npm run dev`

## Architecture

**Rust workspace** (edition 2024, rust 1.85 stable) with 5 crates:

- **lgtm-cli** (`crates/lgtm-cli/`) — Binary `lgtm`. Thin stateless HTTP client that auto-discovers the server via lockfile at `~/.lgtm/server.json`, launching the app if needed. Uses clap/reqwest/tungstenite.
- **lgtm-app** (`crates/lgtm-app/`) — Binary `lgtm-app`. Tauri 2.0 native app embedding the Axum server. Supports `--headless` for server-only mode (used by e2e tests). Binds to OS-assigned port, writes lockfile.
- **lgtm-server** (`crates/lgtm-server/`) — Core Axum HTTP server. Routes scoped per session (`/api/sessions/{id}/*`). Handles session CRUD, diffs, threads/comments, file review status, WebSocket updates, file watching.
- **lgtm-session** (`crates/lgtm-session/`) — Session data model and SessionStore (in-memory HashMap with RwLock, persisted to `~/.lgtm/sessions/{id}.json`). Atomic writes via tmp+rename.
- **lgtm-git** (`crates/lgtm-git/`) — `DiffProvider` trait with `CliDiffProvider` impl that shells out to `git` CLI (no git library).

**Frontend** (`packages/web/`) — Svelte 5 + Vite 8 + TypeScript. Key modules: `src/lib/api.ts` (HTTP client), `src/lib/ws.ts` (WebSocket), `src/lib/stores/` (Svelte stores), `src/lib/components/` (UI components). Syntax highlighting via Shiki.

**Single app, multiple sessions**: One persistent Tauri app on one port manages all review sessions. CLI discovers it via lockfile.

## Key Design Decisions

- **ULID IDs** everywhere — sortable, no coordination needed, both UI and agent generate independently
- **Atomic session writes** — write to `.tmp`, rename for concurrency safety
- **File blob hashing** — `reviewed_hash` on file review status auto-resets if file is modified after review
- **WebSocket** for real-time updates from file watcher + session changes
- **No external git library** — CliDiffProvider shells out to `git` for portability

## E2E Tests

Playwright tests in `packages/web/e2e/`. The fixture (`fixtures.ts`) starts lgtm-app in headless mode, creates a temporary git repo with test branches, and captures the server port from stdout.

## Spec

`lgtm-spec.md` at repo root is the comprehensive protocol specification.
