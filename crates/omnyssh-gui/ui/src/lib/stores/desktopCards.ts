import { writable } from 'svelte/store';

const HOSTS_KEY = 'omnyssh-desktop-card-hosts';
const PINNED_KEY = 'omnyssh-desktop-card-pinned';

export function normalizeDesktopCardHosts(value: unknown): string[] {
  if (!Array.isArray(value)) return [];
  const unique = new Set<string>();
  for (const item of value) {
    if (typeof item !== 'string') continue;
    const name = item.trim();
    if (name) unique.add(name);
  }
  return [...unique];
}

export function addDesktopCardHost(hosts: string[], name: string): string[] {
  return normalizeDesktopCardHosts([...hosts, name]);
}

export function removeDesktopCardHost(hosts: string[], name: string): string[] {
  return hosts.filter((host) => host !== name);
}

function readHosts(): string[] {
  try {
    return normalizeDesktopCardHosts(JSON.parse(localStorage.getItem(HOSTS_KEY) ?? '[]'));
  } catch {
    return [];
  }
}

function readPinned(): boolean {
  try {
    return localStorage.getItem(PINNED_KEY) !== 'false';
  } catch {
    return true;
  }
}

function createDesktopCardHosts() {
  let current = readHosts();
  const { subscribe, set: setStore } = writable(current);

  function apply(value: string[], persist: boolean): void {
    current = normalizeDesktopCardHosts(value);
    setStore(current);
    if (persist) {
      try {
        localStorage.setItem(HOSTS_KEY, JSON.stringify(current));
      } catch {
        // The current webview keeps working when localStorage is unavailable.
      }
    }
  }

  return {
    subscribe,
    set: (value: string[]) => apply(value, true),
    add: (name: string) => apply(addDesktopCardHost(current, name), true),
    remove: (name: string) => apply(removeDesktopCardHost(current, name), true),
    toggle(name: string): boolean {
      const selected = current.includes(name);
      apply(selected ? removeDesktopCardHost(current, name) : addDesktopCardHost(current, name), true);
      return !selected;
    },
    /** Reconcile changes made by the other Tauri webview. */
    syncFromStorage: () => apply(readHosts(), false)
  };
}

function createDesktopCardPinned() {
  let current = readPinned();
  const { subscribe, set: setStore } = writable(current);

  function apply(value: boolean, persist: boolean): void {
    current = value;
    setStore(value);
    if (persist) {
      try {
        localStorage.setItem(PINNED_KEY, String(value));
      } catch {
        // Keep the in-memory preference if persistence is unavailable.
      }
    }
  }

  return {
    subscribe,
    set: (value: boolean) => apply(value, true),
    toggle(): boolean {
      apply(!current, true);
      return current;
    },
    syncFromStorage: () => apply(readPinned(), false)
  };
}

export const desktopCardHosts = createDesktopCardHosts();
export const desktopCardPinned = createDesktopCardPinned();
