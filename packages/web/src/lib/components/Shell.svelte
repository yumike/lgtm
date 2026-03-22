<script lang="ts">
    import { onMount } from 'svelte';
    import TabBar from './TabBar.svelte';
    import App from './App.svelte';
    import { listSessions } from '../api';
    import type { Session } from '../types';

    let sessions = $state<Session[]>([]);
    let activeSessionId = $state<string | null>(null);

    onMount(async () => {
        sessions = await listSessions();
        if (sessions.length > 0 && !activeSessionId) {
            activeSessionId = sessions[0].id;
        }
    });

    function handleSelect(id: string) {
        activeSessionId = id;
    }

    function handleClose(id: string) {
        sessions = sessions.filter(s => s.id !== id);
        if (activeSessionId === id) {
            activeSessionId = sessions.length > 0 ? sessions[0].id : null;
        }
    }

    // Poll for new sessions created via CLI
    onMount(() => {
        const interval = setInterval(async () => {
            const latest = await listSessions();
            for (const s of latest) {
                if (!sessions.find(existing => existing.id === s.id)) {
                    sessions = [...sessions, s];
                    activeSessionId = s.id;
                }
            }
        }, 2000);
        return () => clearInterval(interval);
    });
</script>

<div class="shell">
    {#if sessions.length > 0}
        <TabBar
            sessions={sessions}
            activeId={activeSessionId}
            onselect={handleSelect}
            onclose={handleClose}
        />
    {/if}

    {#if activeSessionId}
        {#each sessions as session (session.id)}
            <div class="tab-content" class:hidden={session.id !== activeSessionId}>
                <App sessionId={session.id} />
            </div>
        {/each}
    {:else}
        <div class="empty">
            <p>No active review sessions.</p>
            <p>Run <code>lgtm start</code> in a repo to begin.</p>
        </div>
    {/if}
</div>

<style>
    .shell {
        display: flex;
        flex-direction: column;
        height: 100vh;
    }
    .tab-content {
        flex: 1;
        overflow: hidden;
    }
    .tab-content.hidden {
        display: none;
    }
    .empty {
        display: flex;
        flex-direction: column;
        align-items: center;
        justify-content: center;
        height: 100%;
        color: #6c7086;
    }
    .empty code {
        background: #313244;
        padding: 2px 6px;
        border-radius: 4px;
    }
</style>
