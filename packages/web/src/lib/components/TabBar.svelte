<script lang="ts">
    import type { Session } from '../types';

    interface Props {
        sessions: Session[];
        activeId: string | null;
        onselect: (id: string) => void;
        onclose: (id: string) => void;
    }

    let { sessions, activeId, onselect, onclose }: Props = $props();

    function label(session: Session): string {
        const parts = session.repo_path.split('/');
        const repo = parts[parts.length - 1] || 'unknown';
        return `${repo} / ${session.head}`;
    }

    function openCount(session: Session): number {
        return session.threads.filter(t => t.status === 'open').length;
    }
</script>

<div class="tab-bar">
    {#each sessions as session (session.id)}
        <button
            class="tab"
            class:active={session.id === activeId}
            onclick={() => onselect(session.id)}
        >
            <span class="tab-label">{label(session)}</span>
            {#if openCount(session) > 0}
                <span class="badge">{openCount(session)}</span>
            {/if}
            <span
                class="tab-close"
                role="button"
                tabindex="-1"
                onclick={(e: MouseEvent) => { e.stopPropagation(); onclose(session.id); }}
                onkeydown={(e: KeyboardEvent) => { if (e.key === 'Enter') { e.stopPropagation(); onclose(session.id); } }}
            >×</span>
        </button>
    {/each}
</div>

<style>
    .tab-bar {
        display: flex;
        background: #1e1e2e;
        border-bottom: 1px solid #313244;
        overflow-x: auto;
        flex-shrink: 0;
    }
    .tab {
        display: flex;
        align-items: center;
        gap: 6px;
        padding: 8px 12px;
        background: transparent;
        border: none;
        border-bottom: 2px solid transparent;
        color: #6c7086;
        cursor: pointer;
        font-size: 13px;
        white-space: nowrap;
    }
    .tab:hover { color: #cdd6f4; }
    .tab.active {
        color: #cdd6f4;
        border-bottom-color: #89b4fa;
    }
    .badge {
        background: #f38ba8;
        color: #1e1e2e;
        border-radius: 10px;
        padding: 1px 6px;
        font-size: 11px;
        font-weight: 600;
    }
    .tab-close {
        background: none;
        border: none;
        color: inherit;
        cursor: pointer;
        padding: 0 2px;
        font-size: 14px;
        opacity: 0.5;
    }
    .tab-close:hover { opacity: 1; }
</style>
