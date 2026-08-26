import { expect, test, type Page } from '@playwright/test';

// Host management CRUD (tech-gui.md §4.1). e2e runs against the static SPA; Tauri is
// absent, so we stub `__TAURI_INTERNALS__` at the boundary (§6.4). The stub is
// stateful: save/delete mutate an in-memory list, and `reload_hosts` replays it as a
// `hosts-loaded` event through the same listener the app registers — so a save/delete
// round-trips into the dashboard grid exactly as the real backend would drive it.
const HOSTS = [
  { name: 'web-1', hostname: 'web-1.example.com', user: 'deploy', port: 22, tags: ['prod'], source: 'manual', hasKey: true },
  { name: 'imported', hostname: 'imported.example.com', user: 'root', port: 22, tags: [], source: 'sshConfig', hasKey: false }
];

async function boot(page: Page): Promise<void> {
  await page.addInitScript(
    ({ hosts }) => {
      let cbid = 0;
      const listeners: Record<string, number[]> = {};
      const state: { hosts: Array<Record<string, unknown>> } = { hosts: hosts.map((h) => ({ ...h })) };
      const win = window as unknown as Record<string, unknown>;

      function fire(event: string, payload: unknown): void {
        for (const id of listeners[event] ?? []) {
          const cb = win[`__cb${id}`] as ((e: unknown) => void) | undefined;
          cb?.({ event, id, payload });
        }
      }

      (win as { __TAURI_INTERNALS__: unknown }).__TAURI_INTERNALS__ = {
        invoke: (cmd: string, args: Record<string, unknown>) => {
          switch (cmd) {
            case 'list_hosts':
              return Promise.resolve([...state.hosts]);
            case 'reload_hosts':
              // The real command reloads + restarts pollers, then broadcasts the list.
              setTimeout(() => fire('hosts-loaded', [...state.hosts]), 0);
              return Promise.resolve(null);
            case 'save_host': {
              // Upsert by name as a manual host; the outbound view (HostDto) omits the
              // secret fields the input carried, mirroring the backend map (§3.4).
              const h = args.input as Record<string, unknown> & { name: string; identityFile?: string };
              const view = {
                name: h.name,
                hostname: h.hostname,
                user: h.user,
                port: h.port,
                tags: (h.tags as string[]) ?? [],
                notes: h.notes,
                source: 'manual',
                hasKey: !!h.identityFile
              };
              const i = state.hosts.findIndex((x) => (x as { name: string }).name === view.name);
              if (i >= 0) state.hosts[i] = { ...state.hosts[i], ...view };
              else state.hosts.push(view);
              return Promise.resolve(null);
            }
            case 'delete_host':
              state.hosts = state.hosts.filter((x) => (x as { name: string }).name !== args.name);
              return Promise.resolve(null);
            case 'set_desktop_card_always_on_top':
              return Promise.resolve(args.alwaysOnTop);
            case 'open_desktop_card_host':
              win.__desktopCardAction = { ...args };
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
        }
      };
    },
    { hosts: HOSTS }
  );
  await page.goto('/');
  // The dashboard is the default screen; the seeded cards confirm the app booted.
  await expect(page.getByText('web-1', { exact: true })).toBeVisible();
}

test('adds a host and it appears as a card', async ({ page }) => {
  await boot(page);

  await page.getByRole('button', { name: 'Add OmnySSH host' }).click();
  const editor = page.getByRole('dialog', { name: 'Add OmnySSH host' });
  await expect(editor).toBeVisible();
  await expect(editor).toContainText('Saved in OmnySSH hosts.toml');

  await editor.getByLabel('Name', { exact: true }).fill('db-1');
  await editor.getByLabel('Hostname / IP').fill('db-1.example.com');
  await editor.getByLabel('User').fill('postgres');
  await editor.getByRole('button', { name: 'Add OmnySSH host' }).click();

  await expect(page.getByRole('dialog')).toHaveCount(0);
  await expect(page.getByText('db-1', { exact: true })).toBeVisible();
  await expect(page.getByText('postgres@db-1.example.com:22')).toBeVisible();
});

test('labels both dashboard host configuration sources', async ({ page }) => {
  await boot(page);

  await expect(page.getByText('OmnySSH config', { exact: true })).toBeVisible();
  await expect(page.getByTitle(/Imported from ~\/\.ssh\/config/)).toHaveText('SSH Config');
});

test('desktop card keeps several hosts but displays one at a time', async ({ page }) => {
  await boot(page);

  await page.getByRole('button', { name: 'Add web-1 to the desktop card' }).click();
  await page.getByRole('button', { name: 'Add imported to the desktop card' }).click();
  await expect(page.getByRole('button', { name: 'Desktop card (2)' })).toBeVisible();

  await page.goto('/?view=desktop-card');
  await expect(page.getByText('Desktop card', { exact: true })).toHaveCount(0);
  await expect(page.getByText('web-1', { exact: true })).toBeVisible();
  await expect(page.getByText('imported', { exact: true })).toHaveCount(0);
  await expect(page.getByText('OmnySSH config', { exact: true })).toHaveCount(0);
  await expect(page.getByRole('button', { name: 'Remove web-1 from the desktop card' })).toHaveCount(0);
  await expect(page.locator('[data-tauri-drag-region="deep"]')).toHaveCount(1);
  const cardFrame = page.locator('main > section');
  await expect
    .poll(() => cardFrame.evaluate((element) => getComputedStyle(element).boxShadow))
    .toBe('none');
  await expect
    .poll(() => page.locator('main').evaluate((element) => getComputedStyle(element).padding))
    .toBe('0px');
  const switcher = page.getByRole('navigation', { name: 'Hosts' });
  await expect(switcher).toBeVisible();
  await expect(switcher.getByRole('button')).toHaveCount(2);
  await expect(switcher.getByRole('button', { name: 'Switch to web-1' })).toHaveAttribute(
    'aria-current',
    'true'
  );
  await expect(switcher.locator('span').first()).toHaveAttribute(
    'style',
    /background-color: var\(--text-faint\)/
  );

  const pin = page.getByRole('button', { name: 'Stop keeping the desktop card on top' });
  await expect(pin).toHaveAttribute('aria-pressed', 'true');
  await expect(pin).toHaveClass(/bg-accent/);
  await pin.click();
  await expect(page.getByRole('button', { name: 'Keep the desktop card on top' })).toHaveAttribute(
    'aria-pressed',
    'false'
  );

  await page.getByRole('button', { name: 'Open a terminal for web-1' }).click();
  await expect
    .poll(() =>
      page.evaluate(() =>
        (window as unknown as { __desktopCardAction?: unknown }).__desktopCardAction
      )
    )
    .toEqual({ hostName: 'web-1', kind: 'terminal' });

  await switcher.getByRole('button', { name: 'Switch to imported' }).click();
  await expect(page.getByText('imported', { exact: true })).toBeVisible();
  await expect(page.getByText('web-1', { exact: true })).toHaveCount(0);
  await expect(switcher.getByRole('button', { name: 'Switch to imported' })).toHaveAttribute(
    'aria-current',
    'true'
  );
  await expect(page.getByText('SSH Config', { exact: true })).toHaveCount(0);
  await page.getByRole('button', { name: 'Open SFTP for imported' }).click();
  await expect
    .poll(() =>
      page.evaluate(() =>
        (window as unknown as { __desktopCardAction?: unknown }).__desktopCardAction
      )
    )
    .toEqual({ hostName: 'imported', kind: 'sftp' });
});

test('scrollbars stay hidden until their pane is scrolling', async ({ page }) => {
  await boot(page);
  const scroller = page.locator('.h-full.overflow-auto.overscroll-contain');

  await expect
    .poll(() =>
      scroller.evaluate(
        (element) => getComputedStyle(element, '::-webkit-scrollbar-thumb').backgroundColor
      )
    )
    .toBe('rgba(0, 0, 0, 0)');

  await scroller.dispatchEvent('scroll');
  await expect(scroller).toHaveClass(/scrollbar-active/);
  await expect
    .poll(() =>
      scroller.evaluate(
        (element) => getComputedStyle(element, '::-webkit-scrollbar-thumb').backgroundColor
      )
    )
    .not.toBe('rgba(0, 0, 0, 0)');
  await expect(scroller).not.toHaveClass(/scrollbar-active/, { timeout: 2_000 });
});

test('edits a manual host in place', async ({ page }) => {
  await boot(page);

  await page.getByRole('button', { name: 'Edit web-1' }).click();
  const editor = page.getByRole('dialog', { name: 'Edit host' });
  await expect(editor).toBeVisible();

  // The name is the on-disk key: fixed on edit.
  await expect(editor.getByLabel(/^Name/)).toHaveAttribute('readonly', '');

  const hostname = editor.getByLabel('Hostname / IP');
  await hostname.fill('web-1b.example.com');
  await editor.getByRole('button', { name: 'Save' }).click();

  await expect(page.getByRole('dialog')).toHaveCount(0);
  await expect(page.getByText('deploy@web-1b.example.com:22')).toBeVisible();
});

test('deletes a manual host after confirmation', async ({ page }) => {
  await boot(page);
  await expect(page.getByText('web-1', { exact: true })).toBeVisible();

  await page.getByRole('button', { name: 'Delete web-1' }).click();
  const confirm = page.getByRole('dialog', { name: 'Delete host' });
  await expect(confirm).toBeVisible();
  await confirm.getByRole('button', { name: 'Delete', exact: true }).click();

  await expect(page.getByText('web-1', { exact: true })).toHaveCount(0);
});

test('an SSH-config host is adopted by editing it', async ({ page }) => {
  await boot(page);

  // The import is marked, and there is nothing here to delete: it lives in
  // ~/.ssh/config, which this app never writes.
  await expect(page.getByTitle(/Imported from ~\/\.ssh\/config/)).toHaveText('SSH Config');
  await expect(page.getByRole('button', { name: 'Delete imported' })).toHaveCount(0);

  await page.getByRole('button', { name: 'Edit imported' }).click();
  const editor = page.getByRole('dialog', { name: 'Edit host' });
  await expect(editor).toBeVisible();
  // The form states what saving does, since the SSH config file itself does not change.
  await expect(editor.getByText(/never written/)).toBeVisible();

  await editor.getByLabel('Hostname / IP').fill('adopted.example.com');
  await editor.getByRole('button', { name: 'Save' }).click();

  // Saved as a manual copy: the import badge is gone and delete is now offered.
  await expect(page.getByRole('dialog')).toHaveCount(0);
  await expect(page.getByText('root@adopted.example.com:22')).toBeVisible();
  await expect(page.getByTitle(/Imported from ~\/\.ssh\/config/)).toHaveCount(0);
  await expect(page.getByRole('button', { name: 'Delete imported' })).toHaveCount(1);
});

test('rejects a new host whose name already exists', async ({ page }) => {
  await boot(page);

  await page.getByRole('button', { name: 'Add OmnySSH host' }).click();
  const editor = page.getByRole('dialog', { name: 'Add OmnySSH host' });
  await editor.getByLabel('Name', { exact: true }).fill('web-1');
  await editor.getByLabel('Hostname / IP').fill('dupe.example.com');
  await editor.getByRole('button', { name: 'Add OmnySSH host' }).click();

  // The editor stays open with an inline error rather than clobbering the existing host.
  await expect(editor).toBeVisible();
  await expect(editor.getByText(/A host named .*web-1.* already exists/)).toBeVisible();
});
