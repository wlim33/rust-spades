import { describe, it, expect, beforeEach } from 'vitest';
import { render } from 'lit-html';
import { scores } from '../../src/ui/components/scores';

const base = {
  teamAScore: 127,
  teamBScore: 94,
  teamABags: 3,
  teamBBags: 1,
  centerText: '',
};

describe('scores placard', () => {
  beforeEach(() => {
    document.body.innerHTML = '<main id="root"></main>';
  });

  it('renders both team blocks with scores and bag counts', () => {
    render(scores(base), document.getElementById('root')!);
    const teams = document.querySelectorAll('.spades-scoreboard__team');
    expect(teams).toHaveLength(2);
    expect(teams[0]!.textContent).toContain('127');
    expect(teams[0]!.textContent).toContain('3');
    expect(teams[1]!.textContent).toContain('94');
    expect(teams[1]!.textContent).toContain('1');
    expect(document.querySelector('section[aria-label="Scores"]')).not.toBeNull();
  });

  it('labels the teams plainly, with no (You) marker', () => {
    render(scores(base), document.getElementById('root')!);
    const labels = document.querySelectorAll('.spades-scoreboard__label');
    expect(labels[0]!.textContent).toBe('Team A');
    expect(labels[1]!.textContent).toBe('Team B');
  });

  it('replaces the Bags word with a labeled bag glyph per team', () => {
    render(scores(base), document.getElementById('root')!);
    const glyphs = document.querySelectorAll('.spades-scoreboard__nums .icon[aria-label="Bags"]');
    expect(glyphs).toHaveLength(2);
    expect(glyphs[0]!.getAttribute('role')).toBe('img');
    expect(document.querySelector('.spades-scoreboard')!.textContent).not.toContain('Bags');
    const nums = document.querySelector('.spades-scoreboard__nums')!;
    expect(nums.textContent!.replace(/\s+/g, ' ').trim()).toBe('127 · 3');
  });

  it('renders center text only when provided', () => {
    render(scores(base), document.getElementById('root')!);
    expect(document.querySelector('.spades-scoreboard__center')).toBeNull();
    document.body.innerHTML = '<main id="root"></main>';
    render(scores({ ...base, centerText: 'Trick 7' }), document.getElementById('root')!);
    expect(document.querySelector('.spades-scoreboard__center')!.textContent).toBe('Trick 7');
  });
});
