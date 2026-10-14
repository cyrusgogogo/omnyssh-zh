import { get, writable } from 'svelte/store';
import { listen } from '@tauri-apps/api/event';
import type { DesktopSettingsDto } from '$lib/bindings';
import { loadDesktopSettings, saveDesktopSettings } from '$lib/ipc/commands';

export const desktopSettings = writable<DesktopSettingsDto>({
  closeBehavior: 'tray', trayClickBehavior: 'openMain', shortcut: 'Control+Shift+H',
  shortcutRegistered: false, shortcutError: null
});

export async function startDesktopSettings(): Promise<() => void> {
  let version = 0;
  const off = await listen<DesktopSettingsDto>('desktop-settings-changed', ({ payload }) => {
    version++;
    desktopSettings.set(payload);
  });
  try {
    const saved = await loadDesktopSettings();
    if (version === 0 && saved) desktopSettings.set(saved);
    return off;
  } catch (error) { off(); throw error; }
}

export async function changeDesktopSettings(patch: Partial<DesktopSettingsDto>, retry = false) {
  const saved = await saveDesktopSettings({ ...get(desktopSettings), ...patch }, retry);
  desktopSettings.set(saved);
}

export function shortcutLabel(value: string): string {
  return value.replace(/Control/g, 'Ctrl').replace(/Key([A-Z])/g, '$1').replace(/Digit([0-9])/g, '$1');
}

/** KeyboardEvent.code uses the same physical-key vocabulary as global-hotkey. */
export function captureShortcut(event: Pick<KeyboardEvent, 'code' | 'ctrlKey' | 'altKey' | 'shiftKey' | 'metaKey'>): string | null {
  if (!event.ctrlKey && !event.altKey && !event.metaKey) return null;
  if (!/^(Key[A-Z]|Digit[0-9]|F([1-9]|1[0-9]|2[0-4])|Space|Arrow(Up|Down|Left|Right)|Home|End|PageUp|PageDown|Insert|Delete)$/.test(event.code)) return null;
  return [event.ctrlKey && 'Control', event.altKey && 'Alt', event.shiftKey && 'Shift', event.metaKey && 'Super', event.code].filter(Boolean).join('+');
}
