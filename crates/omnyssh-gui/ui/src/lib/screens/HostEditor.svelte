<script lang="ts">
  // Add/edit host form (tech-gui.md §4.1). Always writes a manual entry: editing an
  // SSH-config import adopts it, leaving ~/.ssh/config untouched. Validation mirrors the TUI via
  // `formToInput`; on submit the parent persists + reloads, and a rejected save
  // surfaces inline without closing. Semantic tokens only.
  import { onMount } from 'svelte';
  import { get } from 'svelte/store';
  import type { HostInputDto, SshKeyRecordDto } from '$lib/bindings';
  import { Button } from '$lib/theme';
  import Modal from '$lib/components/Modal.svelte';
  import Select from '$lib/components/Select.svelte';
  import { formToInput, type HostFormFields } from './hostForm';
  import { t } from '$lib/i18n';
  import { getSshKeySnapshot } from '$lib/ipc/commands';

  let {
    mode,
    initial,
    previousName,
    imported = false,
    onSubmit,
    onCancel
  }: {
    mode: 'add' | 'edit';
    initial: HostFormFields;
    previousName?: string;
    /** Editing an `~/.ssh/config` import, so the save is an adoption — say so. */
    imported?: boolean;
    onSubmit: (input: HostInputDto, previousName: string | undefined) => Promise<void>;
    onCancel: () => void;
  } = $props();

  // Seeded once from `initial`; the editor is remounted per open, so the prop never
  // changes under a live instance.
  // svelte-ignore state_referenced_locally
  let fields = $state<HostFormFields>({ ...initial });
  let error = $state<string | null>(null);
  let saving = $state(false);
  let nameEl = $state<HTMLInputElement>();
  let hostnameEl = $state<HTMLInputElement>();
  let keyRecords = $state<SshKeyRecordDto[]>([]);

  // The name is the on-disk key; a rename can't carry backend-only secrets across the
  // boundary (§3.4), so on edit it is immutable — rename by delete + re-add. Focus the
  // first editable field accordingly.
  onMount(() => {
    (mode === 'add' ? nameEl : hostnameEl)?.focus();
    void getSshKeySnapshot()
      .then((snapshot) => (keyRecords = snapshot.records.filter((record) => record.available)))
      .catch(() => {
        // Key management is optional for saving a host; keep the empty choice available.
      });
  });

  async function save(): Promise<void> {
    const result = formToInput(fields);
    if (!result.ok) {
      error = translateFormError(result.error);
      return;
    }
    error = null;
    saving = true;
    try {
      await onSubmit(result.input, previousName);
    } catch (e) {
      error = e instanceof Error ? e.message : String(e);
    } finally {
      saving = false;
    }
  }

  function translateFormError(value: string): string {
    if (value === 'Name cannot be empty') return get(t)('validation-name-required');
    if (value === 'Hostname / IP cannot be empty') return get(t)('validation-host-required');
    const port = value.match(/^Port must .* got '(.+)'$/)?.[1];
    if (port) return get(t)('validation-port', { value: port });
    const probe = value.match(/^Probe port must .* got '(.+)'$/)?.[1];
    if (probe) return get(t)('validation-probe-port', { value: probe });
    return value;
  }

  // On edit the DTO omits identity/password (§3.4), so the fields start blank and mean
  // "keep the stored value"; on add they mean "none".
  const secretHint = $derived(mode === 'edit' ? $t('host-editor-keep-secret') : undefined);

  const label = 'block space-y-1 text-xs font-medium text-muted';
  const field =
    'w-full rounded-lg bg-surface-inset px-3 py-2 text-sm text-fg outline-none ' +
    'focus-visible:ring-2 focus-visible:ring-focus placeholder:text-faint';
  const fileName = (value: string): string => value.split(/[\\/]/).pop() ?? value;
</script>

