import { describe, it, expect, beforeEach, vi } from 'vitest';
import { saveSession, loadSession, clearSession } from '../../src/lib/storage';

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
