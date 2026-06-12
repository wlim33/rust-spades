import { describe, it, expect, beforeEach, vi } from 'vitest';
import { saveSession, loadSession, clearSession } from '../../src/lib/storage';
import { getThemePref, setThemePref, clearThemePref } from '../../src/lib/storage';
import { getSoundPref, setSoundPref } from '../../src/lib/storage';

describe('game session storage', () => {
  beforeEach(() => {
    const store: Record<string, string> = {};
    vi.stubGlobal('localStorage', {
      getItem: (k: string) => (k in store ? store[k]! : null),
      setItem: (k: string, v: string) => {
        store[k] = v;
      },
      removeItem: (k: string) => {
        delete store[k];
      },
    });
  });

  it('round-trips a session', () => {
    saveSession('abc', 'gid', 'pid');
    expect(loadSession('abc')).toEqual({ gid: 'gid', pid: 'pid' });
  });

  it('returns null for missing', () => {
    expect(loadSession('nope')).toBe(null);
  });

  it('clears a session', () => {
    saveSession('abc', 'gid', 'pid');
    clearSession('abc');
    expect(loadSession('abc')).toBe(null);
  });

  it('returns null on parse error', () => {
    localStorage.setItem('spades_game_bad', 'not json');
    expect(loadSession('bad')).toBe(null);
  });
});

describe('theme preference storage', () => {
  beforeEach(() => {
    const store: Record<string, string> = {};
    vi.stubGlobal('localStorage', {
      getItem: (k: string) => (k in store ? store[k]! : null),
      setItem: (k: string, v: string) => {
        store[k] = v;
      },
      removeItem: (k: string) => {
        delete store[k];
      },
    });
  });

  it('returns null when unset', () => {
    expect(getThemePref()).toBe(null);
  });

  it('round-trips a valid theme', () => {
    setThemePref('dark');
    expect(getThemePref()).toBe('dark');
  });

  it('ignores an invalid stored value', () => {
    localStorage.setItem('spades_theme', 'banana');
    expect(getThemePref()).toBe(null);
  });

  it('clears the preference', () => {
    setThemePref('light');
    clearThemePref();
    expect(getThemePref()).toBe(null);
  });
});

describe('sound preference storage', () => {
  beforeEach(() => {
    const store: Record<string, string> = {};
    vi.stubGlobal('localStorage', {
      getItem: (k: string) => (k in store ? store[k]! : null),
      setItem: (k: string, v: string) => {
        store[k] = v;
      },
      removeItem: (k: string) => {
        delete store[k];
      },
    });
  });

  it('defaults to on', () => {
    expect(getSoundPref()).toBe(true);
  });

  it('round-trips off and on', () => {
    setSoundPref(false);
    expect(getSoundPref()).toBe(false);
    setSoundPref(true);
    expect(getSoundPref()).toBe(true);
  });
});
