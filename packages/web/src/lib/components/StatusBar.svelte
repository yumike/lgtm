<script lang="ts">
  import { session } from '../stores/session';
  import { diffFiles } from '../stores/diff';
  import { patchSession } from '../api';

  let openCount = $derived($session?.threads.filter(t => t.status === 'open').length ?? 0);
  let resolvedCount = $derived($session?.threads.filter(t => t.status === 'resolved').length ?? 0);
  let wontfixCount = $derived($session?.threads.filter(t => t.status === 'wontfix').length ?? 0);
  let totalFiles = $derived($diffFiles.length);
  let reviewedFiles = $derived(Object.values($session?.files ?? {}).filter(s => s === 'reviewed').length);

  let canApprove = $derived(openCount === 0 && reviewedFiles >= totalFiles && totalFiles > 0);

  async function approve() {
    if (!canApprove) return;
    try {
      await patchSession('approved');
    } catch {
      // toast
    }
  }
</script>

<footer class="status-bar">
  <div class="status-left">
    <span>{openCount} open</span>
    <span>&middot;</span>
    <span>{resolvedCount} resolved</span>
    {#if wontfixCount > 0}
      <span>&middot;</span>
      <span>{wontfixCount} won't fix</span>
    {/if}
    <span>&middot;</span>
    <span>{reviewedFiles}/{totalFiles} files reviewed</span>
  </div>
  <div class="status-right">
    <button class="btn-approve" disabled={!canApprove} onclick={approve}>
      Approve session
    </button>
  </div>
</footer>

<style>
  .status-bar {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 8px 16px;
    background: #161b22;
    border-top: 1px solid #30363d;
    font-size: 13px;
    color: #8b949e;
  }

  .status-left {
    display: flex;
    gap: 8px;
  }

  .btn-approve {
    padding: 4px 16px;
    border: none;
    border-radius: 6px;
    background: #238636;
    color: white;
    cursor: pointer;
    font-size: 13px;
  }

  .btn-approve:disabled {
    opacity: 0.3;
    cursor: not-allowed;
  }
</style>
