import { createHighlighterCore } from 'shiki/core';
import { createJavaScriptRegExpEngine } from 'shiki/engine/javascript';
import type { HighlighterCore } from 'shiki/core';

let highlighter: HighlighterCore | null = null;

const defaultLangs = [
  import('shiki/langs/javascript'),
  import('shiki/langs/typescript'),
  import('shiki/langs/python'),
  import('shiki/langs/rust'),
  import('shiki/langs/go'),
  import('shiki/langs/css'),
  import('shiki/langs/html'),
  import('shiki/langs/json'),
  import('shiki/langs/yaml'),
  import('shiki/langs/bash'),
  import('shiki/langs/markdown'),
];

export async function getHighlighter(): Promise<HighlighterCore> {
  if (highlighter) return highlighter;

  highlighter = await createHighlighterCore({
    themes: [import('shiki/themes/github-dark')],
    langs: defaultLangs,
    engine: createJavaScriptRegExpEngine(),
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
