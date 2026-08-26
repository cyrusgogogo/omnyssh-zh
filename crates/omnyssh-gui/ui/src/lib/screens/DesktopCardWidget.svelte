<script lang="ts">
  import { onMount } from 'svelte';
  import { serverCards } from './serverCard';
  import { desktopCardHosts, desktopCardPinned } from '$lib/stores/desktopCards';
  import { streamerMode, displayHostname } from '$lib/stores/streamer';
  import {
    closeDesktopCard,
    openDesktopCardHost,
    setDesktopCardAlwaysOnTop
  } from '$lib/ipc/commands';
  import { lastError } from '$lib/stores/notifications';
  import { Icon, StatusDot, statusToken } from '$lib/theme';
  import { t } from '$lib/i18n';

  let currentIndex = $state(0);
  let pinBusy = $state(false);
  let actionBusy = $state<'terminal' | 'sftp' | null>(null);
  const selectedCards = $derived(
    $desktopCardHosts
      .map((name) => $serverCards.find((card) => card.host.name === name))
      .filter((card) => card !== undefined)
  );
  const currentCard = $derived(selectedCards[currentIndex]);

  $effect(() => {
    if (currentIndex >= selectedCards.length) currentIndex = Math.max(0, selectedCards.length - 1);
  });

  const message = (error: unknown): string =>
    error instanceof Error ? error.message : String(error);

  function move(offset: number): void {
    if (selectedCards.length < 2) return;
    currentIndex = (currentIndex + offset + selectedCards.length) % selectedCards.length;
  }

  async function togglePinned(): Promise<void> {
    if (pinBusy) return;
    pinBusy = true;
    const requested = !$desktopCardPinned;
    try {
      desktopCardPinned.set(await setDesktopCardAlwaysOnTop(requested));
    } catch (error) {
      lastError.set(message(error));
    } finally {
      pinBusy = false;
    }
  }

  async function openHost(kind: 'terminal' | 'sftp'): Promise<void> {
    if (!currentCard) return;
    actionBusy = kind;
    try {
      await openDesktopCardHost(currentCard.host.name, kind);
    } catch (error) {
      lastError.set(message(error));
    } finally {
      actionBusy = null;
    }
  }

  async function closeCard(): Promise<void> {
    try {
      await closeDesktopCard();
    } catch (error) {
      lastError.set(message(error));
    }
  }

  function onKeydown(event: KeyboardEvent): void {
    if (event.key === 'ArrowLeft') move(-1);
    if (event.key === 'ArrowRight') move(1);
  }

  onMount(() => {
    desktopCardHosts.syncFromStorage();
    desktopCardPinned.syncFromStorage();
    void setDesktopCardAlwaysOnTop($desktopCardPinned)
      .then((actual) => desktopCardPinned.set(actual))
      .catch((error) => lastError.set(message(error)));
    const sync = () => {
      desktopCardHosts.syncFromStorage();
      desktopCardPinned.syncFromStorage();
    };
    const timer = window.setInterval(sync, 750);
    window.addEventListener('storage', sync);
    return () => {
      window.clearInterval(timer);
      window.removeEventListener('storage', sync);
    };
  });
</script>

<svelte:window onkeydown={onKeydown} />

