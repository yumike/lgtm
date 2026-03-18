import { createHighlighter } from 'shiki';
import type { Highlighter } from 'shiki';

let highlighter: Highlighter | null = null;

const defaultLangs = [
  'javascript',
  'typescript',
  'python',
  'rust',
  'go',
  'css',
  'html',
  'json',
  'yaml',
  'bash',
  'markdown',
] as const;

export async function getHighlighter(): Promise<Highlighter> {
  if (highlighter) return highlighter;

  highlighter = await createHighlighter({
    themes: ['github-dark'],
    langs: [...defaultLangs],
  });

  return highlighter;
}

const extToLang: Record<string, string> = {
  js: 'javascript',
  jsx: 'javascript',
  ts: 'typescript',
  tsx: 'typescript',
  py: 'python',
  rs: 'rust',
  go: 'go',
  css: 'css',
  html: 'html',
  json: 'json',
  yml: 'yaml',
  yaml: 'yaml',
  sh: 'bash',
  bash: 'bash',
  md: 'markdown',
  svelte: 'html',
};

export function langFromPath(path: string): string {
  const ext = path.split('.').pop() ?? '';
  return extToLang[ext] ?? 'text';
}
