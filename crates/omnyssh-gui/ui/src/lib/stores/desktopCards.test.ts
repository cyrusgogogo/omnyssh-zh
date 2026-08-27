import { afterEach, describe, expect, it, vi } from 'vitest';
import {
  addDesktopCardHost,
  desktopCardCompact,
  normalizeDesktopCardHosts,
  removeDesktopCardHost
} from './desktopCards';
import { get } from 'svelte/store';

describe('desktop card host selection', () => {
  afterEach(() => vi.unstubAllGlobals());

  it('keeps the first occurrence and removes invalid names', () => {
    expect(normalizeDesktopCardHosts([' alpha ', 'beta', 'alpha', '', 4])).toEqual([
      'alpha',
      'beta'
    ]);
  });

  it('adds without duplicating and removes only the selected host', () => {
    expect(addDesktopCardHost(['alpha'], 'alpha')).toEqual(['alpha']);
    expect(addDesktopCardHost(['alpha'], 'beta')).toEqual(['alpha', 'beta']);
    expect(removeDesktopCardHost(['alpha', 'beta'], 'alpha')).toEqual(['beta']);
  });

  it('persists and restores the compact layout preference', () => {
    const values = new Map<string, string>();
    vi.stubGlobal('localStorage', {
      getItem: (key: string) => values.get(key) ?? null,
      setItem: (key: string, value: string) => values.set(key, value)
    });

    desktopCardCompact.set(true);
    expect(get(desktopCardCompact)).toBe(true);
    expect(localStorage.getItem('omnyssh-desktop-card-compact')).toBe('true');

    localStorage.setItem('omnyssh-desktop-card-compact', 'false');
    desktopCardCompact.syncFromStorage();
    expect(get(desktopCardCompact)).toBe(false);
  });
});
