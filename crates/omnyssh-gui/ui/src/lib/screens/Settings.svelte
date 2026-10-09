<script lang="ts">
  // Minimal settings (tech-gui.md §4.3): the light/dark theme mirrored from the sidebar
  // (§5.1), the metric auto-refresh interval (a tauri-plugin-store UI pref), and the
  // update preferences (`check_on_startup`) + a manual update check. Update prefs persist
  // to the shared config via `save_update_config`; theme/interval are frontend prefs.
  import { onMount } from 'svelte';
  import type { UpdateConfigDto } from '$lib/bindings';
  import { Surface, Icon } from '$lib/theme';
  import { theme } from '$lib/stores/theme';
  import { streamerMode } from '$lib/stores/streamer';
  import {
    refreshInterval,
    terminalOpenMode,
    REFRESH_OPTIONS,
    type TerminalOpenMode
  } from '$lib/stores/settings';
  import { offerUpdate } from '$lib/stores/update';
  import { lastError } from '$lib/stores/notifications';
  import { checkUpdate, loadUpdateConfig, saveUpdateConfig } from '$lib/ipc/commands';
  import { language, languagePreference, t, type LanguagePreference } from '$lib/i18n';

  const message = (e: unknown): string => (e instanceof Error ? e.message : String(e));
  const formatInterval = (secs: number): string => (secs < 60 ? `${secs}s` : `${secs / 60}m`);

  let updateConfig = $state<UpdateConfigDto | null>(null);
  type CheckState =
    | { kind: 'idle' }
    | { kind: 'checking' }
    | { kind: 'upToDate' }
    | { kind: 'available'; version: string }
    | { kind: 'error'; message: string };
  let check = $state<CheckState>({ kind: 'idle' });

  onMount(async () => {
    try {
      updateConfig = await loadUpdateConfig();
    } catch (e) {
      lastError.set(message(e));
    }
  });

  // Persist immediately; revert the optimistic flip if the write fails. Read-modify-write
  // fresh rather than from the mount-time cache: the update banner may have written a
  // `skipVersion` out-of-band, and `save_update_config` replaces the whole [update]
  // section — writing a stale copy would clobber that skip.
  async function toggleCheckOnStartup(): Promise<void> {
    if (!updateConfig) return;
    const previous = updateConfig;
    const desired = !previous.checkOnStartup;
    updateConfig = { ...previous, checkOnStartup: desired };
    try {
      const current = await loadUpdateConfig();
      const next = { ...current, checkOnStartup: desired };
      await saveUpdateConfig(next);
      updateConfig = next;
    } catch (e) {
      updateConfig = previous;
      lastError.set(message(e));
    }
  }

  async function checkNow(): Promise<void> {
    check = { kind: 'checking' };
    try {
      const info = await checkUpdate();
      if (info) {
        offerUpdate(info); // raise the banner too
        check = { kind: 'available', version: info.version };
      } else {
        check = { kind: 'upToDate' };
      }
    } catch (e) {
      check = { kind: 'error', message: message(e) };
    }
  }

  async function selectTerminalMode(mode: TerminalOpenMode): Promise<void> {
    try {
      await terminalOpenMode.set(mode);
    } catch (e) {
      lastError.set(message(e));
    }
  }

  const seg =
    'rounded-lg px-3 py-1.5 text-sm transition focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-focus';
  const segState = (active: boolean): string =>
    active ? 'bg-accent text-accent-fg' : 'text-muted hover:bg-surface-inset hover:text-fg';
</script>

