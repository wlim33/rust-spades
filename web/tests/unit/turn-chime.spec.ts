import { describe, it, expect } from 'vitest';
import { becameMyTurn } from '../../src/state/helpers';

describe('becameMyTurn', () => {
  it('fires on the edge where the turn becomes mine', () => {
    expect(becameMyTurn('p2', 'p1', 'p1', 'PLAYING')).toBe(true);
    expect(becameMyTurn(null, 'p1', 'p1', 'BETTING')).toBe(true);
  });

  it('does not fire while the turn stays mine', () => {
    expect(becameMyTurn('p1', 'p1', 'p1', 'PLAYING')).toBe(false);
  });

  it('does not fire for someone else or outside turn phases', () => {
    expect(becameMyTurn('p1', 'p2', 'p1', 'PLAYING')).toBe(false);
    expect(becameMyTurn('p2', 'p1', 'p1', 'GAME_OVER')).toBe(false);
    expect(becameMyTurn('p2', 'p1', 'p1', 'LOBBY')).toBe(false);
  });

  it('never fires with an empty player id', () => {
    expect(becameMyTurn(null, '', '', 'PLAYING')).toBe(false);
  });
});
