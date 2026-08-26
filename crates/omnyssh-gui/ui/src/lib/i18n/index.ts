import { FluentBundle, FluentResource, type FluentVariable } from '@fluent/bundle';
import { derived, writable } from 'svelte/store';
import enMessages from '../../../../../../locales/en-US/gui.ftl?raw';
import zhMessages from '../../../../../../locales/zh-CN/gui.ftl?raw';

export type AppLocale = 'en-US' | 'zh-CN';
export type LanguagePreference = 'system' | AppLocale;

const LOCAL_KEY = 'omnyssh-language';
const STORE_FILE = 'settings.json';
const STORE_KEY = 'language';
const resources: Record<AppLocale, string> = { 'en-US': enMessages, 'zh-CN': zhMessages };

export function normalizeLocale(value: string | null | undefined): AppLocale | undefined {
  const locale = value?.trim().replaceAll('_', '-').toLowerCase();
  if (!locale) return undefined;
  if (locale === 'en' || locale.startsWith('en-')) return 'en-US';
  if (
    locale === 'zh' ||
    locale === 'zh-cn' ||
    locale === 'zh-sg' ||
    locale === 'zh-hans' ||
    locale.startsWith('zh-hans-')
  ) {
    return 'zh-CN';
  }
  return undefined;
}

function systemLocale(): AppLocale {
  if (typeof navigator !== 'undefined') {
    for (const requested of navigator.languages ?? [navigator.language]) {
      const locale = normalizeLocale(requested);
      if (locale) return locale;
    }
  }
  return 'en-US';
}

function resolvePreference(preference: LanguagePreference): AppLocale {
  return preference === 'system' ? systemLocale() : preference;
}

function bundleFor(locale: AppLocale): FluentBundle {
  const bundle = new FluentBundle(locale, { useIsolating: false });
  const errors = bundle.addResource(new FluentResource(resources[locale]));
  if (errors.length) console.warn(`Invalid ${locale} translations`, errors);
  return bundle;
}

function mirroredPreference(): LanguagePreference {
  try {
    const saved = localStorage.getItem(LOCAL_KEY);
    if (saved === 'system' || saved === 'en-US' || saved === 'zh-CN') return saved;
  } catch {
    // The Tauri store remains canonical when localStorage is unavailable.
  }
  return 'system';
}

function reflectLocale(locale: AppLocale): void {
  if (typeof document !== 'undefined') document.documentElement.lang = locale;
}

function mirrorPreference(preference: LanguagePreference): void {
  try {
    localStorage.setItem(LOCAL_KEY, preference);
  } catch {
    // The Tauri store remains canonical when localStorage is unavailable.
  }
}

async function persistPreference(preference: LanguagePreference): Promise<void> {
  try {
    const { load } = await import('@tauri-apps/plugin-store');
    const store = await load(STORE_FILE);
    await store.set(STORE_KEY, preference);
    await store.save();
  } catch {
    // Plain Vite previews and tests use the localStorage mirror only.
  }
}

type I18nState = { preference: LanguagePreference; locale: AppLocale; bundle: FluentBundle };

function createLanguage() {
  const initialPreference = mirroredPreference();
  const initialLocale = resolvePreference(initialPreference);
  const state = writable<I18nState>({
    preference: initialPreference,
    locale: initialLocale,
    bundle: bundleFor(initialLocale)
  });
  let interacted = false;

  function apply(preference: LanguagePreference, user: boolean): void {
    const locale = resolvePreference(preference);
    reflectLocale(locale);
    mirrorPreference(preference);
    state.set({ preference, locale, bundle: bundleFor(locale) });
    if (user) {
      interacted = true;
      void persistPreference(preference);
    }
  }

  reflectLocale(initialLocale);

  return {
    subscribe: state.subscribe,
    set: (preference: LanguagePreference) => apply(preference, true),
    async hydrate(): Promise<void> {
      try {
        const { load } = await import('@tauri-apps/plugin-store');
        const store = await load(STORE_FILE);
        const saved = await store.get<LanguagePreference>(STORE_KEY);
        if (interacted) return;
        if (saved === 'system' || saved === 'en-US' || saved === 'zh-CN') {
          apply(saved, false);
          return;
        }

        // A settings file with any pre-i18n preference marks an existing
        // installation. Preserve its historical English UI on first upgrade.
        const oldKeys = ['theme', 'sidebarCollapsed', 'streamerMode', 'refreshInterval'];
        for (const key of oldKeys) {
          if ((await store.get(key)) != null) {
            apply('en-US', false);
            await persistPreference('en-US');
            return;
          }
        }
      } catch {
        // Store unavailable: keep the synchronous browser-derived preference.
      }
    }
  };
}

export const language = createLanguage();
export const locale = derived(language, ($language) => $language.locale);
export const languagePreference = derived(language, ($language) => $language.preference);
export const t = derived(language, ($language) => {
  return (id: string, args?: Record<string, FluentVariable>): string => {
    const message = $language.bundle.getMessage(id);
    if (!message?.value) {
      if ($language.locale !== 'en-US') {
        const fallbackBundle = bundleFor('en-US');
        const fallback = fallbackBundle.getMessage(id);
        if (fallback?.value) return fallbackBundle.formatPattern(fallback.value, args);
      }
      console.warn(`Missing translation: ${id}`);
      return id;
    }
    return $language.bundle.formatPattern(message.value, args);
  };
});

export function formatCommandError(error: {
  code?: string;
  args?: Partial<Record<string, string>>;
  rawDetail?: string | null;
  message?: string | null;
}): string {
  let translate: (id: string, args?: Record<string, FluentVariable>) => string = (id) => id;
  const unsubscribe = t.subscribe((value) => (translate = value));
  unsubscribe();
  const args = Object.fromEntries(
    Object.entries(error.args ?? {}).filter((entry): entry is [string, string] => entry[1] !== undefined)
  );
  const summary = error.code ? translate(`error-${error.code}`, args) : translate('error-unknown');
  const detail = error.rawDetail ?? error.message;
  return detail && detail !== summary ? `${summary}: ${detail}` : summary;
}
