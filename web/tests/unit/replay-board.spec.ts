// @vitest-environment happy-dom
import { describe, it, expect, beforeEach } from 'vitest';
import { ReplayBoard } from '../../src/replay/board';
import type { Containers } from '../../src/cards/hand-manager';
import type { ViewState } from '../../src/replay/controller';

function makeContainers(): Containers {
  const mk = () => document.createElement('div');
  return { south: mk(), west: mk(), north: mk(), east: mk(), trick: mk() };
}

function makeViewState(overrides: Partial<ViewState> = {}): ViewState {
  return {
    round: 1,
    totalRounds: 1,
    toAct: null,
    hands: {
      south: [{ suit: 'Spade', rank: 'Ace' }],
      north: [{ suit: 'Heart', rank: 'King' }],
      east: [{ suit: 'Diamond', rank: 'Queen' }],
      west: [{ suit: 'Club', rank: 'Jack' }],
    },
    trick: [],
    trickWinner: null,
    bids: { south: 3, north: 2, east: 1, west: 4 },
    tricksWon: { south: 0, north: 0, east: 0, west: 0 },
    score: [0, 0],
    phase: 'play',
    aborted: false,
    ...overrides,
  };
}

describe('ReplayBoard', () => {
  let containers: Containers;

  beforeEach(() => {
    containers = makeContainers();
  });

  it('renders all four hands face-up', async () => {
    const board = new ReplayBoard(containers);
    const vs = makeViewState();
    await board.render(null, vs, { animate: false });
    expect(containers.south.querySelectorAll('.card-front').length).toBeGreaterThan(0);
    expect(containers.north.querySelectorAll('.card-front').length).toBeGreaterThan(0);
    expect(containers.east.querySelectorAll('.card-front').length).toBeGreaterThan(0);
    expect(containers.west.querySelectorAll('.card-front').length).toBeGreaterThan(0);
  });

  it('places trick cards in the trick container', async () => {
    const board = new ReplayBoard(containers);
    const vs = makeViewState({
      hands: {
        south: [{ suit: 'Spade', rank: 'Two' }],
        north: [{ suit: 'Heart', rank: 'Two' }],
        east: [],
        west: [],
      },
      trick: [
        { seat: 'east', card: { suit: 'Diamond', rank: 'Queen' } },
        { seat: 'west', card: { suit: 'Club', rank: 'Jack' } },
      ],
    });
    await board.render(null, vs, { animate: false });
    expect(containers.trick.querySelectorAll('.card-front').length).toBe(2);
  });

  it('clear() empties all five containers', async () => {
    const board = new ReplayBoard(containers);
    const vs = makeViewState();
    await board.render(null, vs, { animate: false });
    board.clear();
    expect(containers.south.children.length).toBe(0);
    expect(containers.north.children.length).toBe(0);
    expect(containers.east.children.length).toBe(0);
    expect(containers.west.children.length).toBe(0);
    expect(containers.trick.children.length).toBe(0);
  });

  it('re-render with animate:false replaces prior state', async () => {
    const board = new ReplayBoard(containers);
    const vs1 = makeViewState();
    await board.render(null, vs1, { animate: false });
    const vs2 = makeViewState({
      hands: {
        south: [
          { suit: 'Spade', rank: 'Ace' },
          { suit: 'Spade', rank: 'King' },
        ],
        north: [{ suit: 'Heart', rank: 'King' }],
        east: [{ suit: 'Diamond', rank: 'Queen' }],
        west: [{ suit: 'Club', rank: 'Jack' }],
      },
    });
    await board.render(vs1, vs2, { animate: false });
    // south gained one card
    expect(containers.south.querySelectorAll('.card-front').length).toBe(2);
  });

  it('marks trickWinner cards with the winner class', async () => {
    const board = new ReplayBoard(containers);
    const vs = makeViewState({
      trick: [
        { seat: 'south', card: { suit: 'Spade', rank: 'Ace' } },
        { seat: 'north', card: { suit: 'Spade', rank: 'Two' } },
      ],
      trickWinner: 'south',
    });
    await board.render(null, vs, { animate: false });
    // The winner's trick card should carry the winner class
    const winnerCards = containers.trick.querySelectorAll('.replay-trick-winner');
    expect(winnerCards.length).toBe(1);
  });
});
