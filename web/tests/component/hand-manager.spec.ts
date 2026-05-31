import { describe, it, expect, beforeEach } from 'vitest';
import { HandManager } from '../../src/cards/hand-manager';
import type { Card } from '../../src/state/helpers';

const c = (suit: Card['suit'], rank: Card['rank']): Card => ({ suit, rank });

describe('HandManager', () => {
  let south: HTMLDivElement;
  let north: HTMLDivElement;
  let hm: HandManager;

  beforeEach(() => {
    document.body.innerHTML = '<div id="s"></div><div id="n"></div>';
    south = document.getElementById('s') as HTMLDivElement;
    north = document.getElementById('n') as HTMLDivElement;
    hm = new HandManager();
    hm.setContainers({ south, north, west: south, east: south, trick: south });
  });

  it('mounts the player hand sorted by suit/rank', () => {
    hm.setPlayerHand([c('Heart', 'Two'), c('Spade', 'Ace')]);
    expect(south.children.length).toBe(2);
    expect(
      (south.children[0]!.querySelector('img.card-face') as HTMLImageElement).getAttribute('src'),
    ).toBe('/cards/AS.svg');
    expect(
      (south.children[1]!.querySelector('img.card-face') as HTMLImageElement).getAttribute('src'),
    ).toBe('/cards/2H.svg');
  });

  it('replacing the hand reuses kept entries and unmounts removed', () => {
    hm.setPlayerHand([c('Heart', 'Two'), c('Spade', 'Ace')]);
    const aceEl = south.children[0];
    hm.setPlayerHand([c('Spade', 'Ace')]);
    expect(south.children.length).toBe(1);
    expect(south.children[0]).toBe(aceEl); // same DOM node reused
  });

  it('mounts opponent backs by count', () => {
    hm.setOpponentCount('north', 13);
    expect(north.children.length).toBe(13);
    expect(north.children[0]!.className).toContain('card-back');
  });

  it('reducing opponent count unmounts the tail', () => {
    hm.setOpponentCount('north', 3);
    hm.setOpponentCount('north', 1);
    expect(north.children.length).toBe(1);
  });

  it('removeCard removes a specific player card and returns its element', () => {
    hm.setPlayerHand([c('Heart', 'Two'), c('Spade', 'Ace')]);
    const removed = hm.removeCard(c('Spade', 'Ace'));
    expect((removed?.querySelector('img.card-face') as HTMLImageElement).getAttribute('src')).toBe(
      '/cards/AS.svg',
    );
    expect(south.children.length).toBe(1);
  });

  it('clear empties everything', () => {
    hm.setPlayerHand([c('Spade', 'Ace')]);
    hm.setOpponentCount('north', 3);
    hm.clear();
    expect(south.children.length).toBe(0);
    expect(north.children.length).toBe(0);
  });

  it('cards() returns the live entries for the player', () => {
    hm.setPlayerHand([c('Spade', 'Ace')]);
    const entries = hm.cards('south');
    expect(entries.length).toBe(1);
    expect(entries[0]!.card).toEqual(c('Spade', 'Ace'));
  });
});
