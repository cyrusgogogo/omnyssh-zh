// @vitest-environment jsdom
import { beforeEach, describe, expect, it, vi } from 'vitest';

// The canonical layer (tech-gui.md §5.1) needs a fake Tauri store: Vitest has no
// runtime. A fresh module per test isolates the store singleton's `interacted`.
const backend = { get: vi.fn(), set: vi.fn(), save: vi.fn(), onKeyChange: vi.fn() };
vi.mock('@tauri-apps/plugin-store', () => ({ load: vi.fn(async () => backend) }));

async function freshTheme() {
  vi.resetModules();
  return (await import('./theme')).theme;
}

describe('theme canonical persistence', () => {
  beforeEach(() => {
    localStorage.clear();
    document.documentElement.setAttribute('data-theme', 'dark');
    backend.get.mockReset();
    backend.set.mockReset().mockResolvedValue(undefined);
    backend.save.mockReset().mockResolvedValue(undefined);
    backend.onKeyChange.mockReset();
  });

  it('writes the canonical store on a user flip', async () => {
    const theme = await freshTheme();
    theme.set('light');
    await vi.waitFor(() => {
      expect(backend.set).toHaveBeenCalledWith('theme', 'light');
      expect(backend.save).toHaveBeenCalled();
    });
  });

  it('hydrate applies the stored value and refreshes the no-FOUC mirror', async () => {
    backend.get.mockResolvedValue('light');
    const theme = await freshTheme();
    await theme.hydrate();
    expect(document.documentElement.getAttribute('data-theme')).toBe('light');
    // Mirror must be rewritten or the next boot script repaints the stale theme.
    expect(localStorage.getItem('omnyssh-theme')).toBe('light');
  });

  it('hydrate does not clobber a fresh user toggle', async () => {
    backend.get.mockResolvedValue('dark'); // stale persisted value
    const theme = await freshTheme();
    theme.set('light'); // user acts before the async hydrate resolves
    await theme.hydrate();
    expect(document.documentElement.getAttribute('data-theme')).toBe('light');
  });

  it('follows another webview through the canonical store and disposes the listener', async () => {
    let receive: (theme: string) => void = () => {};
    const off = vi.fn();
    backend.onKeyChange.mockImplementation(async (_key, callback) => { receive = callback; return off; });
    backend.get.mockResolvedValue('dark');
    const theme = await freshTheme();
    const stop = theme.startSync();
    await vi.waitFor(() => expect(backend.get).toHaveBeenCalled());
    receive('light');
    expect(document.documentElement.getAttribute('data-theme')).toBe('light');
    expect(localStorage.getItem('omnyssh-theme')).toBe('light');
    expect(backend.set).not.toHaveBeenCalled(); // No cross-window write loop.
    stop();
    expect(off).toHaveBeenCalledOnce();
    receive('dark');
    expect(document.documentElement.getAttribute('data-theme')).toBe('light');
  });

  it('does not let an older hydration overwrite a live theme event', async () => {
    let resolve: (value: string) => void = () => {};
    let receive: (theme: string) => void = () => {};
    backend.get.mockReturnValue(new Promise<string>(r => { resolve = r; }));
    backend.onKeyChange.mockImplementation(async (_key, callback) => { receive = callback; return () => {}; });
    const theme = await freshTheme();
    const stop = theme.startSync();
    await vi.waitFor(() => expect(backend.get).toHaveBeenCalled());
    receive('light');
    resolve('dark');
    await Promise.resolve();
    expect(document.documentElement.getAttribute('data-theme')).toBe('light');
    stop();
  });
});
