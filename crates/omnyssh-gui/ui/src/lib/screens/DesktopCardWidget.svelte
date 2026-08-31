<script lang="ts">
  import { onMount } from 'svelte';
  import { serverCards } from './serverCard';
  import {
    desktopCardCompact,
    desktopCardHosts,
    desktopCardPinned
  } from '$lib/stores/desktopCards';
  import { streamerMode, displayHostname } from '$lib/stores/streamer';
  import {
    closeDesktopCard,
    openDesktopCardHost,
    setDesktopCardAlwaysOnTop,
    setDesktopCardCompact
  } from '$lib/ipc/commands';
  import { lastError } from '$lib/stores/notifications';
  import { Icon, StatusDot, statusToken } from '$lib/theme';
  import { t } from '$lib/i18n';

  let currentIndex = $state(0);
  let hoveredIndex = $state<number | null>(null);
  let lockedIndex = $state<number | null>(null);
  let pinBusy = $state(false);
  let actionBusy = $state<'terminal' | 'sftp' | null>(null);
  let actionError = $state<string | null>(null);
  let expandedAbove = $state(false);
  let resizeKey = '';
  let resizeQueue: Promise<void> = Promise.resolve();
  const selectedCards = $derived(
    $desktopCardHosts
      .map((name) => $serverCards.find((card) => card.host.name === name))
      .filter((card) => card !== undefined)
  );
  const detailIndex = $derived(
    $desktopCardCompact ? (lockedIndex ?? hoveredIndex) : currentIndex
  );
  const currentCard = $derived(detailIndex == null ? undefined : selectedCards[detailIndex]);
  const dotsOnly = $derived($desktopCardCompact && currentCard === undefined);

  $effect(() => {
    if (currentIndex >= selectedCards.length) currentIndex = Math.max(0, selectedCards.length - 1);
    if (hoveredIndex != null && hoveredIndex >= selectedCards.length) hoveredIndex = null;
    if (lockedIndex != null && lockedIndex >= selectedCards.length) lockedIndex = null;
  });

  $effect(() => {
    requestWindowLayout($desktopCardCompact && currentCard === undefined, selectedCards.length);
  });

  const message = (error: unknown): string =>
    error instanceof Error ? error.message : String(error);

  function requestWindowLayout(compact: boolean, hostCount: number): void {
    const nextKey = `${compact}:${hostCount}`;
    if (nextKey === resizeKey) return;
    resizeKey = nextKey;
    resizeQueue = resizeQueue
      .then(async () => {
        const opensAbove = await setDesktopCardCompact(compact, hostCount, expandedAbove);
        expandedAbove = compact ? false : opensAbove;
      })
      .catch((error) => lastError.set(message(error)));
  }

  function move(offset: number): void {
    if (selectedCards.length < 2) return;
    currentIndex = (currentIndex + offset + selectedCards.length) % selectedCards.length;
  }

  function minimizeCard(): void {
    hoveredIndex = null;
    lockedIndex = null;
    desktopCardCompact.set(true);
  }

  function restoreCard(): void {
    hoveredIndex = null;
    lockedIndex = null;
    desktopCardCompact.set(false);
  }

  function previewHost(index: number): void {
    if (lockedIndex == null) hoveredIndex = index;
  }

  function toggleHostLock(index: number): void {
    currentIndex = index;
    lockedIndex = lockedIndex === index ? null : index;
  }

  function onCardPointerLeave(): void {
    if ($desktopCardCompact && lockedIndex == null) hoveredIndex = null;
  }

  function onCardFocusOut(event: FocusEvent & { currentTarget: HTMLElement }): void {
    if (event.relatedTarget instanceof Node && event.currentTarget.contains(event.relatedTarget)) return;
    if ($desktopCardCompact && lockedIndex == null) hoveredIndex = null;
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
    const hostName = currentCard.host.name;
    actionBusy = kind;
    actionError = null;
    try {
      await openDesktopCardHost(hostName, kind);
    } catch (error) {
      const detail = message(error);
      if ($desktopCardCompact) {
        const currentHostIndex = selectedCards.findIndex((card) => card.host.name === hostName);
        if (currentHostIndex >= 0) lockedIndex = currentHostIndex;
        else desktopCardCompact.set(false);
      }
      actionError = detail;
      lastError.set(detail);
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
    if ($desktopCardCompact) return;
    if (event.key === 'ArrowLeft') move(-1);
    if (event.key === 'ArrowRight') move(1);
  }

  onMount(() => {
    desktopCardHosts.syncFromStorage();
    desktopCardPinned.syncFromStorage();
    desktopCardCompact.syncFromStorage();
    void setDesktopCardAlwaysOnTop($desktopCardPinned)
      .then((actual) => desktopCardPinned.set(actual))
      .catch((error) => lastError.set(message(error)));
    const sync = () => {
      desktopCardHosts.syncFromStorage();
      desktopCardPinned.syncFromStorage();
      desktopCardCompact.syncFromStorage();
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
  <section
    class="relative flex h-full overflow-hidden border-0 bg-surface-raised {$desktopCardCompact &&
    expandedAbove
      ? 'flex-col-reverse'
      : 'flex-col'} {dotsOnly
      ? 'rounded-full'
      : 'rounded-2xl'}"
    data-mode={dotsOnly ? 'dots' : $desktopCardCompact ? 'compact-detail' : 'expanded'}
    role="group"
    aria-label={$t('desktop-card-title')}
    onpointerleave={onCardPointerLeave}
    onfocusout={onCardFocusOut}
  >
    {#if $desktopCardCompact}
      <div
        class="flex h-12 shrink-0 items-center justify-center overflow-hidden px-3"
        data-tauri-drag-region="deep"
      >
        <nav
          class="desktop-card-switcher flex max-w-full items-center overflow-x-auto"
          aria-label={$t('desktop-card-host-switcher')}
        >
          {#if selectedCards.length === 0}
            <button
              type="button"
              class="grid h-8 w-8 shrink-0 place-items-center rounded-full text-muted focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-focus"
              title={$t('desktop-card-restore')}
              aria-label={$t('desktop-card-restore')}
              onclick={restoreCard}
            >
              <Icon name="maximize" size={14} />
            </button>
          {/if}
          {#each selectedCards as card, index (card.host.name)}
            <button
              type="button"
              class="group grid h-8 w-8 shrink-0 place-items-center rounded-full focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-focus"
              title={lockedIndex === index
                ? $t('desktop-card-unlock-host', { host: card.host.name })
                : $t('desktop-card-lock-host', { host: card.host.name })}
              aria-label={lockedIndex === index
                ? $t('desktop-card-unlock-host', { host: card.host.name })
                : $t('desktop-card-lock-host', { host: card.host.name })}
              aria-pressed={lockedIndex === index}
              aria-expanded={detailIndex === index}
              onpointerenter={() => previewHost(index)}
              onfocus={() => previewHost(index)}
              onclick={() => toggleHostLock(index)}
            >
              <span
                class="h-3 w-3 rounded-full transition-transform motion-reduce:transition-none group-hover:scale-125 {detailIndex ===
                index
                  ? 'ring-2 ring-focus ring-offset-2 ring-offset-surface-raised'
                  : 'opacity-80'}"
                style="background-color: {statusToken(card.overall)};"
                aria-hidden="true"
              ></span>
            </button>
          {/each}
        </nav>
      </div>
    {:else}
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
          class="grid h-7 w-7 place-items-center rounded-md text-muted transition hover:bg-surface-inset hover:text-fg focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-focus"
          title={$t('desktop-card-minimize')}
          aria-label={$t('desktop-card-minimize')}
          onclick={minimizeCard}
        >
          <Icon name="minimize" size={14} />
        </button>
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
    {/if}

    {#each currentCard ? [currentCard] : [] as card (card.host.name)}
      <div class="flex min-h-0 flex-1 flex-col gap-3 p-4">
        <div class="flex min-w-0 items-start gap-2.5">
          <span class="mt-1 shrink-0">
            <StatusDot
              status={card.overall}
              size={9}
              label={$t('dashboard-host-status', { host: card.host.name })}
            />
          </span>
          <div class="min-w-0 flex-1">
            <div class="flex min-w-0 items-center gap-2">
              <span class="truncate font-semibold" title={card.host.name}>{card.host.name}</span>
              {#if lockedIndex === detailIndex}
                <span class="flex shrink-0 items-center gap-1 text-[10px] text-faint">
                  <Icon name="pin" size={10} />
                  {$t('desktop-card-status-locked')}
                </span>
              {/if}
              <div class="ml-1 flex shrink-0 items-center gap-1" data-desktop-card-host-actions>
                <button
                  type="button"
                  class="grid h-6 w-6 place-items-center rounded-md bg-surface-inset text-muted transition hover:bg-surface hover:text-fg focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-focus disabled:cursor-wait disabled:opacity-60"
                  title={$t('desktop-card-open-terminal', { host: card.host.name })}
                  aria-label={$t('desktop-card-open-terminal', { host: card.host.name })}
                  disabled={actionBusy !== null}
                  onclick={() => void openHost('terminal')}
                >
                  <Icon name="terminal" size={13} />
                </button>
                <button
                  type="button"
                  class="grid h-6 w-6 place-items-center rounded-md bg-surface-inset text-muted transition hover:bg-surface hover:text-fg focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-focus disabled:cursor-wait disabled:opacity-60"
                  title={$t('desktop-card-open-sftp', { host: card.host.name })}
                  aria-label={$t('desktop-card-open-sftp', { host: card.host.name })}
                  disabled={actionBusy !== null}
                  onclick={() => void openHost('sftp')}
                >
                  <Icon name="sftp" size={13} />
                </button>
              </div>
            </div>
            <div class="truncate font-mono text-xs text-faint">
              {card.host.user}@{displayHostname(card.host.hostname, $streamerMode)}:{card.host.port}
            </div>
          </div>
          {#if $desktopCardCompact}
            <button
              type="button"
              class="grid h-7 w-7 shrink-0 place-items-center rounded-md text-muted transition hover:bg-surface-inset hover:text-fg focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-focus"
              title={$t('desktop-card-restore')}
              aria-label={$t('desktop-card-restore')}
              onclick={restoreCard}
            >
              <Icon name="maximize" size={14} />
            </button>
            <button
              type="button"
              class="grid h-7 w-7 shrink-0 place-items-center rounded-md text-muted transition hover:bg-surface-inset hover:text-fg focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-focus"
              title={$t('common-close')}
              aria-label={$t('common-close')}
              onclick={() => void closeCard()}
            >
              <Icon name="close" size={14} />
            </button>
          {/if}
        </div>

        {#if card.reachability}
          <div
            class="rounded-lg bg-surface-inset px-3 py-4 text-center text-xs"
            style="color: {statusToken(card.overall)};"
          >
            {$t(`desktop-card-${card.reachability}`)}
          </div>
        {:else if card.offline}
          <div class="rounded-lg bg-surface-inset px-3 py-4 text-center text-xs text-faint">
            {$t('dashboard-status-offline')}
          </div>
        {:else}
          <div class="space-y-2.5">
            {#each card.metricRows as row (row.label)}
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
          {#if card.uptime || card.osInfo}
            <div class="flex min-w-0 items-center gap-2 text-xs text-muted">
              {#if card.uptime}<span>{$t('dashboard-up-for', { uptime: card.uptime })}</span>{/if}
              {#if card.uptime && card.osInfo}<span class="text-faint">·</span>{/if}
              {#if card.osInfo}<span class="truncate">{card.osInfo}</span>{/if}
            </div>
          {/if}
          {#if card.topProcesses.length}
            <ul class="space-y-1 rounded-lg bg-surface-inset px-3 py-2">
              {#each card.topProcesses.slice(0, 3) as process, index (`${process.name}-${index}`)}
                <li class="flex items-center justify-between gap-3 text-xs">
                  <span class="min-w-0 truncate font-mono text-muted" title={process.name}
                    >{process.name}</span
                  >
                  <span class="shrink-0 tabular-nums text-faint"
                    >CPU {Math.round(process.cpuPercent)}% · MEM {Math.round(process.memPercent)}%</span
                  >
                </li>
              {/each}
            </ul>
          {/if}
        {/if}

      </div>
    {:else}
      {#if !$desktopCardCompact}
        <div class="flex min-h-0 flex-1 flex-col items-center justify-center gap-2 px-6 text-center">
          <Icon name="desktop-card" size={24} />
          <p class="text-sm font-medium">{$t('desktop-card-empty')}</p>
          <p class="text-xs text-faint">{$t('desktop-card-empty-description')}</p>
        </div>
      {/if}
    {/each}

    {#if actionError}
      <div
        class="absolute left-3 right-3 z-20 flex max-h-24 items-start gap-2 overflow-auto rounded-lg border border-status-crit/40 bg-surface-raised p-2 shadow-lg {$desktopCardCompact &&
        expandedAbove
          ? 'bottom-14'
          : 'bottom-3'}"
        role="alert"
      >
        <span class="min-w-0 flex-1 break-words text-xs text-status-crit">{actionError}</span>
        <button
          type="button"
          class="grid h-6 w-6 shrink-0 place-items-center rounded-md text-muted transition hover:bg-surface-inset hover:text-fg focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-focus"
          title={$t('common-close')}
          aria-label={$t('common-close')}
          onclick={() => (actionError = null)}
        >
          <Icon name="close" size={12} />
        </button>
      </div>
    {/if}
  </section>
</main>
