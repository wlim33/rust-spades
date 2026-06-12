import { describe, it, expect, beforeEach, vi } from 'vitest';
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
    expect(south.children[0]!.getAttribute('aria-label')).toBe('Ace of Spades');
    expect(south.children[1]!.getAttribute('aria-label')).toBe('Two of Hearts');
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
    expect(removed?.getAttribute('aria-label')).toBe('Ace of Spades');
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

  it('sets --hand-ml on the south container as the hand changes', () => {
    hm.setPlayerHand([c('Spade', 'Ace'), c('Spade', 'King')]);
    // happy-dom reports zero widths: 2 cards clamp to full compression (-22px)
    expect(south.style.getPropertyValue('--hand-ml')).toBe('-22px');
    hm.setPlayerHand([c('Spade', 'Ace')]);
    expect(south.style.getPropertyValue('--hand-ml')).toBe('0px');
  });

  it('updates --hand-ml when a card is removed via removeCard', () => {
    hm.setPlayerHand([c('Spade', 'Ace'), c('Spade', 'King')]);
    hm.removeCard(c('Spade', 'Ace'));
    expect(south.style.getPropertyValue('--hand-ml')).toBe('0px');
  });

  it('publishes --fan-mt vertical overlap on side-fan count changes', () => {
    document.body.innerHTML = '<div id="s2"></div><div id="w2"></div><div id="e2"></div>';
    const s2 = document.getElementById('s2') as HTMLDivElement;
    const w2 = document.getElementById('w2') as HTMLDivElement;
    const e2 = document.getElementById('e2') as HTMLDivElement;
    const hm2 = new HandManager();
    hm2.setContainers({ south: s2, north: s2, west: w2, east: e2, trick: s2 });
    hm2.setOpponentCount('west', 13);
    // happy-dom: clientHeight/offsetHeight are 0 -> full compression at the
    // 4px strip with the 64px fallback card height: -(64 - 4) = -60.
    expect(w2.style.getPropertyValue('--fan-mt')).toBe('-60px');
    expect(e2.style.getPropertyValue('--fan-mt')).toBe('');
    hm2.setOpponentCount('east', 5);
    expect(e2.style.getPropertyValue('--fan-mt')).toBe('-60px');
  });
  it('keeps observing through clear() and disconnects only on dispose()', () => {
    const calls: string[] = [];
    class FakeRO {
      constructor(_cb: ResizeObserverCallback) {}
      observe(): void {
        calls.push('observe');
      }
      disconnect(): void {
        calls.push('disconnect');
      }
      unobserve(): void {}
    }
    vi.stubGlobal('ResizeObserver', FakeRO);
    try {
      const m = new HandManager();
      m.setContainers({ south, north, west: south, east: south, trick: south });
      // south + west + east are observed (the fans all spread adaptively)
      expect(calls).toEqual(['observe', 'observe', 'observe']);
      m.clear(); // mid-game reset (every orchestrator setup) must not kill the observer
      expect(calls).toEqual(['observe', 'observe', 'observe']);
      m.dispose();
      expect(calls).toEqual(['observe', 'observe', 'observe', 'disconnect']);
    } finally {
      vi.unstubAllGlobals();
    }
  });
});
