import { expect, test, type Page } from '@playwright/test';

// SFTP dual-pane vertical (tech-gui.md §3.2). e2e runs against the static SPA with
// Tauri absent, so we stub `__TAURI_INTERNALS__` at the boundary (§6.4). The stub owns
// an in-memory local + remote filesystem: `list_local_dir` returns directly, `sftp_*`
// commands fire the stamped `sftp-*` events the per-session forwarder would emit, and a
// transfer holds at a progress tick until `__completeTransfer()` fires its op-done — so
// the live progress bar is deterministically observable. Both spawn paths (a card's
// `files`, and the SFTP spawner via the host picker) are load-bearing for the stage.
const HOSTS = [
  { name: 'web-1', hostname: 'web-1.example.com', user: 'deploy', port: 22, tags: ['prod'], source: 'manual', hasKey: true },
  { name: 'db-1', hostname: 'db-1.example.com', user: 'root', port: 22, tags: [], source: 'manual', hasKey: false }
];

async function boot(page: Page, path = '/'): Promise<void> {
  await page.addInitScript(
    ({ hosts }) => {
      let cbid = 0;
      const win = window as unknown as Record<string, unknown>;
      const listeners: Record<string, number[]> = {};
      let nextSession = 0;
      let nextTransfer = 0;
      let preferences = { closeBehavior: 'tray', trayClickBehavior: 'openMain', shortcut: '', shortcutRegistered: false, shortcutError: null, sftpOpenMode: 'main', sftpCardCloseOnBlur: true };
      win.__cardClosed = false;
      // A pending transfer holds until the test fires its op-done, so the progress bar
      // is observable mid-flight (the core is sequential — one transfer at a time).
      const completions: Array<() => void> = [];

      type Entry = { name: string; path: string; size: number; isDir: boolean; modified?: number };
      const local: Record<string, Entry[]> = {
        '/home/user': [
          { name: 'notes.txt', path: '/home/user/notes.txt', size: 24, isDir: false, modified: 1_700_000_000 },
          { name: 'work', path: '/home/user/work', size: 0, isDir: true, modified: 1_700_000_100 },
          { name: 'a.log', path: '/home/user/a.log', size: 8, isDir: false, modified: 1_700_000_200 }
        ]
      };
      const remote: Record<string, Entry[]> = {
        '/': [
          { name: 'config.yml', path: '/config.yml', size: 64, isDir: false },
          { name: 'var', path: '/var', size: 0, isDir: true }
        ]
      };

      function parentOf(p: string): string {
        const i = p.lastIndexOf('/');
        return i <= 0 ? '/' : p.slice(0, i);
      }
      function baseName(p: string): string {
        return p.slice(p.lastIndexOf('/') + 1);
      }
      function withParent(path: string, entries: Entry[]): Entry[] {
        if (path === '/') return entries;
        return [{ name: '..', path: parentOf(path), size: 0, isDir: true }, ...entries];
      }
      function addFile(fs: Record<string, Entry[]>, dir: string, name: string): void {
        const list = (fs[dir] ||= []);
        if (!list.some((e) => e.name === name)) {
          list.push({ name, path: dir === '/' ? `/${name}` : `${dir}/${name}`, size: 8, isDir: false });
        }
      }

      function fireEvent(event: string, payload: unknown): void {
        for (const id of listeners[event] ?? []) {
          const cb = win[`__cb${id}`] as ((e: unknown) => void) | undefined;
          cb?.({ event, id, payload });
        }
      }

      // Fire the oldest still-pending transfer's op-done (deterministic completion).
      (win as { __completeTransfer?: () => void }).__completeTransfer = () => completions.shift()?.();

      (win as { __TAURI_INTERNALS__: unknown }).__TAURI_INTERNALS__ = {
        invoke: (cmd: string, args: Record<string, unknown>) => {
          if (cmd.startsWith('plugin:path')) return Promise.resolve('/home/user');
          switch (cmd) {
            case 'load_desktop_settings':
              return Promise.resolve(preferences);
            case 'save_desktop_settings':
              preferences = { ...preferences, ...(args.settings as typeof preferences) };
              fireEvent('desktop-settings-changed', preferences);
              return Promise.resolve(preferences);
            case 'open_sftp_card':
              win.__openedSftpCard = args.hostName;
              return Promise.resolve(null);
            case 'close_sftp_card':
              win.__cardClosed = true;
              return Promise.resolve(null);
            case 'ready_sftp_card':
              win.__sftpCardReady = true;
              return Promise.resolve(null);
            case 'list_hosts':
              return Promise.resolve(hosts);
            case 'reload_hosts':
              return Promise.resolve(null);
            case 'list_local_dir': {
              const path = args.path as string;
              return Promise.resolve(withParent(path, local[path] ?? []));
            }
            case 'sftp_open': {
              const sid = ++nextSession;
              setTimeout(() => fireEvent('sftp-connected', { sessionId: sid, hostName: args.hostName }), 0);
              return Promise.resolve(sid);
            }
            case 'sftp_list': {
              const { sessionId, path } = args as { sessionId: number; path: string };
              setTimeout(
                () => fireEvent('sftp-dir-listed', { sessionId, path, entries: withParent(path, remote[path] ?? []) }),
                0
              );
              return Promise.resolve(null);
            }
            case 'sftp_upload': {
              const { sessionId, remote: dest } = args as { sessionId: number; remote: string };
              const tid = ++nextTransfer;
              setTimeout(() => fireEvent('transfer-progress', { sessionId, transferId: tid, done: 4, total: 8 }), 0);
              completions.push(() => {
                addFile(remote, parentOf(dest), baseName(dest));
                fireEvent('sftp-op-done', { sessionId, ok: true });
              });
              return Promise.resolve(null);
            }
            case 'sftp_download': {
              const { sessionId, local: dest } = args as { sessionId: number; local: string };
              const tid = ++nextTransfer;
              setTimeout(() => fireEvent('transfer-progress', { sessionId, transferId: tid, done: 2, total: 8 }), 0);
              completions.push(() => {
                addFile(local, parentOf(dest), baseName(dest));
                fireEvent('sftp-op-done', { sessionId, ok: true });
              });
              return Promise.resolve(null);
            }
            case 'sftp_close':
              return Promise.resolve(null);
            case 'plugin:event|listen': {
              const { event, handler } = args as { event: string; handler: number };
              (listeners[event] ||= []).push(handler);
              return Promise.resolve(cbid);
            }
            default:
              return Promise.resolve(null);
          }
        },
        transformCallback: (cb: unknown) => {
          const id = ++cbid;
          win[`__cb${id}`] = cb;
          return id;
        },
        unregisterCallback: (id: number) => {
          delete win[`__cb${id}`];
        }
      };
    },
    { hosts: HOSTS }
  );

  await page.goto(path);
  if (path === '/') await expect(page.getByText('2 hosts')).toBeVisible();
}

