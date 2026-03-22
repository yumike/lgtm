import { test as base } from '@playwright/test';
import { spawn, execSync, type ChildProcess } from 'child_process';
import { mkdtempSync, writeFileSync, rmSync } from 'fs';
import { join, dirname } from 'path';
import { tmpdir } from 'os';
import { fileURLToPath } from 'url';

const __dirname = dirname(fileURLToPath(import.meta.url));

export interface LgtmServer {
  baseURL: string;
  port: number;
  createSession(): Promise<string>;
  agentReply(sessionId: string, threadId: string, body: string): Promise<void>;
}

function createTestRepo(): string {
  const dir = mkdtempSync(join(tmpdir(), 'lgtm-e2e-'));

  execSync('git init', { cwd: dir });
  execSync('git checkout -b main', { cwd: dir });
  execSync('git config user.email "test@test.com"', { cwd: dir });
  execSync('git config user.name "Test"', { cwd: dir });

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

export const test = base.extend<{}, { lgtm: LgtmServer }>({
  lgtm: [async ({}, use) => {
    const repoDir = createTestRepo();
    const sessionsDir = mkdtempSync(join(tmpdir(), 'lgtm-sessions-'));

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

    // Read port from first line of stdout
    const port = await new Promise<number>((resolve, reject) => {
      let stdout = '';
      proc.stdout!.on('data', (chunk: Buffer) => {
        stdout += chunk.toString();
        const match = stdout.match(/^(\d+)/);
        if (match) resolve(parseInt(match[1], 10));
      });
      proc.on('error', reject);
      setTimeout(() => reject(new Error('Timeout reading port from server')), 10_000);
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
        if (!resp.ok) {
          const text = await resp.text();
          throw new Error(`Create session failed: ${resp.status} ${text}`);
        }
        const session: any = await resp.json();
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
        if (!resp.ok) {
          const text = await resp.text();
          throw new Error(`Agent reply failed: ${resp.status} ${text}`);
        }
      },
    };

    await use(server);

    proc.kill('SIGTERM');
    rmSync(repoDir, { recursive: true, force: true });
    rmSync(sessionsDir, { recursive: true, force: true });
  }, { scope: 'worker' }],
});

export { expect } from '@playwright/test';
