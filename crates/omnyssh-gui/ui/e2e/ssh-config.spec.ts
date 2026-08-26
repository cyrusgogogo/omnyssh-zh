import { expect, test, type Page } from '@playwright/test';

async function boot(page: Page): Promise<void> {
  await page.addInitScript(() => {
    let cbid = 0;
    const win = window as unknown as Record<string, unknown>;
    const managed = [{ alias: 'prod', hostname: 'prod.example.com', user: 'deploy', port: 22, identityFile: null, proxyJump: null }];
    const snapshot = () => ({
      mainPath: '/tmp/home/.ssh/config', managedPath: '/tmp/home/.ssh/omnyssh.conf',
      installed: false, writable: true, mainHash: 'missing', managedHash: 'missing',
      hosts: managed,
      sources: [
        { path: '/tmp/home/.ssh/config', content: 'Host legacy\n  HostName legacy.example.com\n', readOnly: true },
        { path: '/tmp/home/.ssh/omnyssh.conf', content: 'Host prod\n  HostName prod.example.com\n', readOnly: false }
      ], diagnostics: [], backups: []
    });
    const keyRecords: Array<Record<string, unknown>> = [];
    let keyDeleted = false;
    const discovered = [{
        privatePath: '/tmp/home/.ssh/id_existing',
        publicPath: '/tmp/home/.ssh/id_existing.pub',
        suggestedName: 'id_existing',
        keyType: 'ed25519'
      }];
    const keySnapshot = () => ({
      records: keyRecords.map((record) => ({ ...record })),
      discovered: keyRecords.length || keyDeleted ? [] : discovered,
      backups: []
    });
    (win as { __TAURI_INTERNALS__: unknown }).__TAURI_INTERNALS__ = {
      invoke: (cmd: string, args?: Record<string, unknown>) => {
        if (cmd === 'list_hosts') return Promise.resolve([]);
        if (cmd === 'get_ssh_config_snapshot') return Promise.resolve(snapshot());
        if (cmd === 'get_ssh_key_snapshot') return Promise.resolve(keySnapshot());
        if (cmd === 'import_ssh_key') {
          const record = {
            id: 'key-1', name: args?.name, privatePath: args?.privatePath,
            publicPath: `${String(args?.privatePath)}.pub`, keyType: 'ed25519', available: true
          };
          keyRecords.push(record);
          return Promise.resolve(record);
        }
        if (cmd === 'delete_ssh_key') {
          if (args?.confirmation !== keyRecords[0]?.name) return Promise.reject({ code: 'ssh-keys', args: {}, rawDetail: 'confirmation mismatch' });
          keyRecords.splice(0, 1);
          keyDeleted = true;
          return Promise.resolve('backup-1');
        }
        if (cmd === 'preview_ssh_config') {
          win.__lastPreviewHosts = args?.hosts;
          return Promise.resolve({ mainHash: 'missing', managedHash: 'missing', mainDiff: '+ Include ~/.ssh/omnyssh.conf\n', managedDiff: '+ Host prod\n', warnings: [], canApply: true });
        }
        if (cmd === 'apply_ssh_config') {
          win.__lastAppliedHosts = args?.hosts;
          return Promise.resolve({ backupId: 'backup-1', sshValidation: 'OpenSSH test' });
        }
        if (cmd === 'reload_hosts') return Promise.resolve(null);
        if (cmd === 'plugin:event|listen') return Promise.resolve(cbid);
        return Promise.resolve(null);
      },
      transformCallback: (cb: unknown) => { const id = ++cbid; win[`__cb${id}`] = cb; return id; }
    };
  });
  await page.goto('/');
}

test('previews managed SSH config without exposing a password value', async ({ page }) => {
  await boot(page);
  await page.getByRole('button', { name: 'SSH Config' }).click();
  await expect(page.getByText('prod', { exact: true })).toBeVisible();
  if (process.env.OMNYSSH_CAPTURE_SCREENSHOT) {
    await page.screenshot({ path: '../../../assets/screenshots/ssh-config.png', fullPage: true });
  }

  await expect(page.getByText(/secret|password123/i)).toHaveCount(0);
  await page.getByRole('button', { name: 'Preview changes' }).click();
  const dialog = page.getByRole('dialog', { name: 'Review SSH config changes' });
  await expect(dialog).toContainText('Include ~/.ssh/omnyssh.conf');
  await expect(dialog).toContainText('Host prod');
});

test('localizes the Chinese SSH Config menu and editor fields', async ({ page }) => {
  await page.addInitScript(() => localStorage.setItem('omnyssh-language', 'zh-CN'));
  await boot(page);
  await page.getByRole('button', { name: 'SSH配置' }).click();
  await page.getByRole('button', { name: '新增 SSH Config 主机' }).click();
  const editor = page.getByRole('dialog', { name: '用户 SSH Config 主机' });
  await expect(editor.getByLabel('别名')).toBeVisible();
  await expect(editor.getByLabel('主机名')).toBeVisible();
  await expect(editor.getByLabel('用户')).toBeVisible();
  await expect(editor.getByLabel('端口')).toBeVisible();
  await expect(editor.getByLabel('身份文件')).toBeVisible();
  await expect(editor.getByLabel('跳板机（ProxyJump）')).toBeVisible();
  await expect(editor.getByLabel('加密算法（Ciphers）')).toBeVisible();
});

