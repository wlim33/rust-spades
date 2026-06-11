import { describe, it, expect, beforeEach } from 'vitest';
import { createFront, createBack, setFront } from '../../src/cards/card-el';

describe('card-el', () => {
  beforeEach(() => {
    document.body.innerHTML = '';
  });

  it('renders a front card as a typographic face with corner index + aria-label', () => {
    const el = createFront({ suit: 'Heart', rank: 'Ace' });
    expect(el.querySelector('.card-corner-rank')?.textContent).toBe('A');
    expect(el.querySelector('.card-corner-suit')?.textContent).toBe('♥');
    expect(el.className).toContain('card-front');
    expect(el.className).toContain('suit-red');
    expect(el.getAttribute('aria-label')).toBe('Ace of Hearts');
    expect(el.getAttribute('role')).toBe('button');
  });

  it('back card has card-back class and no face markup', () => {
    const el = createBack();
    expect(el.className).toContain('card-back');
    expect(el.querySelector('.card-corner')).toBeNull();
  });

  it('setFront swaps the face + aria-label on an existing element', () => {
    const el = createBack();
    setFront(el, { suit: 'Diamond', rank: 'Two' });
    expect(el.querySelector('.card-corner-rank')?.textContent).toBe('2');
    expect(el.querySelector('.card-corner-suit')?.textContent).toBe('♦');
    expect(el.getAttribute('aria-label')).toBe('Two of Diamonds');
  });
});
