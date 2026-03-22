import type {
  Session,
  Thread,
  Comment,
  DiffFile,
  DiffSide,
  ThreadStatus,
  FileReviewStatus,
  SessionStatus,
} from './types';

async function request<T>(path: string, init?: RequestInit): Promise<T> {
  const resp = await fetch(path, {
    headers: { 'Content-Type': 'application/json' },
    ...init,
  });
  if (!resp.ok) {
    const body = await resp.json().catch(() => ({ error: resp.statusText }));
    throw new Error(body.error || resp.statusText);
  }
  return resp.json();
}

function apiBase(sessionId: string) {
  return `/api/sessions/${sessionId}`;
}

export async function listSessions(): Promise<Session[]> {
  return request('/api/sessions');
}

export async function createSession(repoPath: string, base: string): Promise<Session> {
  return request('/api/sessions', {
    method: 'POST',
    body: JSON.stringify({ repo_path: repoPath, base }),
  });
}

export async function deleteSession(sessionId: string): Promise<void> {
  return request(`/api/sessions/${sessionId}`, { method: 'DELETE' });
}

export function getSession(sessionId: string): Promise<Session> {
  return request(apiBase(sessionId));
}

export function patchSession(sessionId: string, status: SessionStatus): Promise<Session> {
  return request(apiBase(sessionId), {
    method: 'PATCH',
    body: JSON.stringify({ status }),
  });
}

export function getDiff(sessionId: string, file?: string): Promise<DiffFile[]> {
  const params = file ? `?file=${encodeURIComponent(file)}` : '';
  return request(`${apiBase(sessionId)}/diff${params}`);
}

export function createThread(sessionId: string, params: {
  file: string;
  line_start: number;
  line_end: number;
  diff_side: DiffSide;
  anchor_context: string;
  body: string;
}): Promise<Thread> {
  return request(`${apiBase(sessionId)}/threads`, {
    method: 'POST',
    body: JSON.stringify(params),
  });
}

export function addComment(sessionId: string, threadId: string, body: string): Promise<Comment> {
  return request(`${apiBase(sessionId)}/threads/${threadId}/comments`, {
    method: 'POST',
    body: JSON.stringify({ body }),
  });
}

export function patchThread(sessionId: string, threadId: string, status: ThreadStatus): Promise<Thread> {
  return request(`${apiBase(sessionId)}/threads/${threadId}`, {
    method: 'PATCH',
    body: JSON.stringify({ status }),
  });
}

export function patchFile(sessionId: string, path: string, status: FileReviewStatus): Promise<void> {
  return request(`${apiBase(sessionId)}/files?path=${encodeURIComponent(path)}`, {
    method: 'PATCH',
    body: JSON.stringify({ status }),
  });
}

export function submitToAgent(sessionId: string): Promise<{ pending: boolean }> {
  return request(`${apiBase(sessionId)}/submit`, { method: 'POST' });
}

export function getSubmitStatus(sessionId: string): Promise<{ pending: boolean }> {
  return request(`${apiBase(sessionId)}/submit`);
}
