<script lang="ts">
  import { onMount } from 'svelte';
  import type {
    ManagedSshHostDto,
    SshConfigPreviewDto,
    SshConfigSnapshotDto,
    SshKeyRecordDto,
    SshKeySnapshotDto
  } from '$lib/bindings';
  import {
    applySshConfig,
    backupSshKey,
    createSshKey,
    deleteSshKey,
    getSshConfigSnapshot,
    getSshKeySnapshot,
    importSshKey,
    previewSshConfig,
    previewSshConfigRestore,
    readSshPublicKey,
    reloadHosts,
    renameSshKey,
    restoreSshKey,
    restoreSshConfig
  } from '$lib/ipc/commands';
  import { t } from '$lib/i18n';
  import { lastError } from '$lib/stores/notifications';
  import { Button, Surface } from '$lib/theme';
  import Modal from '$lib/components/Modal.svelte';
  import Select from '$lib/components/Select.svelte';
  import { validateManagedHost } from './sshConfigForm';
  import { randomKeyFileName } from './sshKeyForm';

  type Tab = 'managed' | 'keys' | 'sources' | 'backups';
  type KeyDialog =
    | { kind: 'create' }
    | { kind: 'import'; privatePath: string; suggestedName: string }
    | { kind: 'rename'; record: SshKeyRecordDto }
    | { kind: 'delete'; record: SshKeyRecordDto };
  let tab: Tab = $state('managed');
  let snapshot: SshConfigSnapshotDto | null = $state(null);
  let keySnapshot: SshKeySnapshotDto = $state({ records: [], discovered: [], backups: [] });
  let hosts: ManagedSshHostDto[] = $state([]);
  let search = $state('');
  let loading = $state(true);
  let saving = $state(false);
  let editor: ManagedSshHostDto | null = $state(null);
  let editorIdentity = $state('');
  let editorOriginal: string | null = $state(null);
  let preview: SshConfigPreviewDto | null = $state(null);
  let restoreId: string | null = $state(null);
  let dirty = $state(false);
  let stale = $state(false);
  let keyDialog: KeyDialog | null = $state(null);
  let keyName = $state('');
  let keyFileStem = $state(randomKeyFileName());
  let keyType = $state('ed25519');
  let deleteConfirmation = $state('');
  let keyBusy = $state(false);
  let copiedPath = $state('');

  const visibleHosts = $derived(
    hosts.filter((host) =>
      `${host.alias} ${host.hostname} ${host.user}`.toLowerCase().includes(search.toLowerCase())
    )
  );

  async function load(): Promise<void> {
    loading = true;
    try {
      [snapshot, keySnapshot] = await Promise.all([
        getSshConfigSnapshot(),
        getSshKeySnapshot()
      ]);
      hosts = snapshot.hosts.map((host) => ({ ...host }));
      preview = null;
      restoreId = null;
      dirty = false;
      stale = false;
    } catch (error) {
      lastError.set(error instanceof Error ? error.message : String(error));
    } finally {
      loading = false;
    }
  }

  async function refreshKeys(): Promise<void> {
    keySnapshot = await getSshKeySnapshot();
  }

  function openKeyDialog(dialog: KeyDialog): void {
    keyDialog = dialog;
    deleteConfirmation = '';
    if (dialog.kind === 'create') {
      keyName = '';
      keyFileStem = randomKeyFileName();
      keyType = 'ed25519';
    } else if (dialog.kind === 'import') {
      keyName = dialog.suggestedName;
    } else {
      keyName = dialog.record.name;
    }
  }

  async function submitKeyDialog(): Promise<void> {
    if (!keyDialog || !keyName.trim()) return;
    keyBusy = true;
    try {
      if (keyDialog.kind === 'create') {
        await createSshKey(keyName.trim(), keyFileStem.trim(), keyType);
      } else if (keyDialog.kind === 'import') {
        await importSshKey(keyName.trim(), keyDialog.privatePath);
      } else if (keyDialog.kind === 'rename') {
        await renameSshKey(keyDialog.record.id, keyName.trim());
      } else {
        await deleteSshKey(keyDialog.record.id, deleteConfirmation);
      }
      keyDialog = null;
      await refreshKeys();
    } catch (error) {
      lastError.set(error instanceof Error ? error.message : String(error));
    } finally {
      keyBusy = false;
    }
  }

  async function backupKey(id: string): Promise<void> {
    try {
      await backupSshKey(id);
      await refreshKeys();
    } catch (error) {
      lastError.set(error instanceof Error ? error.message : String(error));
    }
  }

  async function restoreKey(id: string): Promise<void> {
    try {
      await restoreSshKey(id);
      await refreshKeys();
    } catch (error) {
      lastError.set(error instanceof Error ? error.message : String(error));
    }
  }

  async function copyPath(value: string): Promise<void> {
    try {
      await navigator.clipboard.writeText(value);
      copiedPath = value;
      window.setTimeout(() => {
        if (copiedPath === value) copiedPath = '';
      }, 1600);
    } catch (error) {
      lastError.set(error instanceof Error ? error.message : String(error));
    }
  }

  async function copyPublicKey(record: SshKeyRecordDto): Promise<void> {
    try {
      await navigator.clipboard.writeText(await readSshPublicKey(record.id));
      copiedPath = record.publicPath;
      window.setTimeout(() => {
        if (copiedPath === record.publicPath) copiedPath = '';
      }, 1600);
    } catch (error) {
      lastError.set(error instanceof Error ? error.message : String(error));
    }
  }

  const fileName = (value: string): string => value.split(/[\\/]/).pop() ?? value;

  function blankHost(): ManagedSshHostDto {
    return { alias: '', hostname: '', user: 'root', port: 22, identityFile: null, proxyJump: null, ciphers: null };
  }

  function openEditor(host?: ManagedSshHostDto): void {
    editorOriginal = host?.alias ?? null;
    editor = host ? { ...host } : blankHost();
    editorIdentity = host?.identityFile ?? '';
  }

  async function saveEditor(): Promise<void> {
    if (!editor) return;
    const copy = {
      ...editor,
      alias: editor.alias.trim(),
      hostname: editor.hostname.trim(),
      user: editor.user.trim(),
      identityFile: editorIdentity || null
    };
    const validationError = validateManagedHost(copy);
    if (validationError) {
      lastError.set(validationError);
      return;
    }
    const nextHosts = editorOriginal
      ? hosts.map((host) => (host.alias === editorOriginal ? copy : host))
      : [...hosts, copy];
    hosts = nextHosts;
    editor = null;
    editorOriginal = null;
    dirty = true;
    await previewHosts(nextHosts);
  }

  async function removeHost(alias: string): Promise<void> {
    const nextHosts = hosts.filter((host) => host.alias !== alias);
    hosts = nextHosts;
    dirty = true;
    await previewHosts(nextHosts);
  }

  async function previewHosts(desired: ManagedSshHostDto[]): Promise<void> {
    try {
      restoreId = null;
      preview = await previewSshConfig(desired);
      stale = false;
    } catch (error) {
      lastError.set(error instanceof Error ? error.message : String(error));
    }
  }

  async function openPreview(): Promise<void> {
    await previewHosts(hosts);
  }

  async function applyChanges(allowMissingSsh = false): Promise<void> {
    if (!preview) return;
    saving = true;
    try {
      await applySshConfig(hosts, preview.mainHash, preview.managedHash, allowMissingSsh);
      await reloadHosts();
      await load();
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error);
      if (!allowMissingSsh && message.includes('explicit confirmation required')) {
        if (window.confirm($t('ssh-config-confirm-without-ssh'))) await applyChanges(true);
      } else {
        lastError.set(message);
      }
    } finally {
      saving = false;
    }
  }

  async function openRestorePreview(id: string): Promise<void> {
    try {
      preview = await previewSshConfigRestore(id);
      restoreId = id;
    } catch (error) {
      lastError.set(error instanceof Error ? error.message : String(error));
    }
  }

  async function confirmRestore(): Promise<void> {
    if (!preview || !restoreId) return;
    saving = true;
    try {
      await restoreSshConfig(restoreId, preview.mainHash, preview.managedHash);
      await reloadHosts();
      await load();
    } catch (error) {
      lastError.set(error instanceof Error ? error.message : String(error));
    } finally {
      saving = false;
    }
  }

  onMount(() => {
    void load();
    const timer = window.setInterval(async () => {
      if (!snapshot || saving) return;
      try {
        const next = await getSshConfigSnapshot();
        if (next.mainHash === snapshot.mainHash && next.managedHash === snapshot.managedHash) return;
        preview = null;
        restoreId = null;
        snapshot = next;
        if (dirty) {
          stale = true;
        } else {
          hosts = next.hosts.map((host) => ({ ...host }));
        }
      } catch {
        // The explicit refresh/save path surfaces errors; background watching is quiet.
      }
    }, 3000);
    return () => window.clearInterval(timer);
  });
