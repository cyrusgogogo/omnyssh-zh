import { writable } from 'svelte/store';
import type { ServerCard } from '$lib/screens/serverCard';

export const SORT_OPTIONS = ['default', 'name', 'status', 'cpu', 'ram', 'disk'] as const;
export type OverviewSort = (typeof SORT_OPTIONS)[number];
export type OverviewPreferences = {
  view: 'cards' | 'list';
  sort: OverviewSort;
  descending: boolean;
};
const DEFAULT: OverviewPreferences = { view: 'cards', sort: 'default', descending: false };
const KEY = 'omnyssh-overview';

function normalize(value: unknown): OverviewPreferences {
  const p = value as Partial<OverviewPreferences> | null;
  return {
    view: p?.view === 'list' ? 'list' : 'cards',
    sort: SORT_OPTIONS.includes(p?.sort as OverviewSort) ? p!.sort! : 'default',
    descending: p?.descending === true
  };
}

function initial(): OverviewPreferences {
  try { return normalize(JSON.parse(localStorage.getItem(KEY) ?? 'null')); }
  catch { return DEFAULT; }
}

function createOverviewPreferences() {
  let current = initial();
  let interacted = false;
  let pending = Promise.resolve();
  const { subscribe, set } = writable(current);
  function apply(value: OverviewPreferences) {
    current = value;
    set(value);
    try { localStorage.setItem(KEY, JSON.stringify(value)); } catch { /* Storage unavailable. */ }
  }
  return {
    subscribe,
    change(patch: Partial<OverviewPreferences>) {
      interacted = true;
      const next = normalize({ ...current, ...patch });
      apply(next);
      pending = pending.then(async () => {
        const { load } = await import('@tauri-apps/plugin-store');
        const store = await load('settings.json');
        await store.set('overview', next);
        await store.save();
      }).catch(() => {});
    },
    async hydrate() {
      try {
        const { load } = await import('@tauri-apps/plugin-store');
        const store = await load('settings.json');
        const saved = await store.get('overview');
        if (!interacted && saved) apply(normalize(saved));
      } catch { /* The local mirror works in browser previews. */ }
    }
  };
}

export const overviewPreferences = createOverviewPreferences();

/** Missing metrics remain last in either direction; ties have a stable name order. */
export function sortCards(cards: ServerCard[], sort: OverviewSort, descending: boolean): ServerCard[] {
  if (sort === 'default') return descending ? [...cards].reverse() : cards;
  const direction = descending ? -1 : 1;
  const byName = (a: ServerCard, b: ServerCard) =>
    a.host.name.localeCompare(b.host.name, undefined, { numeric: true, sensitivity: 'base' });
  const severity = { ok: 0, unknown: 1, warn: 2, crit: 3, off: 4 };
  function value(card: ServerCard): number | null {
    if (sort === 'status') return severity[card.overall];
    const metric = card.metricRows.find((row) => row.label.toLowerCase() === sort)?.percent;
    return metric != null && Number.isFinite(metric) ? metric : null;
  }
  return [...cards].sort((a, b) => {
    if (sort === 'name') return direction * byName(a, b);
    const av = value(a), bv = value(b);
    if (av == null && bv != null) return 1;
    if (av != null && bv == null) return -1;
    return av != null && bv != null && av !== bv ? direction * (av - bv) : byName(a, b);
  });
}
