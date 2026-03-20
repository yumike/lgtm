<script lang="ts">
  import { session } from '../stores/session';
  import { diffFiles } from '../stores/diff';
  import { submitPending } from '../stores/submit';
  import { patchSession, submitToAgent } from '../api';

  let threads = $derived($session?.threads ?? []);
  let openCount = $derived(threads.filter(t => t.status === 'open').length);
  let resolvedCount = $derived(threads.filter(t => t.status === 'resolved').length);
  let wontfixCount = $derived(threads.filter(t => t.status === 'wontfix').length);
  let dismissedCount = $derived(threads.filter(t => t.status === 'dismissed').length);
  let totalFiles = $derived($diffFiles.length);
  let reviewedFiles = $derived(Object.values($session?.files ?? {}).filter(s => s === 'reviewed').length);

  // Approve requires:
  // - developer threads: all resolved or wontfix
  // - agent threads: all resolved or dismissed
  let devThreadsCleared = $derived(
    threads.filter(t => t.origin !== 'agent').every(t => t.status === 'resolved' || t.status === 'wontfix')
  );
  let agentThreadsCleared = $derived(
    threads.filter(t => t.origin === 'agent').every(t => t.status === 'resolved' || t.status === 'dismissed')
  );

  let isApproved = $derived($session?.status === 'approved');
  let isAbandoned = $derived($session?.status === 'abandoned');
  let canApprove = $derived(
    !isApproved && !isAbandoned && openCount === 0 &&
    devThreadsCleared && agentThreadsCleared &&
    reviewedFiles >= totalFiles && totalFiles > 0
  );

  // Submit state — reads from the shared store (updated via WebSocket in App.svelte)
  let canSubmit = $derived(
    !isApproved && !isAbandoned && !$submitPending && openCount > 0
  );

  async function submit() {
    if (!canSubmit) return;
    try {
      await submitToAgent();
      submitPending.set(true);
    } catch {
      // toast
    }
  }

  async function approve() {
    if (!canApprove) return;
    try {
      await patchSession('approved');
    } catch {
      // toast
    }
  }
</script>

<footer class="status-bar" class:approved={isApproved} class:abandoned={isAbandoned}>
  <div class="status-left">
    {#if isApproved}
      <span class="status-badge approved-badge">Approved</span>
    {:else if isAbandoned}
      <span class="status-badge abandoned-badge">Abandoned</span>
    {/if}
    <span>{openCount} open</span>
    <span>&middot;</span>
    <span>{resolvedCount} resolved</span>
    {#if wontfixCount > 0}
      <span>&middot;</span>
      <span>{wontfixCount} won't fix</span>
    {/if}
    {#if dismissedCount > 0}
      <span>&middot;</span>
      <span>{dismissedCount} dismissed</span>
    {/if}
    <span>&middot;</span>
    <span>{reviewedFiles}/{totalFiles} files reviewed</span>
  </div>
  <div class="status-right">
    {#if isApproved}
      <span class="approved-text">Session approved</span>
    {:else}
      <button class="btn-submit" disabled={!canSubmit} onclick={submit}>
        {#if $submitPending}
          Waiting for agent...
        {:else}
          Submit to agent
        {/if}
      </button>
      <button class="btn-approve" disabled={!canApprove} onclick={approve}>
        Approve session
      </button>
    {/if}
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

  .status-right {
    display: flex;
    gap: 8px;
  }

  .btn-submit {
    padding: 4px 16px;
    border: 1px solid #30363d;
    border-radius: 6px;
    background: #21262d;
    color: #c9d1d9;
    cursor: pointer;
    font-size: 13px;
  }

  .btn-submit:disabled {
    opacity: 0.3;
    cursor: not-allowed;
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

  .status-bar.approved {
    border-top: 1px solid #238636;
    background: #0d1117;
  }

  .status-bar.abandoned {
    border-top: 1px solid #da3633;
  }

  .status-badge {
    font-weight: 600;
    padding: 2px 8px;
    border-radius: 12px;
    font-size: 12px;
  }

  .approved-badge {
    background: #238636;
    color: white;
  }

  .abandoned-badge {
    background: #da3633;
    color: white;
  }

  .approved-text {
    color: #3fb950;
    font-weight: 600;
  }
</style>
