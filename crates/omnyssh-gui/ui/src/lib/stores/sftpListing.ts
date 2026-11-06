import { writable } from 'svelte/store';
import type { FileEntryDto } from '$lib/bindings';
import type { PaneSide } from './sftp';

export const FILE_SORT_OPTIONS = ['name', 'modified', 'type', 'size'] as const;
export type FileSort = (typeof FILE_SORT_OPTIONS)[number];
export type FileOrder = { sort: FileSort; descending: boolean };
type Preferences = Record<PaneSide, FileOrder>;
const KEY = 'omnyssh-sftp-listing';

function normalize(value: unknown): Preferences {
  const saved = value as Partial<Preferences> | null;
  const order = (side: PaneSide): FileOrder => ({
    sort: FILE_SORT_OPTIONS.includes(saved?.[side]?.sort as FileSort) ? saved![side]!.sort : 'name',
    descending: saved?.[side]?.descending === true
  });
  return { local: order('local'), remote: order('remote') };
}

function initial(): Preferences {
  try { return normalize(JSON.parse(localStorage.getItem(KEY) ?? 'null')); }
  catch { return normalize(null); }
}

function createPreferences() {
  let current = initial();
  let interacted = false;
  let pending = Promise.resolve();
  const { subscribe, set } = writable(current);
  function apply(value: Preferences) {
    current = value;
    set(value);
    try { localStorage.setItem(KEY, JSON.stringify(value)); } catch { /* Storage unavailable. */ }
  }
  return {
    subscribe,
    change(side: PaneSide, patch: Partial<FileOrder>) {
      interacted = true;
      const next = normalize({ ...current, [side]: { ...current[side], ...patch } });
      apply(next);
      pending = pending.then(async () => {
        const { load } = await import('@tauri-apps/plugin-store');
        const store = await load('settings.json');
        await store.set('sftpListing', next);
        await store.save();
      }).catch(() => {});
    },
    async hydrate() {
      try {
        const { load } = await import('@tauri-apps/plugin-store');
        const store = await load('settings.json');
        const saved = await store.get('sftpListing');
        if (!interacted && saved) apply(normalize(saved));
      } catch { /* Browser previews use the local mirror. */ }
    }
  };
}

export const sftpListing = createPreferences();

export function fileExtension(name: string): string {
  const dot = name.lastIndexOf('.');
  return dot > 0 && dot < name.length - 1 ? name.slice(dot + 1).toLowerCase() : '';
}

/** Parent and directories stay first; unavailable dates stay last in either direction. */
export function sortEntries(entries: FileEntryDto[], order: FileOrder): FileEntryDto[] {
  const direction = order.descending ? -1 : 1;
  const byName = (a: FileEntryDto, b: FileEntryDto) =>
    a.name.localeCompare(b.name, undefined, { numeric: true, sensitivity: 'base' });
  return [...entries].sort((a, b) => {
    if ((a.name === '..') !== (b.name === '..')) return a.name === '..' ? -1 : 1;
    if (a.isDir !== b.isDir) return a.isDir ? -1 : 1;
    let compared = 0;
    if (order.sort === 'name') compared = byName(a, b);
    else if (order.sort === 'size') compared = a.size - b.size;
    else if (order.sort === 'type') compared = a.isDir ? 0 : fileExtension(a.name).localeCompare(fileExtension(b.name));
    else {
      const av = a.modified, bv = b.modified;
      if (av == null && bv != null) return 1;
      if (av != null && bv == null) return -1;
      if (av != null && bv != null) compared = av - bv;
    }
    return compared === 0 ? byName(a, b) : direction * compared;
  });
}

const dateFormatters = new Map<string, Intl.DateTimeFormat>();

export function formatModified(seconds: number | null | undefined, locale: string): string {
  if (seconds == null || !Number.isFinite(seconds)) return '—';
  const date = new Date(seconds * 1000);
  if (!Number.isFinite(date.getTime())) return '—';
  let formatter = dateFormatters.get(locale);
  if (!formatter) {
    formatter = new Intl.DateTimeFormat(locale, {
      year: 'numeric', month: '2-digit', day: '2-digit',
      hour: '2-digit', minute: '2-digit', hour12: false
    });
    dateFormatters.set(locale, formatter);
  }
  return formatter.format(date);
}
