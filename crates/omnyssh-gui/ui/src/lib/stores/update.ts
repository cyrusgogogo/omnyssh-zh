import { derived, get, writable } from 'svelte/store';
import { Channel } from '@tauri-apps/api/core';
import type { UpdateInfoDto, UpdateProgress } from '$lib/bindings';
import { installUpdate } from '$lib/ipc/commands';

// The update the banner offers, or null (up to date / dismissed / skipped). Set by the
// `update-available` startup event and by a manual `check_update` from Settings
// (tech-gui.md §4.3). The banner (AppShell) renders whenever this is non-null.
export const availableUpdate = writable<UpdateInfoDto | null>(null);

export const updateProgress = writable<UpdateProgress | null>(null);
export const updateError = writable<string | null>(null);
export const updateBusy = derived(updateProgress, (progress) => progress !== null);

/** Ignore double clicks and retain the offered version on failure for a retry. */
export async function downloadAndInstallUpdate(): Promise<void> {
  const info = get(availableUpdate);
  if (!info || get(updateBusy)) return;
  updateError.set(null);
  updateProgress.set({ phase: 'checking' });
  const channel = new Channel<UpdateProgress>();
  channel.onmessage = (progress) => updateProgress.set(progress);
  try {
    await installUpdate(info.version, channel);
    updateProgress.set({ phase: 'restarting' });
  } catch (error) {
    updateError.set(error instanceof Error ? error.message : String(error));
    updateProgress.set(null);
  }
}

/** Show an available update (from the event or a manual check). */
export function offerUpdate(info: UpdateInfoDto): void {
  if (get(updateBusy)) return;
  updateError.set(null);
  availableUpdate.set(info);
}

/** Dismiss the banner for this session (no persistence — it returns next launch). */
export function dismissUpdate(): void {
  if (get(updateBusy)) return;
  updateError.set(null);
  availableUpdate.set(null);
}
