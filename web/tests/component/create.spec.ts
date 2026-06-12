import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest';
import { create } from '../../src/routes/create';

describe('create route', () => {
  beforeEach(() => {
    document.body.innerHTML = '<main id="root"></main>';
  });
  afterEach(() => vi.restoreAllMocks());

  it('renders three segmented-control groups', () => {
    const cleanup = create.render({}, { path: '/create', search: new URLSearchParams() });
    expect(document.querySelectorAll('.seg')).toHaveLength(3);
    cleanup();
  });

  it('marks the default team (A), points (500) and timer (None) segments pressed', () => {
    const cleanup = create.render({}, { path: '/create', search: new URLSearchParams() });
    const pressed = [...document.querySelectorAll('.seg button[aria-pressed="true"]')].map((b) =>
      b.textContent?.trim(),
    );
    expect(pressed).toContain('Team A');
    expect(pressed).toContain('500');
    expect(pressed).toContain('None');
    cleanup();
  });

  it('clicking Team B moves aria-pressed to it', () => {
    const cleanup = create.render({}, { path: '/create', search: new URLSearchParams() });
    const teamSeg = document.querySelector('.seg[aria-label="Team"]')!;
    teamSeg.querySelectorAll('button')[1]!.click(); // 'Team B'
    expect(teamSeg.querySelector('button[aria-pressed="true"]')?.textContent?.trim()).toBe(
      'Team B',
    );
    // Exactly one team is always selected.
    expect(teamSeg.querySelectorAll('button[aria-pressed="true"]')).toHaveLength(1);
    cleanup();
  });

  it('keeps a button named exactly "Create"', () => {
    const cleanup = create.render({}, { path: '/create', search: new URLSearchParams() });
    const createBtn = [...document.querySelectorAll('button')].find(
      (b) => b.textContent?.trim() === 'Create',
    );
    expect(createBtn).toBeTruthy();
    cleanup();
  });
});
