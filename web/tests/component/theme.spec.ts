import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest';
import { themeState, initialTheme } from '../../src/state/theme';
import { clearThemePref, setThemePref } from '../../src/lib/storage';

/** In-memory localStorage stub for happy-dom / Node 26 environments. */
function makeLocalStorage(): Storage {
  const store: Record<string, string> = {};
  return {
    getItem: (k: string) => store[k] ?? null,
    setItem: (k: string, v: string) => {
      store[k] = v;
    },
    removeItem: (k: string) => {
      delete store[k];
    },
    clear: () => {
      for (const k of Object.keys(store)) delete store[k];
    },
    get length() {
      return Object.keys(store).length;
    },
    key: (i: number) => Object.keys(store)[i] ?? null,
  } as Storage;
}

describe('theme controller', () => {
  beforeEach(() => {
    vi.stubGlobal('localStorage', makeLocalStorage());
    clearThemePref();
    document.documentElement.removeAttribute('data-theme');
    vi.stubGlobal('matchMedia', (q: string) => ({
      matches: false,
      media: q,
      addEventListener: () => {},
      removeEventListener: () => {},
    }));
  });
  afterEach(() => vi.restoreAllMocks());

  it('initialTheme falls back to system (light) when unset', () => {
    expect(initialTheme()).toBe('light');
  });

  it('initialTheme honors a stored preference over system', () => {
    setThemePref('dark');
    expect(initialTheme()).toBe('dark');
  });

  it('set() reflects on <html> and persists', () => {
    themeState.set('dark');
    expect(document.documentElement.getAttribute('data-theme')).toBe('dark');
    expect(themeState.theme.value).toBe('dark');
  });

  it('toggle() flips the current theme', () => {
    themeState.set('light');
    themeState.toggle();
    expect(themeState.theme.value).toBe('dark');
  });
});
