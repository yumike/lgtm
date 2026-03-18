<script lang="ts">
  import { error } from '../stores/ui';

  $effect(() => {
    if ($error) {
      const timer = setTimeout(() => error.set(null), 5000);
      return () => clearTimeout(timer);
    }
  });
</script>

{#if $error}
  <div class="toast">
    <span>{$error}</span>
    <button onclick={() => error.set(null)}>&times;</button>
  </div>
{/if}

<style>
  .toast {
    position: fixed;
    bottom: 56px;
    right: 16px;
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 8px 16px;
    background: #f85149;
    color: white;
    border-radius: 6px;
    font-size: 13px;
    z-index: 100;
  }

  .toast button {
    background: none;
    border: none;
    color: white;
    cursor: pointer;
    font-size: 16px;
  }
</style>