test('host-first: a card’s files opens SFTP and browses both sides', async ({ page }) => {
  await boot(page);

  // Host-first spawn — a Dashboard card's `files`, no picker (tech-gui.md §2, §3.2).
  await page.getByTitle('files on web-1').click();

  await expect(page.getByRole('button', { name: 'web-1 · sftp', exact: true })).toBeVisible();
  const localPane = page.getByRole('region', { name: 'Local' });
  const remotePane = page.getByRole('region', { name: 'web-1' });

  // Both panes list their own filesystem, from distinct commands (local direct, remote event).
  await expect(localPane.getByText('notes.txt')).toBeVisible();
  await expect(remotePane.getByText('config.yml')).toBeVisible();
});

test('round-trip: upload a local file to the remote, then download a remote file', async ({
  page
}) => {
  await boot(page);
  await page.getByTitle('files on web-1').click();

  const localPane = page.getByRole('region', { name: 'Local' });
  const remotePane = page.getByRole('region', { name: 'web-1' });
  await expect(localPane.getByText('notes.txt')).toBeVisible();
  await expect(remotePane.getByText('config.yml')).toBeVisible();

  // Upload: mark the local file, click Upload — the live progress bar shows mid-flight.
  await localPane.getByRole('checkbox', { name: 'Mark notes.txt' }).click();
  await page.getByRole('button', { name: 'Upload' }).click();
  await expect(page.getByLabel('transfer progress')).toBeVisible();

  // Complete it: op-done drains the batch, the remote pane re-lists with the new file.
  await page.evaluate(() => (window as unknown as { __completeTransfer: () => void }).__completeTransfer());
  await expect(remotePane.getByText('notes.txt')).toBeVisible();
  await expect(page.getByLabel('transfer progress')).toHaveCount(0);

  // Download: mark a remote file, click Download, complete — the local pane re-lists it.
  await remotePane.getByRole('checkbox', { name: 'Mark config.yml' }).click();
  await page.getByRole('button', { name: 'Download' }).click();
  await expect(page.getByLabel('transfer progress')).toBeVisible();
  await page.evaluate(() => (window as unknown as { __completeTransfer: () => void }).__completeTransfer());
  await expect(localPane.getByText('config.yml')).toBeVisible();
});

