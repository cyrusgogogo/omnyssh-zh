<script lang="ts">
  import '../app.css';
  import { onMount } from 'svelte';
  import { listen } from '@tauri-apps/api/event';
  import { startEventBridge } from '$lib/ipc/subscribe';
  import { reloadHosts, refreshMetrics } from '$lib/ipc/commands';
  import { theme } from '$lib/stores/theme';
  import { sidebarCollapsed } from '$lib/stores/ui';
  import { streamerMode } from '$lib/stores/streamer';
  import { refreshInterval, terminalOpenMode, driveMetricsRefresh } from '$lib/stores/settings';
  import { lastError } from '$lib/stores/notifications';
  import { openHostSession } from '$lib/stores/navigation';
  import type { SessionKind } from '$lib/stores/sessions';
  import { installAutoHideScrollbars } from '$lib/scrollbars';
  import { language } from '$lib/i18n';

  let { children } = $props();

  onMount(() => {
    let stop: (() => void) | undefined;
    let stopDesktopCardAction: (() => void) | undefined;
    let disposed = false;
    const desktopCardView = new URLSearchParams(window.location.search).get('view') === 'desktop-card';
    const stopScrollbars = installAutoHideScrollbars();
    // Reconcile the persisted prefs with their canonical tauri-plugin-store values;
    // the synchronous localStorage mirrors already seeded the first paint (§5.1, §2).
    void theme.hydrate();
    if (!desktopCardView) void sidebarCollapsed.hydrate();
    void streamerMode.hydrate();
    if (!desktopCardView) {
      void refreshInterval.hydrate();
      void terminalOpenMode.hydrate().catch(() => {});
    }
    void language.hydrate();
    if (!desktopCardView) {
      void listen<{ hostName: string; kind: SessionKind }>(
        'desktop-card-open-host',
        ({ payload }) => openHostSession(payload.kind, payload.hostName)
      )
        .then((off) => {
          if (disposed) return off();
          stopDesktopCardAction = off;
        })
        .catch(() => {});
    }
    // Force a metric refresh on the user's interval; re-arms when the interval changes.
    const stopRefresh = desktopCardView
      ? () => {}
      : driveMetricsRefresh(() => {
          void refreshMetrics().catch(() => {});
        });
    // No-op outside Tauri (e.g. a plain `vite preview`); the shell still mounts.
    // Dispose even if the layout unmounts before the subscription resolves. Start
    // the pollers only once listeners are attached, so no status event is missed.
    startEventBridge()
      .then((off) => {
        if (disposed) return off();
        stop = off;
        if (!desktopCardView) {
          reloadHosts().catch((err) => lastError.set(err instanceof Error ? err.message : String(err)));
        }
      })
      .catch(() => {});
    return () => {
      disposed = true;
      stop?.();
      stopDesktopCardAction?.();
      stopRefresh();
      stopScrollbars();
    };
  });
</script>

{@render children()}
