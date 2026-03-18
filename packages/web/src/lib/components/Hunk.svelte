<script lang="ts">
  import type { Hunk as HunkType, Thread as ThreadType } from '../types';
  import type { ThemedToken } from 'shiki';
  import DiffLine from './DiffLine.svelte';
  import Thread from './Thread.svelte';
  import { getHighlighter, langFromPath } from '../utils/highlight';

  let { hunk, filePath, threadsByAnchorLine }: {
    hunk: HunkType;
    filePath: string;
    threadsByAnchorLine: Map<number, ThreadType[]>;
  } = $props();

  let tokensByIndex = $state<Map<number, ThemedToken[]>>(new Map());

  $effect(() => {
    const content = hunk.lines.map(l => l.content).join('\n');
    const lang = langFromPath(filePath);

    getHighlighter().then(hl => {
      try {
        const result = hl.codeToTokens(content, { lang, theme: 'github-dark' });
        const map = new Map<number, ThemedToken[]>();
        result.tokens.forEach((lineTokens, i) => map.set(i, lineTokens));
        tokensByIndex = map;
      } catch {
        tokensByIndex = new Map();
      }
    });
  });
</script>

<div class="hunk">
  <div class="hunk-header">
    @@ -{hunk.old_start},{hunk.old_count} +{hunk.new_start},{hunk.new_count} @@
  </div>
  {#each hunk.lines as line, idx}
    <DiffLine {line} {filePath} tokens={tokensByIndex.get(idx)} />
    {#if line.new_lineno && threadsByAnchorLine.has(line.new_lineno)}
      {#each threadsByAnchorLine.get(line.new_lineno)! as thread (thread.id)}
        <Thread {thread} />
      {/each}
    {/if}
  {/each}
</div>

<style>
  .hunk {
    border-bottom: 1px solid #30363d;
  }

  .hunk-header {
    padding: 4px 16px;
    background: #161b2266;
    color: #8b949e;
    font-family: monospace;
    font-size: 12px;
    border-bottom: 1px solid #30363d;
  }
</style>
