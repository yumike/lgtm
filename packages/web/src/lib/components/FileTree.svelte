<script lang="ts">
  import { session } from '../stores/session';
  import { diffFiles } from '../stores/diff';
  import { selectedFile } from '../stores/ui';
  import { patchFile } from '../api';

  function getThreadCount(file: string): number {
    const s = $session;
    if (!s) return 0;
    return s.threads.filter(t => t.file === file && t.status === 'open').length;
  }

  function isReviewed(file: string): boolean {
    return $session?.files[file] === 'reviewed';
  }

  async function toggleReviewed(e: MouseEvent, file: string) {
    e.stopPropagation();
    const newStatus = isReviewed(file) ? 'pending' as const : 'reviewed' as const;
    session.update(s => {
      if (!s) return s;
      return { ...s, files: { ...s.files, [file]: newStatus } };
    });
    try {
      await patchFile(file, newStatus);
    } catch {
      // Will be corrected by next WebSocket update
    }
  }
</script>

<div class="file-tree">
  <div class="file-tree-header">Files</div>
  {#each $diffFiles as file}
    <button
      class="file-item"
      class:selected={$selectedFile === file.path}
      onclick={() => selectedFile.set(file.path)}
    >
      <input
        type="checkbox"
        checked={isReviewed(file.path)}
        onclick={(e: MouseEvent) => toggleReviewed(e, file.path)}
      />
      <span class="file-status" class:added={file.status === 'added'} class:deleted={file.status === 'deleted'} class:modified={file.status === 'modified'}>
        {file.status === 'added' ? 'A' : file.status === 'deleted' ? 'D' : file.status === 'renamed' ? 'R' : 'M'}
      </span>
      <span class="file-name">{file.path.split('/').pop()}</span>
      <span class="file-path">{file.path}</span>
      {#if getThreadCount(file.path) > 0}
        <span class="thread-badge">{getThreadCount(file.path)}</span>
      {/if}
    </button>
  {/each}
</div>

<style>
  .file-tree {
    padding: 8px 0;
  }

  .file-tree-header {
    padding: 8px 16px;
    font-size: 12px;
    font-weight: 600;
    text-transform: uppercase;
    color: #8b949e;
  }

  .file-item {
    display: flex;
    align-items: center;
    gap: 8px;
    width: 100%;
    padding: 4px 16px;
    border: none;
    background: transparent;
    color: #c9d1d9;
    cursor: pointer;
    font-size: 13px;
    text-align: left;
  }

  .file-item:hover {
    background: #161b22;
  }

  .file-item.selected {
    background: #1f6feb22;
  }

  .file-status {
    font-family: monospace;
    font-size: 11px;
    font-weight: bold;
    width: 14px;
    text-align: center;
  }

  .file-status.added { color: #3fb950; }
  .file-status.deleted { color: #f85149; }
  .file-status.modified { color: #d29922; }

  .file-name {
    flex: 1;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .file-path {
    display: none;
  }

  .thread-badge {
    background: #f85149;
    color: white;
    border-radius: 10px;
    padding: 0 6px;
    font-size: 11px;
    font-weight: bold;
  }

  input[type="checkbox"] {
    cursor: pointer;
  }
</style>
