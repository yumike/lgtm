export type SessionStatus = 'in_progress' | 'approved' | 'abandoned';
export type ThreadStatus = 'open' | 'resolved' | 'wontfix' | 'dismissed';
export type DiffSide = 'left' | 'right';
export type Author = 'developer' | 'agent';
export type FileReviewStatus = 'pending' | 'reviewed';
export type Origin = 'developer' | 'agent';
export type Severity = 'critical' | 'warning' | 'info';
export type FileChangeKind = 'added' | 'modified' | 'deleted' | 'renamed';
export type LineKind = 'context' | 'add' | 'delete';

export interface Session {
  id: string;
  repo_path: string;
  version: number;
  status: SessionStatus;
  base: string;
  head: string;
  merge_base: string;
  created_at: string;
  updated_at: string;
  threads: Thread[];
  files: Record<string, FileReviewStatus>;
}

export interface Thread {
  id: string;
  origin: Origin;
  severity?: Severity | null;
  status: ThreadStatus;
  file: string;
  line_start: number;
  line_end: number;
  diff_side: DiffSide;
  anchor_context: string;
  comments: Comment[];
}

export interface Comment {
  id: string;
  author: Author;
  body: string;
  timestamp: string;
}

export interface DiffFile {
  path: string;
  status: FileChangeKind;
  old_path: string | null;
  hunks: Hunk[];
}

export interface Hunk {
  old_start: number;
  old_count: number;
  new_start: number;
  new_count: number;
  lines: DiffLine[];
}

export interface DiffLine {
  kind: LineKind;
  content: string;
  old_lineno: number | null;
  new_lineno: number | null;
}

export type WsMessage =
  | { type: 'session_updated'; data: Session }
  | { type: 'diff_updated'; data: DiffFile[] }
  | { type: 'submit_status'; data: { pending: boolean } };
