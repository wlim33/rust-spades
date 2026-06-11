import { describe, it, expect, beforeEach } from 'vitest';
import { CardOrchestrator } from '../../src/cards/orchestrator';
import type { Card } from '../../src/state/helpers';

const c = (suit: Card['suit'], rank: Card['rank']): Card => ({ suit, rank });

describe('CardOrchestrator animation queue', () => {
  let orch: CardOrchestrator;
  let north: HTMLDivElement;
  let trick: HTMLDivElement;

  beforeEach(() => {
    document.body.innerHTML =
      '<div id="s"></div><div id="n"></div><div id="w"></div><div id="e"></div><div id="t"></div>';
    const get = (id: string): HTMLDivElement => document.getElementById(id) as HTMLDivElement;
    north = get('n');
    trick = get('t');
    orch = new CardOrchestrator({
      containers: { south: get('s'), north, west: get('w'), east: get('e'), trick },
    });
    orch.setupImmediate({
      playerHand: [c('Spade', 'Ace'), c('Heart', 'Two')],
      oppCounts: { north: 3, west: 3, east: 3 },
      tableCards: [null, null, null, null],
      myIdx: 0,
      northIdx: 2,
      westIdx: 3,
      eastIdx: 1,
      currentPlayerSeatIdx: 0,
    });
  });

  it('runs visual steps sequentially, not at call time', async () => {
    orch.playOpponentCardToCenter(c('Club', 'Five'), 'east');
    orch.updateOpponentCount('north', 1);

    // Nothing has executed synchronously: steps are queued.
    expect(north.children.length).toBe(3);
    expect(trick.querySelectorAll('.card-front').length).toBe(0);

    await orch.whenIdle();
    expect(trick.querySelectorAll('.card-front').length).toBe(1);
    expect(north.children.length).toBe(1);
  });

  it('completeTrick backfills only the plays the queue has not rendered, then collects', async () => {
    orch.playOpponentCardToCenter(c('Club', 'Five'), 'east');
    orch.completeTrick(
      [
        { card: c('Club', 'Five'), seat: 'east' },
        { card: c('Club', 'Six'), seat: 'north' },
        { card: c('Club', 'Seven'), seat: 'west' },
        { card: c('Club', 'Ace'), seat: 'south' },
      ],
      'south',
    );

    await orch.whenIdle();
    // Collected: back to 4 empty placeholders, no duplicated east card stuck.
    expect(trick.querySelectorAll('.card-front').length).toBe(0);
    expect(trick.querySelectorAll('.trick-placeholder').length).toBe(4);
  });

  it('runs the play submission inside the queued step, before later steps', async () => {
    let northCountAtSubmit = -1;
    void orch.playCardToCenter(c('Spade', 'Ace'), async () => {
      northCountAtSubmit = north.children.length;
    });
    orch.updateOpponentCount('north', 1);
    await orch.whenIdle();
    // The count update queued after the play must not have run when the
    // submission executed — later steps are ordered after the request.
    expect(northCountAtSubmit).toBe(3);
    expect(north.children.length).toBe(1);
  });

  it('clearAll cancels steps that are still queued', async () => {
    orch.updateOpponentCount('north', 1);
    orch.clearAll();
    await orch.whenIdle();
    // clearAll emptied everything and the queued count-update did not resurrect backs.
    expect(north.children.length).toBe(0);
  });
});
