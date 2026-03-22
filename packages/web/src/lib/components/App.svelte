<script lang="ts">
  import { onMount, onDestroy, setContext } from 'svelte';
  import { session } from '../stores/session';
  import { diffFiles } from '../stores/diff';
  import { selectedFile, error } from '../stores/ui';
  import { submitPending } from '../stores/submit';
  import { getSession, getDiff, getSubmitStatus } from '../api';
  import { createWsClient } from '../ws';
  import FileTree from './FileTree.svelte';
  import DiffView from './DiffView.svelte';
  import StatusBar from './StatusBar.svelte';
  import Toast from './Toast.svelte';
  import type { DiffFile } from '../types';

  interface Props {
    sessionId: string;
  }

  let { sessionId }: Props = $props();

  setContext('sessionId', sessionId);

  let wsClient: ReturnType<typeof createWsClient> | null = null;

  async function loadState() {
    try {
      const [s, d] = await Promise.all([getSession(sessionId), getDiff(sessionId)]);
      session.set(s);
      diffFiles.set(d);
      if (d.length > 0 && !$selectedFile) {
        selectedFile.set(d[0].path);
      }
    } catch (e) {
      error.set(e instanceof Error ? e.message : 'Failed to load');
    }
  }

  function mergeDiffUpdate(updated: DiffFile[]) {
    diffFiles.update(existing => {
      const updatedPaths = new Set(updated.map(f => f.path));
      const kept = existing.filter(f => !updatedPaths.has(f.path));
      return [...kept, ...updated];
    });
  }

  onMount(async () => {
    await loadState();
    getSubmitStatus(sessionId).then(s => submitPending.set(s.pending)).catch(() => {});

    wsClient = createWsClient(
      sessionId,
      (msg) => {
        if (msg.type === 'session_updated') {
          session.set(msg.data);
        } else if (msg.type === 'diff_updated') {
          mergeDiffUpdate(msg.data);
        } else if (msg.type === 'submit_status') {
          submitPending.set(msg.data.pending);
        }
      },
      loadState,
    );
  });

  onDestroy(() => {
    wsClient?.stop();
  });
</script>

<div class="app">
  <header class="header">
    <span class="logo">lgtm</span>
    {#if $session}
      <span class="branch-info">{$session.head} &rarr; {$session.base}</span>
      <span class="thread-summary">
        {$session.threads.filter(t => t.status === 'open').length} open
        &middot;
        {$session.threads.filter(t => t.status !== 'open').length} done
      </span>
    {/if}
  </header>

  <div class="main">
    <aside class="sidebar">
      <FileTree />
    </aside>
    <main class="content">
      <DiffView />
    </main>
  </div>

  <StatusBar />
  <Toast />
</div>

<style>
  :global(body) {
    margin: 0;
    font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif;
    background: #0d1117;
    color: #c9d1d9;
  }

  .app {
    display: flex;
    flex-direction: column;
    height: 100%;
  }

  .header {
    display: flex;
    align-items: center;
    gap: 16px;
    padding: 8px 16px;
    background: #161b22;
    border-bottom: 1px solid #30363d;
    font-size: 14px;
  }

  .logo {
    font-weight: bold;
    font-size: 16px;
    color: #58a6ff;
  }

  .branch-info {
    color: #8b949e;
  }

  .thread-summary {
    color: #8b949e;
    margin-left: auto;
  }

  .main {
    display: flex;
    flex: 1;
    overflow: hidden;
  }

  .sidebar {
    width: 260px;
    border-right: 1px solid #30363d;
    overflow-y: auto;
  }

  .content {
    flex: 1;
    overflow-y: auto;
  }
</style>
