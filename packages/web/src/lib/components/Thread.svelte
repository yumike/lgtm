<script lang="ts">
  import { getContext } from 'svelte';
  import type { Thread as ThreadType, ThreadStatus } from '../types';
  import { patchThread, addComment } from '../api';
  import { session } from '../stores/session';

  const sessionId: string = getContext('sessionId');
  import Comment from './Comment.svelte';

  let { thread }: { thread: ThreadType } = $props();

  let isAgent = $derived(thread.origin === 'agent');
  let collapsed = $state(thread.status !== 'open');
  let replyBody = $state('');
  let showReply = $state(false);

  let severityIcon = $derived(
    thread.severity === 'critical' ? '🔴' :
    thread.severity === 'warning' ? '🟡' :
    thread.severity === 'info' ? '🔵' : ''
  );

  async function setStatus(status: ThreadStatus) {
    session.update(s => {
      if (!s) return s;
      return {
        ...s,
        threads: s.threads.map(t =>
          t.id === thread.id ? { ...t, status } : t
        ),
      };
    });
    try {
      await patchThread(sessionId, thread.id, status);
    } catch {
      // Will be corrected by WebSocket
    }
  }

  async function submitReply() {
    if (!replyBody.trim()) return;
    try {
      const comment = await addComment(sessionId, thread.id, replyBody.trim());
      session.update(s => {
        if (!s) return s;
        return {
          ...s,
          threads: s.threads.map(t =>
            t.id === thread.id
              ? { ...t, comments: [...t.comments, comment] }
              : t
          ),
        };
      });
      replyBody = '';
      showReply = false;
    } catch {
      // Error via toast
    }
  }

  function handleReplyKeydown(e: KeyboardEvent) {
    if (e.key === 'Enter' && (e.metaKey || e.ctrlKey)) {
      submitReply();
    } else if (e.key === 'Escape') {
      showReply = false;
    }
  }
</script>

<div class="thread" class:resolved={thread.status !== 'open'} class:agent={isAgent}
     class:severity-critical={thread.severity === 'critical'}
     class:severity-warning={thread.severity === 'warning'}
     class:severity-info={thread.severity === 'info'}>
  <button class="thread-header" onclick={() => collapsed = !collapsed}>
    <span class="thread-status" class:open={thread.status === 'open'}>
      {thread.status === 'open' ? '○' : thread.status === 'dismissed' ? '—' : '✓'}
    </span>
    {#if severityIcon}
      <span class="severity-badge">{severityIcon}</span>
    {/if}
    {#if isAgent}
      <span class="origin-badge">agent</span>
    {/if}
    <span class="thread-preview">
      {thread.comments[0]?.body.slice(0, 80)}{(thread.comments[0]?.body.length ?? 0) > 80 ? '...' : ''}
    </span>
    <span class="thread-count">{thread.comments.length}</span>
  </button>

  {#if !collapsed}
    <div class="thread-body">
      {#each thread.comments as comment (comment.id)}
        <Comment {comment} />
      {/each}

      <div class="thread-actions">
        {#if thread.status === 'open'}
          <button class="btn-action" onclick={() => showReply = true}>Reply</button>
          <button class="btn-action btn-resolve" onclick={() => setStatus('resolved')}>Resolve</button>
          <button class="btn-action" onclick={() => setStatus('wontfix')}>Won't fix</button>
          {#if isAgent}
            <button class="btn-action btn-dismiss" onclick={() => setStatus('dismissed')}>Dismiss</button>
          {/if}
        {:else}
          <button class="btn-action" onclick={() => setStatus('open')}>Reopen</button>
        {/if}
      </div>

      {#if showReply}
        <div class="reply-box">
          <textarea
            bind:value={replyBody}
            placeholder="Reply... (Cmd+Enter to submit)"
            onkeydown={handleReplyKeydown}
            autofocus
          ></textarea>
          <div class="reply-actions">
            <button class="btn-cancel" onclick={() => showReply = false}>Cancel</button>
            <button class="btn-submit" onclick={submitReply} disabled={!replyBody.trim()}>Reply</button>
          </div>
        </div>
      {/if}
    </div>
  {/if}
</div>

<style>
  .thread {
    margin: 4px 16px 4px 116px;
    border: 1px solid #30363d;
    border-radius: 6px;
    background: #161b22;
  }

  .thread.resolved {
    opacity: 0.6;
  }

  .thread.agent {
    border-left: 3px solid #8b949e;
  }

  .thread.agent.severity-critical {
    border-left-color: #f85149;
  }

  .thread.agent.severity-warning {
    border-left-color: #d29922;
  }

  .thread.agent.severity-info {
    border-left-color: #58a6ff;
  }

  .thread-header {
    display: flex;
    align-items: center;
    gap: 8px;
    width: 100%;
    padding: 8px 12px;
    border: none;
    background: transparent;
    color: #c9d1d9;
    cursor: pointer;
    font-size: 13px;
    text-align: left;
  }

  .thread-status {
    color: #f85149;
  }

  .thread-status.open {
    color: #3fb950;
  }

  .severity-badge {
    font-size: 10px;
    line-height: 1;
  }

  .origin-badge {
    font-size: 10px;
    padding: 1px 5px;
    border-radius: 8px;
    background: #30363d;
    color: #8b949e;
    text-transform: uppercase;
    letter-spacing: 0.5px;
    font-weight: 600;
  }

  .thread-preview {
    flex: 1;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .thread-count {
    color: #8b949e;
    font-size: 12px;
  }

  .thread-body {
    border-top: 1px solid #21262d;
  }

  .thread-actions {
    display: flex;
    gap: 8px;
    padding: 8px 12px;
    border-top: 1px solid #21262d;
  }

  .btn-action {
    padding: 4px 12px;
    border: 1px solid #30363d;
    border-radius: 6px;
    background: transparent;
    color: #c9d1d9;
    cursor: pointer;
    font-size: 12px;
  }

  .btn-resolve {
    border-color: #238636;
    color: #3fb950;
  }

  .btn-dismiss {
    border-color: #8b949e;
    color: #8b949e;
  }

  .reply-box {
    border-top: 1px solid #21262d;
  }

  .reply-box textarea {
    width: 100%;
    min-height: 60px;
    padding: 8px 12px;
    border: none;
    background: transparent;
    color: #c9d1d9;
    font-family: inherit;
    font-size: 13px;
    resize: vertical;
    box-sizing: border-box;
  }

  .reply-box textarea:focus { outline: none; }

  .reply-actions {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
    padding: 8px 12px;
  }

  .btn-cancel {
    padding: 4px 12px;
    border: 1px solid #30363d;
    border-radius: 6px;
    background: transparent;
    color: #c9d1d9;
    cursor: pointer;
  }

  .btn-submit {
    padding: 4px 12px;
    border: none;
    border-radius: 6px;
    background: #238636;
    color: white;
    cursor: pointer;
  }

  .btn-submit:disabled { opacity: 0.5; cursor: not-allowed; }
</style>