test('an inactive tab’s modal never overlays another entity (§2 exactly-one-active)', async ({
  page
}) => {
  await boot(page);
  await page.getByTitle('files on web-1').click();
  await expect(page.getByRole('region', { name: 'web-1' })).toBeVisible();
  // Return to the Dashboard (a session hides it) to open a second SFTP session.
  await page.getByRole('button', { name: 'Dashboard', exact: true }).click();
  await page.getByTitle('files on db-1').click();
  await expect(page.getByRole('region', { name: 'db-1' })).toBeVisible();

  // Open db-1's New-folder modal. The modal scrim traps the sidebar, so the ⌘K
  // navigator is the reachable way to switch entity while a modal is open.
  await page.getByRole('button', { name: 'Folder', exact: true }).click();
  await expect(page.getByRole('dialog', { name: 'New folder' })).toBeVisible();
  await page.keyboard.press('Control+k');
  const palette = page.getByRole('dialog', { name: 'Command palette' });
  await palette.getByRole('textbox').fill('web-1');
  await page.keyboard.press('Enter');

  // Activating the web-1 session must fully hide db-1's modal — never two at once.
  await expect(page.getByRole('region', { name: 'web-1' })).toBeVisible();
  await expect(page.getByRole('dialog', { name: 'New folder' })).toHaveCount(0);
});

test('action-first: the SFTP spawner opens the host picker, then a live session', async ({
  page
}) => {
  await boot(page);

  await page.getByRole('button', { name: 'SFTP', exact: true }).click();
  await page.getByRole('dialog').getByText('web-1', { exact: true }).click();

  await expect(page.getByRole('button', { name: 'web-1 · sftp', exact: true })).toBeVisible();
  await expect(page.getByRole('region', { name: 'web-1' }).getByText('config.yml')).toBeVisible();
});

test('file metadata, sorting, marks and remembered pane preferences', async ({ page }) => {
  await boot(page);
  await page.getByTitle('files on web-1').click();
  const local = page.getByRole('region', { name: 'Local' });
  const remote = page.getByRole('region', { name: 'web-1' });
  await expect(local.getByText('TXT file')).toBeVisible();
  await expect(local.locator('button[title="notes.txt"]')).toContainText('2023');
  await local.getByRole('checkbox', { name: 'Mark notes.txt' }).click();
  await local.getByRole('combobox', { name: 'Sort by' }).selectOption('size');
  await local.getByRole('button', { name: 'Sort descending' }).click();
  const rows = local.locator('ul > li > button[title]');
  await expect(rows.nth(0)).toHaveAttribute('title', '..');
  await expect(rows.nth(1)).toHaveAttribute('title', 'work');
  await expect(rows.nth(2)).toHaveAttribute('title', 'notes.txt');
  await expect(local.getByRole('checkbox', { name: 'Mark notes.txt' })).toHaveAttribute('aria-checked', 'true');
  await expect(remote.getByRole('combobox', { name: 'Sort by' })).toHaveValue('name');
  await page.getByRole('button', { name: 'Refresh local files' }).click();
  await expect(rows.nth(2)).toHaveAttribute('title', 'notes.txt');
  await page.reload();
  await page.getByTitle('files on web-1').click();
  await expect(local.getByRole('combobox', { name: 'Sort by' })).toHaveValue('size');
  await expect(local.getByRole('button', { name: 'Sort descending' })).toHaveAttribute('aria-pressed', 'true');
});

test('card preference opens SFTP without adding a main-window session', async ({ page }) => {
  await boot(page);
  await page.getByRole('button', { name: 'Settings', exact: true }).click();
  await page.getByRole('button', { name: 'Separate card', exact: true }).click();
  await expect(page.getByRole('button', { name: 'Separate card', exact: true })).toHaveAttribute('aria-pressed', 'true');
  await page.getByRole('button', { name: 'Dashboard', exact: true }).click();
  await page.getByTitle('files on web-1').click();
  await expect.poll(() => page.evaluate(() => (window as any).__openedSftpCard)).toBe('web-1');
  await expect(page.getByRole('button', { name: 'web-1 · sftp', exact: true })).toHaveCount(0);
});

test('standalone card browses both panes and Escape closes dialogs before the card', async ({ page }) => {
  await page.setViewportSize({ width: 720, height: 480 });
  await boot(page, '/?view=sftp-card&host=web-1');
  await expect(page.getByRole('main', { name: 'SFTP card' })).toBeVisible();
  await expect.poll(() => page.evaluate(() => (window as unknown as { __sftpCardReady?: boolean }).__sftpCardReady)).toBe(true);
  await expect(page.getByRole('region', { name: 'Local' }).getByText('notes.txt')).toBeVisible();
  await expect(page.getByRole('region', { name: 'web-1' }).getByText('config.yml')).toBeVisible();
  await expect(page.getByRole('button', { name: 'Dashboard', exact: true })).toHaveCount(0);
  await page.getByRole('button', { name: 'Folder', exact: true }).click();
  await expect(page.getByRole('dialog', { name: 'New folder' })).toBeVisible();
  await page.keyboard.press('Escape');
  await expect(page.getByRole('dialog')).toHaveCount(0);
  expect(await page.evaluate(() => (window as any).__cardClosed)).toBe(false);
  await page.keyboard.press('Escape');
  await expect.poll(() => page.evaluate(() => (window as any).__cardClosed)).toBe(true);
});
