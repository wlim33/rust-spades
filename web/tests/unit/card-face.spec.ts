// @vitest-environment happy-dom
import { describe, it, expect } from 'vitest';
import { createFront, setCardFace, type CardEl } from '../../src/cards/card-el';

describe('card faces', () => {
  it('renders the corner index with rank and suit glyph', () => {
    const el = createFront({ rank: 'Ten', suit: 'Heart' });
    expect(el.querySelector('.card-corner-rank')?.textContent).toBe('10');
    expect(el.querySelector('.card-corner-suit')?.textContent).toBe('♥');
    expect(el.getAttribute('aria-label')).toBe('Ten of Hearts');
  });

  it('marks red and black suits for CSS color', () => {
    expect(createFront({ rank: 'Ace', suit: 'Diamond' }).classList.contains('suit-red')).toBe(true);
    expect(createFront({ rank: 'Ace', suit: 'Spade' }).classList.contains('suit-black')).toBe(true);
    expect(createFront({ rank: 'Two', suit: 'Club' }).classList.contains('suit-black')).toBe(true);
  });

  it('renders a center pip matching the suit, hidden from a11y', () => {
    const el = createFront({ rank: 'King', suit: 'Club' });
    const pip = el.querySelector('.card-pip');
    expect(pip?.textContent).toBe('♣');
    expect(pip?.getAttribute('aria-hidden')).toBe('true');
  });

  it('setCardFace replaces a previous face entirely', () => {
    const el = createFront({ rank: 'Two', suit: 'Club' });
    setCardFace(el as CardEl, { rank: 'Queen', suit: 'Heart' });
    expect(el.querySelectorAll('.card-corner').length).toBe(1);
    expect(el.querySelector('.card-corner-rank')?.textContent).toBe('Q');
    expect(el.classList.contains('suit-red')).toBe(true);
  });
});
