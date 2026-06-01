import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest';
import { renderLobby } from '../../src/routes/lobby';
import type { Resources } from '../../src/routes/play-resources';
import type { ChallengeStatus } from '../../src/routes/boot';

function makeArgs() {
  const resources: Resources = { cleanups: [], ws: null, pollTimer: null, orchestrator: null };
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

  it('tags every seat with its team', () => {
    renderLobby(makeArgs());
    // empty seats with no session render as joinable buttons, one per seat
    expect(document.querySelectorAll('.seat-grid [data-team]')).toHaveLength(4);
    expect(document.querySelectorAll('.seat-grid [data-team="1"]')).toHaveLength(2);
    expect(document.querySelectorAll('.seat-grid [data-team="2"]')).toHaveLength(2);
  });

  it('shows "Copied!" after a successful copy', async () => {
    const writeText = vi.fn().mockResolvedValue(undefined);
    vi.stubGlobal('navigator', { clipboard: { writeText } });
    renderLobby(makeArgs());
    const copyBtn = [...document.querySelectorAll('button')].find(
      (b) => b.textContent?.trim() === 'Copy',
    )!;
    copyBtn.click();
    await new Promise((r) => setTimeout(r, 0));
    expect(writeText).toHaveBeenCalledWith(expect.stringContaining('/play/abc123'));
    const copied = [...document.querySelectorAll('button')].find(
      (b) => b.textContent?.trim() === 'Copied!',
    );
    expect(copied).toBeTruthy();
  });
});
