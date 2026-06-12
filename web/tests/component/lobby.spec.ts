import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest';
import { renderLobby } from '../../src/routes/lobby';
import type { Resources } from '../../src/routes/play-resources';
import type { ChallengeStatus } from '../../src/routes/boot';

function makeArgs() {
  const resources: Resources = { cleanups: [], ws: null, poller: null, orchestrator: null };
  const initialStatus: ChallengeStatus = {
    challenge_id: 'chal-1',
    max_points: 500,
    seats: [],
    status: 'open',
    expires_at_epoch_secs: 0,
  };
  return {
    root: document.getElementById('root')!,
    resources,
    shortId: 'abc123',
    challengeId: 'chal-1',
    initialStatus,
  };
}

describe('lobby route', () => {
  beforeEach(() => {
    document.body.innerHTML = '<main id="root"></main>';
  });
  afterEach(() => {
    vi.restoreAllMocks();
    vi.unstubAllGlobals();
  });

  it('renders one card per team with two open slots each', () => {
    renderLobby(makeArgs());
    expect(document.querySelectorAll('.team-grid .team-card')).toHaveLength(2);
    expect(document.querySelectorAll('.team-grid [data-team="1"]')).toHaveLength(1);
    expect(document.querySelectorAll('.team-grid [data-team="2"]')).toHaveLength(1);
    expect(document.querySelectorAll('.team-card__open')).toHaveLength(4);
  });

  it('offers both teams when open, with Team A as the primary default', () => {
    renderLobby(makeArgs());
    const joins = [...document.querySelectorAll('.team-card .btn')];
    expect(joins.map((b) => b.textContent?.trim())).toEqual(['Join Team A', 'Join Team B']);
    expect(joins[0]!.classList.contains('btn--primary')).toBe(true);
    expect(joins[1]!.classList.contains('btn--secondary')).toBe(true);
  });

  it('hides a full team’s join option', () => {
    const args = makeArgs();
    args.initialStatus.seats = [
      { seat: 'A', player_id: 'p1', name: 'P1' },
      { seat: 'C', player_id: 'p2', name: 'P2' },
    ];
    renderLobby(args);
    const joins = [...document.querySelectorAll('.team-card .btn')];
    expect(joins.map((b) => b.textContent?.trim())).toEqual(['Join Team B']);
    // With Team A full, the remaining option becomes the default.
    expect(joins[0]!.classList.contains('btn--primary')).toBe(true);
  });

  it('confirms a successful copy on the icon button', async () => {
    const writeText = vi.fn().mockResolvedValue(undefined);
    vi.stubGlobal('navigator', { clipboard: { writeText } });
    renderLobby(makeArgs());
    const copyBtn = document.querySelector<HTMLButtonElement>(
      '.share-link button[aria-label="Copy link"]',
    )!;
    copyBtn.click();
    await new Promise((r) => setTimeout(r, 0));
    expect(writeText).toHaveBeenCalledWith(expect.stringContaining('/play/abc123'));
    expect(document.querySelector('.share-link button[aria-label="Copied"]')).toBeTruthy();
  });
});