<section class="mx-auto h-full max-w-2xl p-6">
  <h1 class="mb-5 text-lg font-semibold tracking-tight">{$t('settings-title')}</h1>

  <div class="space-y-4">
    <!-- Appearance -->
    <Surface class="p-5">
      <h2 class="mb-3 text-sm font-semibold">{$t('settings-appearance')}</h2>
      <div class="flex items-center justify-between gap-4">
        <div>
          <p class="text-sm">{$t('settings-theme')}</p>
          <p class="text-xs text-muted">{$t('settings-theme-description')}</p>
        </div>
        <div class="flex gap-1 rounded-xl bg-surface-inset p-1">
          <button
            type="button"
            class="{seg} {segState($theme === 'light')}"
            aria-pressed={$theme === 'light'}
            onclick={() => theme.set('light')}
          >
            <span class="flex items-center gap-1.5"><Icon name="sun" size={14} /> {$t('settings-theme-light')}</span>
          </button>
          <button
            type="button"
            class="{seg} {segState($theme === 'dark')}"
            aria-pressed={$theme === 'dark'}
            onclick={() => theme.set('dark')}
          >
            <span class="flex items-center gap-1.5"><Icon name="moon" size={14} /> {$t('settings-theme-dark')}</span>
          </button>
        </div>
      </div>
      <div class="mt-4 flex items-center justify-between gap-4 border-t border-default pt-4">
        <div>
          <p class="text-sm">{$t('settings-language')}</p>
          <p class="text-xs text-muted">{$t('settings-language-description')}</p>
        </div>
        <div class="flex flex-wrap justify-end gap-1 rounded-xl bg-surface-inset p-1">
          {#each [
            ['system', 'settings-language-system'],
            ['en-US', 'settings-language-english'],
            ['zh-CN', 'settings-language-chinese']
          ] as option (option[0])}
            <button
              type="button"
              class="{seg} {segState($languagePreference === option[0])}"
              aria-pressed={$languagePreference === option[0]}
              onclick={() => language.set(option[0] as LanguagePreference)}
            >
              {$t(option[1])}
            </button>
          {/each}
        </div>
      </div>
    </Surface>

    <!-- Terminal -->
    <Surface class="p-5">
      <h2 class="mb-3 text-sm font-semibold">{$t('settings-terminal')}</h2>
      <div class="flex items-center justify-between gap-4">
        <div>
          <p class="text-sm">{$t('settings-terminal-open-mode')}</p>
          <p class="text-xs text-muted">{$t('settings-terminal-open-description')}</p>
        </div>
        <div class="flex shrink-0 gap-1 rounded-xl bg-surface-inset p-1">
          {#each [
            ['default', 'settings-terminal-default'],
            ['system', 'settings-terminal-system']
          ] as option (option[0])}
            <button
              type="button"
              class="{seg} {segState($terminalOpenMode === option[0])}"
              aria-pressed={$terminalOpenMode === option[0]}
              onclick={() => void selectTerminalMode(option[0] as TerminalOpenMode)}
            >
              {$t(option[1])}
            </button>
          {/each}
        </div>
      </div>
    </Surface>

    <Surface class="p-5">
      <h2 class="mb-3 text-sm font-semibold">{$t('desktop-card-title')}</h2>
      <div class="flex items-center justify-between gap-4">
        <div>
          <p class="text-sm">{$t('settings-desktop-card-shortcut')}</p>
          <p class="text-xs text-muted">{$t('settings-desktop-card-shortcut-description')}</p>
        </div>
        <kbd class="shrink-0 rounded-lg border border-default bg-surface-inset px-3 py-1.5 text-xs">Ctrl+Shift+H</kbd>
      </div>
    </Surface>

    <!-- Privacy -->
    <Surface class="p-5">
      <h2 class="mb-3 text-sm font-semibold">{$t('settings-privacy')}</h2>
      <div class="flex items-center justify-between gap-4">
        <div class="min-w-0">
          <p class="text-sm">{$t('settings-streamer-mode')}</p>
          <p class="text-xs text-muted">
            {$t('settings-streamer-description')}
          </p>
        </div>
        <button
          type="button"
          role="switch"
          aria-checked={$streamerMode}
          aria-label={$t('settings-streamer-mode')}
          onclick={() => streamerMode.toggle()}
          class="relative h-6 w-11 shrink-0 rounded-full transition focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-focus {$streamerMode
            ? 'bg-accent'
            : 'bg-surface-inset'}"
        >
          <span
            class="absolute top-0.5 h-5 w-5 rounded-full bg-surface shadow-soft transition-[left] {$streamerMode
              ? 'left-[1.375rem]'
              : 'left-0.5'}"
          ></span>
        </button>
      </div>
    </Surface>

    <!-- Dashboard -->
    <Surface class="p-5">
      <h2 class="mb-3 text-sm font-semibold">{$t('settings-dashboard')}</h2>
      <div class="flex items-center justify-between gap-4">
        <div>
          <p class="text-sm">{$t('settings-refresh-interval')}</p>
          <p class="text-xs text-muted">{$t('settings-refresh-description')}</p>
        </div>
        <div class="flex flex-wrap justify-end gap-1 rounded-xl bg-surface-inset p-1">
          {#each REFRESH_OPTIONS as secs (secs)}
            <button
              type="button"
              class="{seg} tabular-nums {segState($refreshInterval === secs)}"
              aria-pressed={$refreshInterval === secs}
              onclick={() => refreshInterval.set(secs)}
            >
              {formatInterval(secs)}
            </button>
          {/each}
        </div>
      </div>
    </Surface>

    <!-- Updates -->
    <Surface class="p-5">
      <h2 class="mb-3 text-sm font-semibold">{$t('settings-updates')}</h2>
      <div class="space-y-4">
        <div class="flex items-center justify-between gap-4">
          <div>
            <p class="text-sm">{$t('settings-check-startup')}</p>
            <p class="text-xs text-muted">{$t('settings-check-startup-description')}</p>
          </div>
          <button
            type="button"
            role="switch"
            aria-checked={updateConfig?.checkOnStartup ?? false}
            aria-label={$t('settings-check-startup')}
            disabled={!updateConfig}
            onclick={toggleCheckOnStartup}
            class="relative h-6 w-11 shrink-0 rounded-full transition disabled:opacity-50 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-focus {updateConfig?.checkOnStartup
              ? 'bg-accent'
              : 'bg-surface-inset'}"
          >
            <span
              class="absolute top-0.5 h-5 w-5 rounded-full bg-surface shadow-soft transition-[left] {updateConfig?.checkOnStartup
                ? 'left-[1.375rem]'
                : 'left-0.5'}"
            ></span>
          </button>
        </div>

        <div class="flex items-center justify-between gap-4 border-t border-default pt-4">
          <div class="min-w-0">
            <p class="text-sm">{$t('settings-manual-check')}</p>
            <p class="text-xs text-muted">
              {#if check.kind === 'checking'}{$t('settings-checking')}
              {:else if check.kind === 'upToDate'}{$t('settings-up-to-date')}
              {:else if check.kind === 'available'}{$t('settings-update-available', { version: check.version })}
              {:else if check.kind === 'error'}{check.message}
              {:else}{$t('settings-manual-description')}
              {/if}
            </p>
          </div>
          <button
            type="button"
            class="inline-flex shrink-0 items-center gap-1.5 rounded-full border border-strong px-4 py-1.5 text-sm text-fg transition hover:bg-accent hover:text-accent-fg disabled:opacity-50 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-focus"
            disabled={check.kind === 'checking'}
            onclick={checkNow}
          >
            <Icon name="refresh" size={14} />
            {$t('settings-check-now')}
          </button>
        </div>
      </div>
    </Surface>
  </div>
</section>