test('loads existing key metadata and uses managed keys as IdentityFile choices', async ({ page }) => {
  await boot(page);
  await page.getByRole('button', { name: 'SSH Config' }).click();
  await page.getByRole('button', { name: 'Key management' }).click();
  await expect(page.getByText('id_existing', { exact: true })).toBeVisible();
  await page.getByRole('button', { name: 'Load' }).click();
  const loadDialog = page.getByRole('dialog', { name: 'Load existing key' });
  await expect(loadDialog).toContainText('/tmp/home/.ssh/id_existing');
  await loadDialog.getByRole('button', { name: 'Save' }).click();
  await expect(page.getByText('/tmp/home/.ssh/id_existing', { exact: true })).toBeVisible();

  await page.getByRole('button', { name: 'Delete' }).click();
  const deleteDialog = page.getByRole('dialog', { name: 'Delete SSH key' });
  const confirmDelete = deleteDialog.getByRole('button', { name: 'Delete' });
  await expect(confirmDelete).toBeDisabled();
  await deleteDialog.getByLabel('Type “id_existing” to confirm deletion').fill('wrong');
  await expect(confirmDelete).toBeDisabled();
  await deleteDialog.getByLabel('Type “id_existing” to confirm deletion').fill('id_existing');
  await expect(confirmDelete).toBeEnabled();
  await confirmDelete.click();
  await expect(page.getByText('/tmp/home/.ssh/id_existing', { exact: true })).toHaveCount(0);
});

test('shows both SSH config sources and applies a newly added host', async ({ page }) => {
  await boot(page);
  const sshConfigEntry = page.getByRole('button', { name: 'SSH Config' });
  const settingsEntry = page.getByRole('button', { name: 'Settings' });
  await expect.poll(async () => sshConfigEntry.locator('svg').innerHTML()).not.toBe(await settingsEntry.locator('svg').innerHTML());
  await sshConfigEntry.click();

  await expect(page.getByRole('button', { name: 'User SSH Config' })).toBeVisible();
  await expect(page.getByRole('button', { name: 'Sync from hosts.toml' })).toHaveCount(0);
  await page.getByRole('button', { name: 'Configuration sources' }).click();
  await expect(page.getByText('Host legacy', { exact: false })).toBeVisible();
  await expect(page.getByText('Host prod', { exact: false })).toBeVisible();

  await page.getByRole('button', { name: 'User SSH Config' }).click();
  await page.getByRole('button', { name: 'Add SSH Config host' }).click();
  const editor = page.getByRole('dialog', { name: 'User SSH Config host' });
  await expect(editor).toContainText('written to ~/.ssh/omnyssh.conf');
  await editor.getByLabel('Alias').fill('new-host');
  await editor.getByLabel('HostName').fill('new.example.com');
  await editor.getByRole('button', { name: 'Save' }).click();

  const review = page.getByRole('dialog', { name: 'Review SSH config changes' });
  await expect(review).toBeVisible();
  await review.getByRole('button', { name: 'Confirm and write' }).click();
  await expect.poll(() => page.evaluate(() => (window as unknown as { __lastAppliedHosts?: Array<{ alias: string }> }).__lastAppliedHosts?.some((host) => host.alias === 'new-host'))).toBe(true);
});

test('saving a newly added host immediately opens the safe apply review', async ({ page }) => {
  await boot(page);
  await page.getByRole('button', { name: 'SSH Config' }).click();
  await page.getByRole('button', { name: 'Add SSH Config host' }).click();
  const editor = page.getByRole('dialog', { name: 'User SSH Config host' });
  await editor.getByLabel('Alias').fill('new-host');
  await editor.getByLabel('HostName').fill('new.example.com');
  await editor.getByRole('button', { name: 'Save' }).click();

  await expect(page.getByRole('dialog', { name: 'Review SSH config changes' })).toBeVisible();
});

test('deleting a managed host immediately opens a preview without that host', async ({ page }) => {
  await boot(page);
  await page.getByRole('button', { name: 'SSH Config' }).click();
  await page.getByRole('button', { name: 'Delete' }).click();

  await expect(page.getByRole('dialog', { name: 'Review SSH config changes' })).toBeVisible();
  await expect
    .poll(() =>
      page.evaluate(() =>
        (window as unknown as { __lastPreviewHosts?: Array<{ alias: string }> })
          .__lastPreviewHosts?.some((host) => host.alias === 'prod')
      )
    )
    .toBe(false);
});