</script>

<section class="mx-auto flex w-full max-w-6xl flex-col gap-5 p-6 lg:p-8">
  <header class="flex flex-wrap items-start justify-between gap-4">
    <div>
      <h1 class="text-2xl font-bold text-fg">{$t('ssh-config-title')}</h1>
      <p class="mt-1 text-sm text-muted">{$t('ssh-config-description')}</p>
      {#if snapshot}
        <p class="mt-2 break-all font-mono text-xs text-muted">{snapshot.mainPath}</p>
      {/if}
    </div>
    <div class="flex gap-2">
      <Button onclick={load} disabled={loading}>{$t('common-refresh')}</Button>
      <Button variant="primary" onclick={openPreview} disabled={loading || !snapshot?.writable}>
        {$t('ssh-config-preview-save')}
      </Button>
    </div>
  </header>

  {#if snapshot && !snapshot.installed}
    <div class="rounded-xl border border-warning/40 bg-warning/10 p-4 text-sm text-fg">
      {$t('ssh-config-not-installed')}
    </div>
  {/if}

  {#if stale}
    <div class="rounded-xl border border-warning/40 bg-warning/10 p-4 text-sm text-fg">
      {$t('ssh-config-external-change')}
    </div>
  {/if}

  {#if snapshot?.diagnostics.length}
    <div class="space-y-2">
      {#each snapshot.diagnostics as diagnostic}
        <div class="rounded-lg border border-default bg-surface-inset p-3 text-sm">
          <strong>{diagnostic.severity}</strong> · {diagnostic.message}
          {#if diagnostic.path}<div class="mt-1 break-all font-mono text-xs text-muted">{diagnostic.path}</div>{/if}
        </div>
      {/each}
    </div>
  {/if}

  <div class="flex flex-wrap gap-2 border-b border-default pb-3">
    {#each [['managed', 'ssh-config-tab-managed'], ['keys', 'ssh-config-tab-keys'], ['sources', 'ssh-config-tab-sources'], ['backups', 'ssh-config-tab-backups']] as item}
      <button type="button" class="rounded-full px-4 py-2 text-sm {tab === item[0] ? 'bg-accent text-accent-fg' : 'text-muted hover:bg-surface-inset'}" onclick={() => (tab = item[0] as Tab)}>
        {$t(item[1])}
      </button>
    {/each}
  </div>

  {#if loading}
    <p class="py-16 text-center text-muted">{$t('common-loading')}</p>
  {:else if tab === 'managed'}
    <div class="flex gap-3">
      <input class="min-w-0 flex-1 rounded-lg border border-default bg-surface px-3 py-2 text-sm" placeholder={$t('common-search')} bind:value={search} />
      <Button onclick={() => openEditor()} disabled={!snapshot?.writable}>{$t('ssh-config-add-host')}</Button>
    </div>
    <div class="grid gap-3">
      {#each visibleHosts as host (host.alias)}
        <Surface class="p-4">
          <div class="flex items-center justify-between gap-4">
            <div class="min-w-0">
              <h2 class="font-semibold text-fg">{host.alias}</h2>
              <p class="truncate text-sm text-muted">{host.user}@{host.hostname}:{host.port}</p>
              {#if host.proxyJump}<p class="truncate text-xs text-muted">ProxyJump {host.proxyJump}</p>{/if}
            </div>
            <div class="flex gap-2">
              <Button variant="ghost" onclick={() => openEditor(host)}>{$t('common-edit')}</Button>
              <Button variant="ghost" onclick={() => void removeHost(host.alias)}>{$t('common-delete')}</Button>
            </div>
          </div>
        </Surface>
      {/each}
    </div>
  {:else if tab === 'keys'}
    <div class="flex flex-wrap items-start justify-between gap-3">
      <div>
        <h2 class="font-semibold text-fg">{$t('ssh-keys-title')}</h2>
        <p class="mt-1 text-sm text-muted">{$t('ssh-keys-description')}</p>
      </div>
      <Button variant="primary" onclick={() => openKeyDialog({ kind: 'create' })}>
        {$t('ssh-keys-create')}
      </Button>
    </div>

    <div class="grid gap-3">
      {#each keySnapshot.records as record (record.id)}
        <Surface class="p-4">
          <div class="flex flex-wrap items-start justify-between gap-4">
            <div class="min-w-0 flex-1">
              <div class="flex flex-wrap items-center gap-2">
                <h3 class="font-semibold text-fg">{record.name}</h3>
                <span class="rounded-full bg-surface-inset px-2 py-0.5 text-xs text-muted">{record.keyType}</span>
                {#if !record.available}
                  <span class="rounded-full bg-status-warn/10 px-2 py-0.5 text-xs text-status-warn">{$t('ssh-keys-missing')}</span>
                {/if}
              </div>
              <p class="mt-2 break-all font-mono text-xs text-muted">{record.privatePath}</p>
              <p class="mt-1 break-all font-mono text-xs text-muted">{record.publicPath}</p>
              {#if copiedPath === record.privatePath || copiedPath === record.publicPath}
                <p class="mt-2 text-xs text-status-ok">{$t('ssh-keys-path-copied')}</p>
              {/if}
            </div>
            <div class="flex flex-wrap justify-end gap-1">
              <Button variant="ghost" onclick={() => void copyPublicKey(record)}>{$t('ssh-keys-copy-public')}</Button>
              <Button variant="ghost" onclick={() => void copyPath(record.privatePath)}>{$t('ssh-keys-copy-private-path')}</Button>
              <Button variant="ghost" onclick={() => openKeyDialog({ kind: 'rename', record })}>{$t('common-edit')}</Button>
              <Button variant="ghost" onclick={() => void backupKey(record.id)} disabled={!record.available}>{$t('ssh-keys-backup')}</Button>
              <Button variant="ghost" onclick={() => openKeyDialog({ kind: 'delete', record })} disabled={!record.available}>{$t('common-delete')}</Button>
            </div>
          </div>
        </Surface>
      {:else}
        <Surface class="p-5 text-sm text-muted">{$t('ssh-keys-empty')}</Surface>
      {/each}
    </div>

    {#if keySnapshot.discovered.length}
      <div>
        <h2 class="mb-2 font-semibold text-fg">{$t('ssh-keys-discovered')}</h2>
        <div class="grid gap-2">
          {#each keySnapshot.discovered as pair (pair.privatePath)}
            <Surface class="flex flex-wrap items-center justify-between gap-3 p-4">
              <div class="min-w-0">
                <p class="font-medium text-fg">{pair.suggestedName}</p>
                <p class="break-all font-mono text-xs text-muted">{pair.privatePath}</p>
              </div>
              <Button onclick={() => openKeyDialog({ kind: 'import', privatePath: pair.privatePath, suggestedName: pair.suggestedName })}>
                {$t('ssh-keys-load')}
              </Button>
            </Surface>
          {/each}
        </div>
      </div>
    {/if}

    {#if keySnapshot.backups.length}
      <div>
        <h2 class="mb-2 font-semibold text-fg">{$t('ssh-keys-backups')}</h2>
        <div class="grid gap-2">
          {#each keySnapshot.backups as backup (backup.id)}
            <Surface class="flex flex-wrap items-center justify-between gap-3 p-4">
              <div class="min-w-0">
                <p class="font-medium text-fg">{backup.name}</p>
                <p class="break-all font-mono text-xs text-muted">{backup.id}</p>
              </div>
              <Button onclick={() => void restoreKey(backup.id)}>{$t('ssh-keys-restore')}</Button>
            </Surface>
          {/each}
        </div>
      </div>
    {/if}
  {:else if tab === 'sources'}
    <div class="grid gap-4">
      {#each snapshot?.sources ?? [] as source (source.path)}
        <Surface class="p-4">
          <h2 class="break-all font-mono text-xs font-semibold">{source.path}</h2>
          <pre class="mt-3 max-h-80 overflow-auto whitespace-pre-wrap rounded-lg bg-surface-inset p-3 text-xs">{source.content ?? $t('ssh-config-source-unavailable')}</pre>
        </Surface>
      {/each}
    </div>
  {:else}
    <div class="grid gap-3">
      {#each snapshot?.backups ?? [] as backup (backup.id)}
        <Surface class="p-4">
          <div class="flex items-center justify-between gap-4">
            <div><strong>{backup.id}</strong><p class="break-all text-xs text-muted">{backup.path}</p></div>
            <Button onclick={() => openRestorePreview(backup.id)}>{$t('ssh-config-restore')}</Button>
          </div>
        </Surface>
      {/each}
    </div>
  {/if}
</section>

{#if editor}
  <Modal label={$t('ssh-config-editor-title')} onClose={() => (editor = null)}>
    <form class="overflow-auto p-6" onsubmit={(event) => { event.preventDefault(); void saveEditor(); }}>
      <h2 class="mb-4 text-lg font-bold">{$t('ssh-config-editor-title')}</h2>
      <p class="mb-4 rounded-lg bg-surface-inset px-3 py-2 text-xs text-muted">{$t('ssh-config-editor-source')}</p>
      <div class="grid gap-3">
        <label class="text-sm">{$t('ssh-config-field-alias')}<input required pattern="[A-Za-z0-9._-]+" class="mt-1 w-full rounded-lg border border-default bg-surface px-3 py-2" bind:value={editor.alias} /></label>
        <label class="text-sm">{$t('ssh-config-field-hostname')}<input required class="mt-1 w-full rounded-lg border border-default bg-surface px-3 py-2" bind:value={editor.hostname} /></label>
        <label class="text-sm">{$t('ssh-config-field-user')}<input required class="mt-1 w-full rounded-lg border border-default bg-surface px-3 py-2" bind:value={editor.user} /></label>
        <label class="text-sm">{$t('ssh-config-field-port')}<input required type="number" min="1" max="65535" class="mt-1 w-full rounded-lg border border-default bg-surface px-3 py-2" bind:value={editor.port} /></label>
        <label class="text-sm">
          {$t('ssh-config-field-identity-file')}
          <Select
            class="mt-1 w-full rounded-lg border border-default bg-surface px-3 py-2"
            bind:value={editorIdentity}
          >
            <option value="">{$t('ssh-keys-no-identity')}</option>
            {#if editor.identityFile && !keySnapshot.records.some((record) => record.privatePath === editor?.identityFile)}
              <option value={editor.identityFile}>{fileName(editor.identityFile)} · {$t('ssh-keys-unmanaged-path')}</option>
            {/if}
            {#each keySnapshot.records.filter((record) => record.available) as record (record.id)}
              <option value={record.privatePath}>{record.name} · {fileName(record.privatePath)}</option>
            {/each}
          </Select>
        </label>
        <label class="text-sm">{$t('ssh-config-field-proxy-jump')}<input class="mt-1 w-full rounded-lg border border-default bg-surface px-3 py-2" value={editor.proxyJump ?? ''} oninput={(event) => { if (editor) editor.proxyJump = event.currentTarget.value || null; }} /></label>
        <label class="text-sm">{$t('ssh-config-field-ciphers')}<input placeholder="+aes256-cbc" class="mt-1 w-full rounded-lg border border-default bg-surface px-3 py-2" value={editor.ciphers ?? ''} oninput={(event) => { if (editor) editor.ciphers = event.currentTarget.value || null; }} /></label>
      </div>
      <div class="mt-6 flex justify-end gap-2"><Button onclick={() => (editor = null)}>{$t('common-cancel')}</Button><Button type="submit" variant="primary">{$t('common-save')}</Button></div>
    </form>
  </Modal>
{/if}

{#if keyDialog}
  <Modal
    label={$t(
      keyDialog.kind === 'create'
        ? 'ssh-keys-create-title'
        : keyDialog.kind === 'import'
          ? 'ssh-keys-load-title'
          : keyDialog.kind === 'rename'
            ? 'ssh-keys-rename-title'
            : 'ssh-keys-delete-title'
    )}
    onClose={() => (keyDialog = null)}
  >
    <form class="overflow-auto p-6" onsubmit={(event) => { event.preventDefault(); void submitKeyDialog(); }}>
      <h2 class="text-lg font-bold">
        {$t(
          keyDialog.kind === 'create'
            ? 'ssh-keys-create-title'
            : keyDialog.kind === 'import'
              ? 'ssh-keys-load-title'
              : keyDialog.kind === 'rename'
                ? 'ssh-keys-rename-title'
                : 'ssh-keys-delete-title'
        )}
      </h2>

      {#if keyDialog.kind === 'delete'}
        <p class="mt-3 rounded-lg border border-status-crit/40 bg-status-crit/10 p-3 text-sm text-fg">
          {$t('ssh-keys-delete-description', { name: keyDialog.record.name })}
        </p>
        <label class="mt-4 block text-sm">
          {$t('ssh-keys-delete-confirm', { name: keyDialog.record.name })}
          <input
            class="mt-1 w-full rounded-lg border border-default bg-surface px-3 py-2"
            bind:value={deleteConfirmation}
          />
        </label>
      {:else}
        {#if keyDialog.kind === 'rename'}
          <p class="mt-3 rounded-lg bg-surface-inset p-3 text-sm text-muted">
            {$t('ssh-keys-rename-description')}
          </p>
        {:else if keyDialog.kind === 'create'}
          <p class="mt-3 rounded-lg bg-surface-inset p-3 text-sm text-muted">
            {$t('ssh-keys-create-description')}
          </p>
        {:else}
          <p class="mt-3 break-all rounded-lg bg-surface-inset p-3 font-mono text-xs text-muted">
            {keyDialog.privatePath}
          </p>
        {/if}
        <label class="mt-4 block text-sm">
          {$t('ssh-keys-record-name')}
          <input
            required
            maxlength="64"
            class="mt-1 w-full rounded-lg border border-default bg-surface px-3 py-2"
            bind:value={keyName}
          />
        </label>
        {#if keyDialog.kind === 'create'}
          <label class="mt-3 block text-sm">
            {$t('ssh-keys-file-name')}
            <input
              required
              maxlength="96"
              pattern="[A-Za-z0-9._-]+"
              class="mt-1 w-full rounded-lg border border-default bg-surface px-3 py-2"
              bind:value={keyFileStem}
            />
          </label>
          <label class="mt-3 block text-sm">
            {$t('ssh-keys-key-type')}
            <Select class="mt-1 w-full rounded-lg border border-default bg-surface px-3 py-2" bind:value={keyType}>
              <option value="ed25519">Ed25519</option>
              <option value="rsa4096">RSA 4096</option>
            </Select>
          </label>
        {/if}
      {/if}

      <div class="mt-6 flex justify-end gap-2">
        <Button onclick={() => (keyDialog = null)}>{$t('common-cancel')}</Button>
        <Button
          type="submit"
          variant="primary"
          disabled={keyBusy || (keyDialog.kind === 'delete' && deleteConfirmation !== keyDialog.record.name)}
        >
          {keyDialog.kind === 'delete' ? $t('common-delete') : $t('common-save')}
        </Button>
      </div>
    </form>
  </Modal>
{/if}

{#if preview}
  <Modal label={$t('ssh-config-preview-title')} onClose={() => (preview = null)}>
    <div class="overflow-auto p-6">
      <h2 class="text-lg font-bold">{$t('ssh-config-preview-title')}</h2>
      {#each preview.warnings as warning}<p class="mt-2 rounded bg-surface-inset p-2 text-sm"><strong>{warning.severity}</strong> · {warning.message}</p>{/each}
      <h3 class="mt-4 text-sm font-semibold">~/.ssh/config</h3><pre class="mt-1 max-h-36 overflow-auto whitespace-pre-wrap rounded bg-surface-inset p-3 text-xs">{preview.mainDiff || $t('ssh-config-no-change')}</pre>
      <h3 class="mt-4 text-sm font-semibold">omnyssh.conf</h3><pre class="mt-1 max-h-52 overflow-auto whitespace-pre-wrap rounded bg-surface-inset p-3 text-xs">{preview.managedDiff || $t('ssh-config-no-change')}</pre>
      <div class="mt-5 flex justify-end gap-2"><Button onclick={() => { preview = null; restoreId = null; }}>{$t('common-cancel')}</Button><Button variant="primary" disabled={!preview.canApply || saving} onclick={() => restoreId ? confirmRestore() : applyChanges(false)}>{restoreId ? $t('ssh-config-confirm-restore-action') : $t('ssh-config-confirm-apply')}</Button></div>
    </div>
  </Modal>
{/if}