<main class="h-screen overflow-hidden bg-transparent text-fg">
  <section class="flex h-full flex-col overflow-hidden rounded-2xl border-0 bg-surface-raised">
    <div
      class="relative flex h-10 shrink-0 items-center border-b border-default px-2"
      data-tauri-drag-region="deep"
    >
      <div class="h-full min-w-0 flex-1 cursor-move select-none" aria-hidden="true"></div>
      {#if selectedCards.length > 0}
        <nav
          class="desktop-card-switcher absolute left-1/2 top-1/2 flex max-w-48 -translate-x-1/2 -translate-y-1/2 items-center overflow-x-auto"
          aria-label={$t('desktop-card-host-switcher')}
        >
          {#each selectedCards as card, index (card.host.name)}
            <button
              type="button"
              class="group grid h-7 w-7 shrink-0 place-items-center rounded-full focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-focus"
              title={$t('desktop-card-select-host', { host: card.host.name })}
              aria-label={$t('desktop-card-select-host', { host: card.host.name })}
              aria-current={index === currentIndex ? 'true' : undefined}
              onclick={() => (currentIndex = index)}
            >
              <span
                class="rounded-full transition-transform motion-reduce:transition-none group-hover:scale-125 {index ===
                currentIndex
                  ? 'h-3 w-3 ring-2 ring-focus ring-offset-2 ring-offset-surface-raised'
                  : 'h-2 w-2 opacity-75'}"
                style="background-color: {statusToken(card.overall)};"
                aria-hidden="true"
              ></span>
            </button>
          {/each}
        </nav>
      {/if}
      <button
        type="button"
        class="grid h-7 w-7 place-items-center rounded-md transition focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-focus disabled:cursor-wait disabled:opacity-70 {$desktopCardPinned
          ? 'bg-accent text-accent-fg shadow-sm'
          : 'text-muted hover:bg-surface-inset hover:text-fg'}"
        title={$desktopCardPinned ? $t('desktop-card-unpin') : $t('desktop-card-pin')}
        aria-label={$desktopCardPinned ? $t('desktop-card-unpin') : $t('desktop-card-pin')}
        aria-pressed={$desktopCardPinned}
        aria-busy={pinBusy}
        disabled={pinBusy}
        onclick={() => void togglePinned()}
      >
        <Icon name="pin" size={14} />
      </button>
      <button
        type="button"
        class="grid h-7 w-7 place-items-center rounded-md text-muted transition hover:bg-surface-inset hover:text-fg focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-focus"
        title={$t('common-close')}
        aria-label={$t('common-close')}
        onclick={() => void closeCard()}
      >
        <Icon name="close" size={14} />
      </button>
    </div>

    {#if currentCard}
      <div class="flex min-h-0 flex-1 flex-col gap-3 p-4">
        <div class="flex min-w-0 items-start gap-2.5">
          <span class="mt-1 shrink-0">
            <StatusDot
              status={currentCard.overall}
              size={9}
              label={$t('dashboard-host-status', { host: currentCard.host.name })}
            />
          </span>
          <div class="min-w-0 flex-1">
            <div class="flex min-w-0 items-center">
              <span class="truncate font-semibold" title={currentCard.host.name}>{currentCard.host.name}</span>
            </div>
            <div class="truncate font-mono text-xs text-faint">
              {currentCard.host.user}@{displayHostname(currentCard.host.hostname, $streamerMode)}:{currentCard.host.port}
            </div>
          </div>
        </div>

        {#if currentCard.reachability}
          <div
            class="rounded-lg bg-surface-inset px-3 py-4 text-center text-xs"
            style="color: {statusToken(currentCard.overall)};"
          >
            {$t(`desktop-card-${currentCard.reachability}`)}
          </div>
        {:else if currentCard.offline}
          <div class="rounded-lg bg-surface-inset px-3 py-4 text-center text-xs text-faint">
            {$t('dashboard-status-offline')}
          </div>
        {:else}
          <div class="space-y-2.5">
            {#each currentCard.metricRows as row (row.label)}
              <div class="flex items-center gap-3">
                <span class="w-9 shrink-0 text-[11px] uppercase tracking-wider text-faint">{row.label}</span>
                <div class="h-1.5 flex-1 overflow-hidden rounded-full bg-surface-inset">
                  {#if row.percent != null}
                    <div
                      class="h-full rounded-full"
                      style="width: {Math.min(row.percent, 100)}%; background-color: {statusToken(row.status)};"
                    ></div>
                  {/if}
                </div>
                <span class="w-10 shrink-0 text-right text-xs tabular-nums text-muted">
                  {row.percent != null ? `${Math.round(row.percent)}%` : '—'}
                </span>
              </div>
            {/each}
          </div>
          {#if currentCard.uptime || currentCard.osInfo}
            <div class="flex min-w-0 items-center gap-2 text-xs text-muted">
              {#if currentCard.uptime}<span>{$t('dashboard-up-for', { uptime: currentCard.uptime })}</span>{/if}
              {#if currentCard.uptime && currentCard.osInfo}<span class="text-faint">·</span>{/if}
              {#if currentCard.osInfo}<span class="truncate">{currentCard.osInfo}</span>{/if}
            </div>
          {/if}
        {/if}

        <div class="mt-auto grid grid-cols-2 gap-2">
          <button
            type="button"
            class="flex h-8 items-center justify-center gap-2 rounded-lg bg-accent px-3 text-xs font-medium text-accent-fg transition hover:opacity-90 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-focus disabled:cursor-wait disabled:opacity-60"
            aria-label={$t('desktop-card-open-terminal', { host: currentCard.host.name })}
            disabled={actionBusy !== null}
            onclick={() => void openHost('terminal')}
          >
            <Icon name="terminal" size={14} />
            <span>{$t('desktop-card-terminal')}</span>
          </button>
          <button
            type="button"
            class="flex h-8 items-center justify-center gap-2 rounded-lg bg-surface-inset px-3 text-xs font-medium text-fg transition hover:bg-surface focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-focus disabled:cursor-wait disabled:opacity-60"
            aria-label={$t('desktop-card-open-sftp', { host: currentCard.host.name })}
            disabled={actionBusy !== null}
            onclick={() => void openHost('sftp')}
          >
            <Icon name="sftp" size={14} />
            <span>{$t('desktop-card-sftp')}</span>
          </button>
        </div>

      </div>
    {:else}
      <div class="flex min-h-0 flex-1 flex-col items-center justify-center gap-2 px-6 text-center">
        <Icon name="desktop-card" size={24} />
        <p class="text-sm font-medium">{$t('desktop-card-empty')}</p>
        <p class="text-xs text-faint">{$t('desktop-card-empty-description')}</p>
      </div>
    {/if}
  </section>
</main>
