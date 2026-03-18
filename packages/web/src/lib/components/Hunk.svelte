<script lang="ts">
  import type { Hunk as HunkType, Thread as ThreadType } from '../types';
  import DiffLine from './DiffLine.svelte';
  import Thread from './Thread.svelte';

  let { hunk, filePath, threadsByAnchorLine }: {
    hunk: HunkType;
    filePath: string;
    threadsByAnchorLine: Map<number, ThreadType[]>;
  } = $props();
</script>

<div class="hunk">
  <div class="hunk-header">
    @@ -{hunk.old_start},{hunk.old_count} +{hunk.new_start},{hunk.new_count} @@
  </div>
  {#each hunk.lines as line}
    <DiffLine {line} {filePath} />
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
