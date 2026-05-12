import { describe, it, expect, beforeEach } from 'vitest';
import { TrickManager } from '../../src/cards/trick-manager';
import type { Card } from '../../src/state/helpers';

const c = (suit: Card['suit'], rank: Card['rank']): Card => ({ suit, rank });

describe('TrickManager', () => {
  let container: HTMLDivElement;
  let tm: TrickManager;

  beforeEach(() => {
    document.body.innerHTML = '<div id="trick"></div>';
    container = document.getElementById('trick') as HTMLDivElement;
    tm = new TrickManager();
    tm.init(container);
  });

  it('initializes with 4 placeholder slots', () => {
    expect(container.children.length).toBe(4);
    for (const child of container.children) {
      expect(child.className).toContain('trick-placeholder');
    }
  });

  it('fillNextSlot replaces a placeholder with a faced card', () => {
    tm.fillNextSlot(c('Heart', 'Ace'), 'south');
    expect(container.children[0]!.textContent).toBe('A♥');
    expect(container.children[0]!.className).not.toContain('trick-placeholder');
    expect(tm.count()).toBe(1);
  });

  it('clear() resets back to 4 placeholders', () => {
    tm.fillNextSlot(c('Heart', 'Ace'), 'south');
    tm.clear();
    expect(container.children.length).toBe(4);
    expect(tm.count()).toBe(0);
  });

  it('slots() returns the live list', () => {
    tm.fillNextSlot(c('Spade', 'King'), 'east');
    const slots = tm.slots();
    expect(slots[0]!.card).toEqual(c('Spade', 'King'));
    expect(slots[0]!.seat).toBe('east');
  });
});
