<script lang="ts">
  import { onMount, tick } from 'svelte';
  import { Icon } from '$lib/theme';
  import { t } from '$lib/i18n';
  import { closeSftpCard, readySftpCard } from '$lib/ipc/commands';
  import { startSftpEventBridge } from '$lib/ipc/subscribe';
  import { sessions, type Session } from '$lib/stores/sessions';
  import { lastError } from '$lib/stores/notifications';
  import SftpView from './SftpView.svelte';

  let { hostName }: { hostName: string } = $props();
  let session = $state<Session | null>(null);
  let error = $state('');
  let closing = $state(false);
  const message = (value: unknown) => value instanceof Error ? value.message : String(value);

  onMount(() => {
    let disposed = false;
    let stop: (() => void) | undefined;
    void startSftpEventBridge().then(async (off) => {
      if (disposed) return off();
      stop = off;
      session = sessions.spawn('sftp', hostName);
      await tick();
      if (!disposed) await readySftpCard();
    }).catch((value) => error = message(value));
    return () => {
      disposed = true;
      stop?.();
      if (session) sessions.close(session.id);
    };
  });

  async function close(): Promise<void> {
    if (closing) return;
    closing = true;
    try { await closeSftpCard(); }
    catch (value) { error = message(value); closing = false; }
  }

  function onKeydown(event: KeyboardEvent) {
    if (event.key === 'Escape' && !event.defaultPrevented && !document.querySelector('[role="dialog"]')) {
      event.preventDefault();
      void close();
    }
  }
</script>

<svelte:window onkeydown={onKeydown} />

<main class="flex h-screen flex-col overflow-hidden rounded-2xl border border-default bg-surface text-fg" style="--titlebar-h: 0px" aria-label={$t('files-card-title')}>
  <header class="flex h-11 shrink-0 items-center gap-2 border-b border-default bg-surface-raised px-3" data-tauri-drag-region="deep">
    <Icon name="sftp" size={16} />
    <h1 class="min-w-0 flex-1 truncate text-sm font-semibold">{hostName} · SFTP</h1>
    <button
      type="button"
      class="grid h-7 w-7 place-items-center rounded-md text-muted hover:bg-surface-inset hover:text-fg focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-focus disabled:opacity-50"
      title={$t('common-close')}
      aria-label={$t('common-close')}
      disabled={closing}
      onclick={() => void close()}
    ><Icon name="close" size={15} /></button>
  </header>
  <div class="relative min-h-0 flex-1">
    {#if session}
      <SftpView {session} active={true} />
    {:else if !error}
      <p class="p-6 text-sm text-muted">{$t('common-loading')}</p>
    {/if}
  </div>
  {#if error || $lastError}
    <p class="shrink-0 break-words border-t border-default px-3 py-2 text-xs text-status-crit" role="alert">{error || $lastError}</p>
  {/if}
</main>
