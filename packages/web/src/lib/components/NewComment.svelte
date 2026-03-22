<script lang="ts">
  import { getContext } from 'svelte';
  import { createThread } from '../api';
  import { session } from '../stores/session';

  const sessionId: string = getContext('sessionId');
  import type { DiffSide } from '../types';

  let { filePath, lineStart, lineEnd, diffSide, anchorContext, onsubmitted, oncancelled }: {
    filePath: string;
    lineStart: number;
    lineEnd: number;
    diffSide: DiffSide;
    anchorContext: string;
    onsubmitted: () => void;
    oncancelled: () => void;
  } = $props();

  let body = $state('');
  let submitting = $state(false);

  async function submit() {
    if (!body.trim()) return;
    submitting = true;
    try {
      const thread = await createThread(sessionId, {
        file: filePath,
        line_start: lineStart,
        line_end: lineEnd,
        diff_side: diffSide,
        anchor_context: anchorContext,
        body: body.trim(),
      });
      session.update(s => {
        if (!s) return s;
        return { ...s, threads: [...s.threads, thread] };
      });
      onsubmitted();
    } catch {
      // Error will show via toast
    } finally {
      submitting = false;
    }
  }

  function handleKeydown(e: KeyboardEvent) {
    if (e.key === 'Enter' && (e.metaKey || e.ctrlKey)) {
      submit();
    } else if (e.key === 'Escape') {
      oncancelled();
    }
  }
</script>

<div class="new-comment">
  <textarea
    bind:value={body}
    placeholder="Leave a comment... (Cmd+Enter to submit)"
    onkeydown={handleKeydown}
    disabled={submitting}
    autofocus
  ></textarea>
  <div class="new-comment-actions">
    <button class="btn-cancel" onclick={oncancelled}>Cancel</button>
    <button class="btn-submit" onclick={submit} disabled={submitting || !body.trim()}>
      Comment
    </button>
  </div>
</div>

<style>
  .new-comment {
    margin: 4px 16px 4px 116px;
    border: 1px solid #30363d;
    border-radius: 6px;
    background: #0d1117;
  }

  textarea {
    width: 100%;
    min-height: 80px;
    padding: 8px 12px;
    border: none;
    background: transparent;
    color: #c9d1d9;
    font-family: inherit;
    font-size: 13px;
    resize: vertical;
    box-sizing: border-box;
  }

  textarea:focus {
    outline: none;
  }

  .new-comment-actions {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
    padding: 8px;
    border-top: 1px solid #30363d;
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

  .btn-submit:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }
</style>
