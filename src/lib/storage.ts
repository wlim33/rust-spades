export type SavedSession = { gid: string; pid: string };

const key = (shortId: string): string => `spades_game_${shortId}`;

export function saveSession(shortId: string, gid: string, pid: string): void {
  try {
    localStorage.setItem(key(shortId), JSON.stringify({ gid, pid }));
  } catch {
    // localStorage may be unavailable (e.g. private mode); ignore
  }
}

export function loadSession(shortId: string): SavedSession | null {
  try {
    const raw = localStorage.getItem(key(shortId));
    if (!raw) return null;
    return JSON.parse(raw) as SavedSession;
  } catch {
    return null;
  }
}

export function clearSession(shortId: string): void {
  try {
    localStorage.removeItem(key(shortId));
  } catch {
    // ignore
  }
}

const creatorKey = (shortId: string): string => `spades_creator_${shortId}`;

export function markChallengeCreator(shortId: string): void {
  try {
    sessionStorage.setItem(creatorKey(shortId), '1');
  } catch {
    // ignore
  }
}

export function isChallengeCreator(shortId: string): boolean {
  try {
    return sessionStorage.getItem(creatorKey(shortId)) === '1';
  } catch {
    return false;
  }
}

export function clearChallengeCreator(shortId: string): void {
  try {
    sessionStorage.removeItem(creatorKey(shortId));
  } catch {
    // ignore
  }
}

const OAUTH_IN_PROGRESS_KEY = 'spades_oauth_in_progress';
const OAUTH_NEXT_KEY = 'spades_oauth_next';

export function markOauthInProgress(provider: 'google' | 'github', next: string): void {
  try {
    localStorage.setItem(OAUTH_IN_PROGRESS_KEY, provider);
    localStorage.setItem(OAUTH_NEXT_KEY, next);
  } catch {
    // ignore
  }
}

export function consumeOauthInProgress(): { provider: string; next: string } | null {
  try {
    const provider = localStorage.getItem(OAUTH_IN_PROGRESS_KEY);
    const next = localStorage.getItem(OAUTH_NEXT_KEY);
    localStorage.removeItem(OAUTH_IN_PROGRESS_KEY);
    localStorage.removeItem(OAUTH_NEXT_KEY);
    if (!provider) return null;
    return { provider, next: next ?? '/' };
  } catch {
    return null;
  }
}
