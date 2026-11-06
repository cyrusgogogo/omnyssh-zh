<script lang="ts">
  // One side of the dual-pane SFTP browser (tech-gui.md §3.2): a current-path header
  // with a parent-supplied toolbar, then the entry list. Clicking a directory (or the
  // `..` row) navigates; clicking a file previews; the leading checkbox marks it for a
  // batch transfer/delete. Semantic tokens only — no colour literals (§5.1).
  import type { Snippet } from 'svelte';
  import { Icon } from '$lib/theme';
  import type { FileEntryDto } from '$lib/bindings';
  import { formatBytes, type Pane, type PaneSide } from '$lib/stores/sftp';
  import { sftpListing, sortEntries, fileExtension, formatModified, FILE_SORT_OPTIONS, type FileSort } from '$lib/stores/sftpListing';
  import Select from '$lib/components/Select.svelte';
  import { language, t } from '$lib/i18n';

  let {
    title,
    side,
    pane,
    onNavigate,
    onToggleMark,
    onPreview,
    toolbar
  }: {
    title: string;
    side: PaneSide;
    pane: Pane;
    onNavigate: (entry: FileEntryDto) => void;
    onToggleMark: (path: string) => void;
    onPreview: (entry: FileEntryDto) => void;
    toolbar?: Snippet;
  } = $props();

  const rowBase =
    'flex w-full min-w-0 items-center gap-2 rounded px-2 py-1.5 text-left text-sm transition ' +
    'focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-focus';

  const order = $derived($sftpListing[side]);
  const entries = $derived(sortEntries(pane.entries, order));

  function entryType(entry: FileEntryDto): string {
    if (entry.name === '..') return '';
    if (entry.isDir) return $t('files-folder');
    const extension = fileExtension(entry.name);
    return extension ? $t('files-extension-type', { extension: extension.toUpperCase() }) : $t('files-type-file');
  }
</script>

<section aria-label={title} class="flex min-h-0 min-w-0 flex-1 flex-col">
  <header class="shrink-0 border-b border-default px-3 py-2.5">
    <div class="flex items-center justify-between gap-2">
      <h2
        title={title}
        class="truncate text-xs font-semibold uppercase tracking-[0.14em] text-muted"
      >
        {title}
      </h2>
      <div class="flex shrink-0 items-center gap-1">
        {@render toolbar?.()}
      </div>
    </div>
    <div class="mt-1 truncate font-mono text-xs text-faint" title={pane.path}>
      {pane.path || '—'}
    </div>
    <div class="mt-2 flex flex-wrap items-center gap-2 text-xs text-muted">
      <span>{$t('files-sort-by')}</span>
      <Select
        value={order.sort}
        aria-label={$t('files-sort-by')}
        class="rounded-md border border-default bg-surface-inset px-2 py-1 text-xs text-fg focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-focus"
        onchange={(event) => sftpListing.change(side, { sort: event.currentTarget.value as FileSort })}
      >
        {#each FILE_SORT_OPTIONS as sort}
          <option value={sort}>{$t(`files-${sort}`)}</option>
        {/each}
      </Select>
      <button
        type="button"
        class="rounded-md border border-default px-2 py-1 hover:bg-surface-inset focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-focus"
        aria-label={$t('files-sort-descending')}
        aria-pressed={order.descending}
        onclick={() => sftpListing.change(side, { descending: !order.descending })}
      >
        {order.descending ? '↓' : '↑'} {$t(order.descending ? 'files-descending' : 'files-ascending')}
      </button>
    </div>
  </header>

  <div class="min-h-0 flex-1 overflow-y-auto px-1.5 py-1.5">
    {#if pane.error}
      <p class="px-2 py-6 text-center text-sm text-status-crit">{pane.error}</p>
    {:else if pane.loading && pane.entries.length === 0}
      <p class="px-2 py-6 text-center text-sm text-faint">{$t('common-loading')}</p>
    {:else if pane.entries.length === 0}
      <p class="px-2 py-6 text-center text-sm text-faint">{$t('files-empty-directory')}</p>
    {:else}
      <ul class="space-y-0.5">
        {#each entries as entry (entry.path)}
          {@const isParent = entry.name === '..'}
          {@const marked = pane.marked.has(entry.path)}
          <li class="flex items-center gap-1.5">
            {#if isParent}
              <span class="h-4 w-4 shrink-0"></span>
            {:else}
              <button
                type="button"
                role="checkbox"
                aria-checked={marked}
                aria-label={$t('files-mark-name', { name: entry.name })}
                class="grid h-4 w-4 shrink-0 place-items-center rounded border transition
                  focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-focus
                  {marked ? 'border-accent bg-accent text-accent-fg' : 'border-strong text-transparent'}"
                onclick={() => onToggleMark(entry.path)}
              >
                {#if marked}<Icon name="check" size={11} />{/if}
              </button>
            {/if}
            <button
              type="button"
              class="{rowBase} text-muted hover:bg-surface-inset hover:text-fg"
              title={entry.name}
              onclick={() => (entry.isDir ? onNavigate(entry) : onPreview(entry))}
            >
              <Icon name={entry.isDir ? 'folder' : 'file'} size={15} />
              <span class="min-w-0 flex-1">
                <span class="block truncate {entry.isDir ? 'font-medium text-fg' : ''}">{entry.name}</span>
                {#if !isParent}
                  <span class="mt-0.5 flex flex-wrap gap-x-3 gap-y-0.5 text-[11px] text-faint">
                    <span title={$t('files-type')}>{entryType(entry)}</span>
                    <span class="tabular-nums" title={$t('files-modified')}>
                      {formatModified(entry.modified, $language.locale)}
                    </span>
                  </span>
                {/if}
              </span>
              {#if !entry.isDir}
                <span class="shrink-0 tabular-nums text-xs text-faint">{formatBytes(entry.size)}</span>
              {/if}
            </button>
          </li>
        {/each}
      </ul>
    {/if}
  </div>
</section>
