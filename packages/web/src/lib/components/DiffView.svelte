<script lang="ts">
  import { diffFiles } from '../stores/diff';
  import { selectedFile } from '../stores/ui';
  import { session } from '../stores/session';
  import Hunk from './Hunk.svelte';
  import type { DiffFile, Thread as ThreadType } from '../types';

  let currentFile = $derived($diffFiles.find(f => f.path === $selectedFile));

  let threadsByAnchorLine = $derived.by(() => {
    const s = $session;
    if (!s || !$selectedFile) return new Map<number, ThreadType[]>();
    const map = new Map<number, ThreadType[]>();
    for (const t of s.threads) {
      if (t.file !== $selectedFile) continue;
      const anchor = t.line_end;
      if (!map.has(anchor)) map.set(anchor, []);
      map.get(anchor)!.push(t);
    }
    return map;
  });
</script>

<div class="diff-view">
  {#if currentFile}
    <div class="diff-header">
      <span class="diff-file-path">{currentFile.path}</span>
      <span class="diff-file-status">{currentFile.status}</span>
    </div>
    {#each currentFile.hunks as hunk, i}
      <Hunk {hunk} filePath={currentFile.path} {threadsByAnchorLine} />
    {/each}
  {:else if $selectedFile}
    <div class="empty-state">No diff data for {$selectedFile}</div>
  {:else}
    <div class="empty-state">Select a file to view its diff</div>
  {/if}
</div>

<style>
  .diff-view {
    padding: 0;
  }

  .diff-header {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 8px 16px;
    background: #161b22;
    border-bottom: 1px solid #30363d;
    font-size: 13px;
    position: sticky;
    top: 0;
    z-index: 10;
  }

  .diff-file-path {
    font-family: monospace;
    color: #c9d1d9;
  }

  .diff-file-status {
    color: #8b949e;
    font-size: 12px;
  }

  .empty-state {
    display: flex;
    align-items: center;
    justify-content: center;
    height: 200px;
    color: #8b949e;
  }
</style>
