import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest';
import { renderLobby } from '../../src/routes/lobby';
import type { Resources } from '../../src/routes/play-resources';
import type { ChallengeStatus } from '../../src/routes/boot';
import { seatTick, gameStart } from '../../src/lib/sound';
import { openSse } from '../../src/api/sse';

vi.mock('../../src/lib/sound', () => ({
  chime: vi.fn(),
  seatTick: vi.fn(),
  gameStart: vi.fn(),
}));
vi.mock('../../src/api/sse', () => ({
  openSse: vi.fn(() => ({ close: vi.fn() })),
}));

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

  it('renders one gauge button per team with two open slots each', () => {
    renderLobby(makeArgs());
    const btns = [...document.querySelectorAll('.team-grid .team-btn')];
    expect(btns).toHaveLength(2);
    expect(btns.map((b) => b.getAttribute('data-team'))).toEqual(['1', '2']);
    expect(btns.map((b) => b.getAttribute('data-fill'))).toEqual(['0', '0']);
    expect(document.querySelectorAll('.team-btn__slot--open')).toHaveLength(4);
  });

  it('offers both teams as joinable when open', () => {
    renderLobby(makeArgs());
    const joins = [...document.querySelectorAll<HTMLButtonElement>('.team-btn:not([disabled])')];
    expect(joins.map((b) => b.getAttribute('aria-label'))).toEqual([
      'Join Team A, 0 of 2 seats filled',
      'Join Team B, 0 of 2 seats filled',
    ]);
  });

  it('disables a full team but keeps showing its members and fill', () => {
    const args = makeArgs();
    args.initialStatus.seats = [
      { seat: 'A', player_id: 'p1', name: 'P1' },
      { seat: 'C', player_id: 'p2', name: 'P2' },
    ];
    renderLobby(args);
    const full = document.querySelector<HTMLButtonElement>('.team-btn[data-team="1"]')!;
    expect(full.disabled).toBe(true);
    expect(full.getAttribute('data-fill')).toBe('2');
    expect(full.getAttribute('aria-label')).toBe('Team A, 2 of 2 seats filled: P1, P2');
    expect(full.textContent).toContain('P1');
    expect(document.querySelector<HTMLButtonElement>('.team-btn[data-team="2"]')!.disabled).toBe(
      false,
    );
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

  it('ticks on seat fills, stays silent on leaves, flourishes on game start', () => {
    renderLobby(makeArgs());
    // Join Team A to open the SSE and capture its event handler.
    document.querySelector<HTMLButtonElement>('.team-btn[data-team="1"]')!.click();
    const nameInput = document.querySelector<HTMLInputElement>('.join-modal input')!;
    nameInput.value = 'Me';
    nameInput.dispatchEvent(new Event('input', { bubbles: true }));
    [...document.querySelectorAll<HTMLButtonElement>('.join-modal .btn')]
      .find((b) => b.textContent?.trim() === 'Join')!
      .click();

    const sseOpts = vi.mocked(openSse).mock.calls.at(-1)![2];
    const seatUpdate = (seats: unknown) =>
      sseOpts.onEvent('seat_update', JSON.stringify({ seats }));

    seatUpdate([{ seat: 'A', player_id: 'p1', name: 'Me' }]);
    expect(seatTick).toHaveBeenCalledWith(1);

    seatUpdate([
      { seat: 'A', player_id: 'p1', name: 'Me' },
      { seat: 'B', player_id: 'p2', name: 'Ada' },
    ]);
    expect(seatTick).toHaveBeenCalledWith(2);
    // announce() writes to its shared polite live region -- the tick's
    // screen-reader twin.
    const liveTexts = [...document.querySelectorAll('[role="status"]')].map((el) => el.textContent);
    expect(liveTexts).toContain('Ada joined Team B');

    // A leave: count decreases, no new tick.
    vi.mocked(seatTick).mockClear();
    seatUpdate([{ seat: 'A', player_id: 'p1', name: 'Me' }]);
    expect(seatTick).not.toHaveBeenCalled();

    sseOpts.onEvent('game_start', JSON.stringify({ game_id: 'g1', player_id: 'p1' }));
    expect(gameStart).toHaveBeenCalledTimes(1);
  });
});
