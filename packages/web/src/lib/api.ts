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

const BASE = '/api';

async function request<T>(path: string, init?: RequestInit): Promise<T> {
  const resp = await fetch(`${BASE}${path}`, {
    headers: { 'Content-Type': 'application/json' },
    ...init,
  });
  if (!resp.ok) {
    const body = await resp.json().catch(() => ({ error: resp.statusText }));
    throw new Error(body.error || resp.statusText);
  }
  return resp.json();
}

export function getSession(): Promise<Session> {
  return request('/session');
}

export function patchSession(status: SessionStatus): Promise<Session> {
  return request('/session', {
    method: 'PATCH',
    body: JSON.stringify({ status }),
  });
}

export function getDiff(file?: string): Promise<DiffFile[]> {
  const params = file ? `?file=${encodeURIComponent(file)}` : '';
  return request(`/diff${params}`);
}

export function createThread(params: {
  file: string;
  line_start: number;
  line_end: number;
  diff_side: DiffSide;
  anchor_context: string;
  body: string;
}): Promise<Thread> {
  return request('/threads', {
    method: 'POST',
    body: JSON.stringify(params),
  });
}

export function addComment(threadId: string, body: string): Promise<Comment> {
  return request(`/threads/${threadId}/comments`, {
    method: 'POST',
    body: JSON.stringify({ body }),
  });
}

export function patchThread(threadId: string, status: ThreadStatus): Promise<Thread> {
  return request(`/threads/${threadId}`, {
    method: 'PATCH',
    body: JSON.stringify({ status }),
  });
}

export function patchFile(path: string, status: FileReviewStatus): Promise<void> {
  return request(`/files?path=${encodeURIComponent(path)}`, {
    method: 'PATCH',
    body: JSON.stringify({ status }),
  });
}
