<script lang="ts">
  import type { DiffLine as DiffLineType } from '../types';
  import type { ThemedToken } from 'shiki';
  import NewComment from './NewComment.svelte';

  let { line, filePath, tokens }: {
    line: DiffLineType;
    filePath: string;
    tokens?: ThemedToken[];
  } = $props();

  let showNewComment = $state(false);
</script>

<div class="diff-line" class:add={line.kind === 'add'} class:delete={line.kind === 'delete'}>
  <span class="gutter old-gutter" onclick={() => showNewComment = !showNewComment}>
    {line.old_lineno ?? ''}
  </span>
  <span class="gutter new-gutter" onclick={() => showNewComment = !showNewComment}>
    {line.new_lineno ?? ''}
  </span>
  <span class="line-marker">
    {line.kind === 'add' ? '+' : line.kind === 'delete' ? '-' : ' '}
  </span>
  <span class="line-content">
    {#if tokens && tokens.length > 0}
      {#each tokens as token}
        <span style="color: {token.color ?? 'inherit'}">{token.content}</span>
      {/each}
    {:else}
      {line.content}
    {/if}
  </span>
</div>
{#if showNewComment}
  <NewComment
    {filePath}
    lineStart={line.new_lineno ?? line.old_lineno ?? 0}
    lineEnd={line.new_lineno ?? line.old_lineno ?? 0}
    diffSide={line.new_lineno ? 'right' : 'left'}
    anchorContext={line.content}
    onsubmitted={() => showNewComment = false}
    oncancelled={() => showNewComment = false}
  />
{/if}

<style>
  .diff-line {
    display: flex;
    font-family: monospace;
    font-size: 13px;
    line-height: 20px;
  }

  .diff-line.add {
    background: #0d2818;
  }

  .diff-line.delete {
    background: #2d1117;
  }

  .gutter {
    width: 50px;
    min-width: 50px;
    padding: 0 8px;
    text-align: right;
    color: #484f58;
    user-select: none;
    cursor: pointer;
  }

  .gutter:hover {
    color: #58a6ff;
  }

  .line-marker {
    width: 16px;
    min-width: 16px;
    text-align: center;
    color: #484f58;
  }

  .diff-line.add .line-marker { color: #3fb950; }
  .diff-line.delete .line-marker { color: #f85149; }

  .line-content {
    flex: 1;
    padding-right: 16px;
    white-space: pre;
    overflow-x: auto;
  }
</style>
