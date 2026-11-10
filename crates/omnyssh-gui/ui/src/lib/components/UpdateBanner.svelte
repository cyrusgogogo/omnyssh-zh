<script lang="ts">
  import { Icon } from '$lib/theme';
  import { availableUpdate, dismissUpdate, downloadAndInstallUpdate, updateBusy, updateProgress, updateError } from '$lib/stores/update';
  import { loadUpdateConfig, saveUpdateConfig } from '$lib/ipc/commands';
  import { formatBytes } from '$lib/stores/sftp';
  import { lastError } from '$lib/stores/notifications';
  import { t } from '$lib/i18n';

  const message = (e: unknown): string => (e instanceof Error ? e.message : String(e));
  let skipping = $state(false);
  const busy = $derived(skipping || $updateBusy);
  const download = $derived($updateProgress?.phase === 'downloading' ? $updateProgress : null);
  const percent = $derived(download?.total ? Math.min(100, Math.floor(download.received / download.total * 100)) : undefined);

  // Persist the skip against the current config so `check_on_startup` is preserved.
  async function skip(version: string): Promise<void> {
    skipping = true;
    try {
      const config = await loadUpdateConfig();
      await saveUpdateConfig({ ...config, skipVersion: version });
      dismissUpdate();
    } catch (e) {
      lastError.set(message(e));
    } finally {
      skipping = false;
    }
  }

  const action =
    'rounded-full px-3 py-1.5 text-sm transition disabled:opacity-50 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-focus';
</script>

{#if $availableUpdate}
  {@const info = $availableUpdate}
  <div class="pointer-events-none fixed inset-x-0 bottom-14 z-40 flex justify-center px-4">
    <div
      class="pointer-events-auto flex w-full max-w-2xl flex-wrap items-center gap-3 rounded-2xl border border-default bg-surface-raised px-4 py-3 shadow-soft"
      role="status"
    >
      <span class="shrink-0 text-muted"><Icon name="download" size={18} /></span>
      <div class="min-w-0 flex-1">
        <p class="text-sm font-medium">{$t('update-description', { version: info.version })}</p>
        <p class="text-xs text-muted">
          {#if download}
            {$t('update-downloading')} {formatBytes(download.received)}{download.total ? ` / ${formatBytes(download.total)}` : ''}
          {:else if $updateProgress}
            {$t(`update-${$updateProgress.phase}`)}
          {:else}
            {$t('update-ready')}
          {/if}
        </p>
        {#if download}
          <div class="mt-2 h-1.5 overflow-hidden rounded-full bg-surface-inset" role="progressbar" aria-label={$t('update-downloading')} aria-valuemin="0" aria-valuemax="100" aria-valuenow={percent}>
            <div class="h-full rounded-full bg-accent" style:width={percent === undefined ? '35%' : `${percent}%`}></div>
          </div>
        {/if}
      </div>
      <div class="ml-auto flex shrink-0 items-center gap-1.5">
        <button
          type="button"
          class="{action} bg-accent text-accent-fg hover:opacity-90"
          disabled={busy}
          onclick={() => void downloadAndInstallUpdate()}
        >
          {$t($updateError ? 'update-retry' : 'update-download')}
        </button>
        <button
          type="button"
          class="{action} text-muted hover:bg-surface-inset hover:text-fg"
          disabled={busy}
          onclick={() => skip(info.version)}
        >
          {$t('update-skip')}
        </button>
        <button
          type="button"
          class="grid h-8 w-8 place-items-center rounded-full text-muted transition hover:bg-surface-inset hover:text-fg focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-focus"
          title={$t('update-later')}
          aria-label={$t('update-dismiss')}
          disabled={busy}
          onclick={dismissUpdate}
        >
          <Icon name="close" size={15} />
        </button>
      </div>
      {#if $updateError}
        <p class="w-full break-words text-xs text-status-crit" role="alert">{$updateError}</p>
      {/if}
    </div>
  </div>
{/if}
