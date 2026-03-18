<script lang="ts">
  import type { Comment as CommentType } from '../types';

  let { comment }: { comment: CommentType } = $props();

  function formatTime(ts: string): string {
    const d = new Date(ts);
    return d.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' });
  }
</script>

<div class="comment" class:agent={comment.author === 'agent'}>
  <div class="comment-header">
    <span class="comment-author">
      {comment.author}
    </span>
    <span class="comment-time">{formatTime(comment.timestamp)}</span>
  </div>
  <div class="comment-body">{comment.body}</div>
</div>

<style>
  .comment {
    padding: 8px 12px;
    border-bottom: 1px solid #21262d;
  }

  .comment:last-child {
    border-bottom: none;
  }

  .comment-header {
    display: flex;
    align-items: center;
    gap: 8px;
    margin-bottom: 4px;
  }

  .comment-author {
    font-size: 12px;
    font-weight: 600;
    color: #c9d1d9;
  }

  .comment-time {
    font-size: 11px;
    color: #484f58;
  }

  .comment-body {
    font-size: 13px;
    white-space: pre-wrap;
    color: #c9d1d9;
  }
</style>
