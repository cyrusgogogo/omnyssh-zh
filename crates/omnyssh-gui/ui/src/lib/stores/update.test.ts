import { beforeEach, describe, expect, it, vi } from 'vitest';
import { get } from 'svelte/store';
import type { Channel } from '@tauri-apps/api/core';
import type { UpdateInfoDto, UpdateProgress } from '$lib/bindings';
import { installUpdate } from '$lib/ipc/commands';
import { availableUpdate, dismissUpdate, offerUpdate, downloadAndInstallUpdate, updateBusy, updateError, updateProgress } from './update';
import { applyUpdateAvailable } from '$lib/ipc/router';

vi.mock('$lib/ipc/commands', () => ({ installUpdate: vi.fn() }));
vi.mock('@tauri-apps/api/core', () => ({ Channel: class { onmessage = (_: unknown) => {}; } }));

const info: UpdateInfoDto = {
  version: '1.2.0', url: 'https://github.com/cyrusgogogo/omnyssh-zh/releases/tag/v1.2.0',
  tag: 'v1.2.0', canSelfUpdate: true
};

describe('desktop update flow', () => {
  beforeEach(() => { vi.clearAllMocks(); updateProgress.set(null); dismissUpdate(); });

  it('routes startup updates and dismisses them', () => {
    expect(get(availableUpdate)).toBeNull();
    applyUpdateAvailable({ info });
    expect(get(availableUpdate)).toEqual(info);
    dismissUpdate();
    expect(get(availableUpdate)).toBeNull();
  });

  it('pins the version, reports progress and prevents duplicate installs or dismissal', async () => {
    let finish!: () => void;
    let channel!: Channel<UpdateProgress>;
    vi.mocked(installUpdate).mockImplementation((_version, progress) => {
      channel = progress;
      return new Promise<void>((resolve) => { finish = resolve; });
    });
    offerUpdate(info);
    const pending = downloadAndInstallUpdate();
    expect(get(updateBusy)).toBe(true);
    expect(installUpdate).toHaveBeenCalledWith('1.2.0', expect.anything());
    channel.onmessage({ phase: 'downloading', received: 20, total: null });
    expect(get(updateProgress)).toEqual({ phase: 'downloading', received: 20, total: null });
    await downloadAndInstallUpdate();
    dismissUpdate();
    offerUpdate({ ...info, version: '1.3.0' });
    expect(installUpdate).toHaveBeenCalledTimes(1);
    expect(get(availableUpdate)).toEqual(info);
    finish();
    await pending;
    expect(get(updateProgress)).toEqual({ phase: 'restarting' });
  });

  it('permits retry after download or signature failure', async () => {
    offerUpdate(info);
    vi.mocked(installUpdate).mockRejectedValueOnce(new Error('Invalid signature')).mockResolvedValueOnce();
    await downloadAndInstallUpdate();
    expect(get(updateError)).toBe('Invalid signature');
    expect(get(updateBusy)).toBe(false);
    expect(get(availableUpdate)).toEqual(info);
    await downloadAndInstallUpdate();
    expect(get(updateError)).toBeNull();
    expect(installUpdate).toHaveBeenCalledTimes(2);
  });
});
