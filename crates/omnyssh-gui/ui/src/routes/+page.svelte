<!-- Content follows the active entity (tech-gui.md §2): exactly one of Dashboard,
     Snippets, or one session. Terminal and SFTP tabs live in a persistent layer so
     their scrollback / byte stream (terminal) and pane state (SFTP) survive switching
     away — only visibility toggles. The list_hosts call feeds the status-bar count. -->
<script lang="ts">
  import { onMount } from 'svelte';
  import { get } from 'svelte/store';
  import { page } from '$app/stores';
  import { getDesktopCardSnapshot, listHosts } from '$lib/ipc/commands';
  import { applyHostRuntimeSnapshot } from '$lib/ipc/router';
  import { hosts } from '$lib/stores/hosts';
  import { lastError } from '$lib/stores/notifications';
  import { activeEntity } from '$lib/stores/activeEntity';
  import { sessions } from '$lib/stores/sessions';
  import AppShell from '$lib/components/AppShell.svelte';
  import Dashboard from '$lib/screens/Dashboard.svelte';
  import Snippets from '$lib/screens/Snippets.svelte';
  import Settings from '$lib/screens/Settings.svelte';
  import SshConfig from '$lib/screens/SshConfig.svelte';
  import TerminalView from '$lib/screens/TerminalView.svelte';
  import SftpView from '$lib/screens/SftpView.svelte';
  import DesktopCardWidget from '$lib/screens/DesktopCardWidget.svelte';
  import { desktopCardHosts } from '$lib/stores/desktopCards';

  onMount(async () => {
    try {
      const loaded = await listHosts();
      hosts.set(loaded);
      if (new URLSearchParams(window.location.search).get('view') === 'desktop-card') {
        const available = new Set(loaded.map((host) => host.name));
        desktopCardHosts.set(get(desktopCardHosts).filter((name) => available.has(name)));
        applyHostRuntimeSnapshot(await getDesktopCardSnapshot());
      }
    } catch (err) {
      lastError.set(err instanceof Error ? err.message : String(err));
    }
  });

  const activeSessionId = $derived($activeEntity.kind === 'session' ? $activeEntity.id : null);
  // A selector (Dashboard/Snippets) owns the overlay; a session owns the persistent
  // layer. The two are mutually exclusive — the §2 exactly-one-active invariant.
  const selectorActive = $derived(
    $activeEntity.kind === 'dashboard' ||
      $activeEntity.kind === 'snippets' ||
      $activeEntity.kind === 'sshConfig' ||
      $activeEntity.kind === 'settings'
  );
  const desktopCardView = $derived($page.url.searchParams.get('view') === 'desktop-card');
</script>

{#if desktopCardView}
  <DesktopCardWidget />
{:else}
<AppShell>
  <div class="relative h-full">
    {#each $sessions as s (s.id)}
      {#if s.kind === 'terminal'}
        <TerminalView session={s} active={activeSessionId === s.id} />
      {:else}
        <SftpView session={s} active={activeSessionId === s.id} />
      {/if}
    {/each}

    {#if selectorActive}
      <!-- Inset the scroll container (not the content) below the macOS title-bar strip,
           so selector content scrolls within its pane and never under the traffic lights. -->
      <div class="absolute inset-0 pt-[var(--titlebar-h)]">
        <div class="h-full overflow-auto overscroll-contain">
          {#if $activeEntity.kind === 'dashboard'}
            <Dashboard />
          {:else if $activeEntity.kind === 'snippets'}
            <Snippets />
          {:else if $activeEntity.kind === 'settings'}
            <Settings />
          {:else if $activeEntity.kind === 'sshConfig'}
            <SshConfig />
          {/if}
        </div>
      </div>
    {/if}
  </div>
</AppShell>
{/if}
