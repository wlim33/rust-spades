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