<Modal label={mode === 'add' ? $t('host-editor-add-title') : $t('host-editor-edit-title')} onClose={onCancel}>
  <form
    onsubmit={(e) => {
      e.preventDefault();
      void save();
    }}
    class="flex min-h-0 flex-col"
  >
    <header class="border-b border-default px-5 py-3.5">
      <h2 class="text-sm font-semibold">{mode === 'add' ? $t('host-editor-add-title') : $t('host-editor-edit-title')}</h2>
    </header>

    <div class="min-h-0 flex-1 space-y-3.5 overflow-y-auto px-5 py-4">
      {#if mode === 'add'}
        <p class="rounded-lg bg-surface-inset px-3 py-2 text-xs text-muted">
          {$t('host-editor-add-source')}
        </p>
      {/if}
      {#if imported}
        <p class="rounded-lg bg-surface-inset px-3 py-2 text-xs text-muted">
          {$t('host-editor-import-before')}<span class="font-mono">~/.ssh/config</span>{$t('host-editor-import-middle')}<span class="font-mono">hosts.toml</span>{$t('host-editor-import-after')}
        </p>
      {/if}
      <label class={label}>
        <span>{$t('host-editor-name')} {mode === 'edit' ? $t('host-editor-fixed') : ''}</span>
        <input
          bind:this={nameEl}
          bind:value={fields.name}
          class="{field} {mode === 'edit' ? 'cursor-not-allowed text-muted' : ''}"
          placeholder="web-prod-1"
          readonly={mode === 'edit'}
          title={mode === 'edit' ? $t('host-editor-rename-hint') : undefined}
        />
      </label>

      <label class={label}>
        <span>{$t('host-editor-hostname')}</span>
        <input bind:this={hostnameEl} bind:value={fields.hostname} class="{field} font-mono" placeholder="10.0.0.1" />
      </label>

      <div class="grid grid-cols-[1fr,7rem] gap-3">
        <label class={label}>
          <span>{$t('host-editor-user')}</span>
          <input bind:value={fields.user} class={field} placeholder="root" />
        </label>
        <label class={label}>
          <span>{$t('host-editor-port')}</span>
          <input bind:value={fields.port} inputmode="numeric" class={field} placeholder="22" />
        </label>
      </div>

      <label class={label}>
        <span>{$t('host-editor-identity-file')}</span>
        <Select bind:value={fields.identityFile} class="{field} font-mono">
          <option value="">{secretHint ?? $t('ssh-keys-no-identity')}</option>
          {#each keyRecords as record (record.id)}
            <option value={record.privatePath}>{record.name} · {fileName(record.privatePath)}</option>
          {/each}
        </Select>
      </label>

      <label class={label}>
        <span>{$t('host-editor-password')}</span>
        <input
          type="password"
          bind:value={fields.password}
          class={field}
          placeholder={secretHint ?? $t('host-editor-password-hint')}
          autocomplete="off"
        />
      </label>

      <label class={label}>
        <span>{$t('host-editor-tags')}</span>
        <input bind:value={fields.tags} class={field} placeholder="prod, web" />
      </label>

      <label class={label}>
        <span>{$t('host-editor-notes')}</span>
        <textarea bind:value={fields.notes} rows="2" class="{field} resize-y" placeholder={$t('host-editor-optional')}></textarea>
      </label>

      <label class="flex cursor-pointer items-start gap-3 rounded-lg bg-surface-inset px-3 py-2.5">
        <input
          type="checkbox"
          bind:checked={fields.hiddenFromOverview}
          class="mt-0.5 h-4 w-4 accent-[var(--color-accent)]"
        />
        <span class="min-w-0">
          <span class="block text-sm font-medium text-fg">{$t('host-editor-hide-overview')}</span>
          <span class="block text-xs text-muted">{$t('host-editor-hide-overview-description')}</span>
        </span>
      </label>

      <div class="grid grid-cols-2 gap-3">
        <label class={label}>
          <span>{$t('host-editor-monitoring')}</span>
          <Select bind:value={fields.monitoring} class={field}>
            <option value="ssh">{$t('host-editor-monitoring-ssh')}</option>
            <option value="tcpPort">{$t('host-editor-monitoring-tcp')}</option>
          </Select>
        </label>
        {#if fields.monitoring === 'tcpPort'}
          <label class={label}>
            <span>{$t('host-editor-probe-port')}</span>
            <input
              bind:value={fields.monitorPort}
              inputmode="numeric"
              class={field}
              placeholder={fields.port || '22'}
            />
          </label>
        {/if}
      </div>
      {#if fields.monitoring === 'tcpPort'}
        <p class="text-xs text-faint">{$t('host-editor-tcp-description')}</p>
      {/if}

      {#if error}
        <p class="text-xs text-status-crit">{error}</p>
      {/if}
    </div>

    <footer class="flex justify-end gap-2 border-t border-default px-5 py-3">
      <Button variant="ghost" onclick={onCancel}>{$t('common-cancel')}</Button>
      <Button variant="primary" type="submit" disabled={saving}>
        {mode === 'add' ? $t('host-editor-add-title') : $t('common-save')}
      </Button>
    </footer>
  </form>
</Modal>
