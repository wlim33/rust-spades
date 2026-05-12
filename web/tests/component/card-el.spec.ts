import { describe, it, expect, beforeEach } from 'vitest';
import { createFront, createBack, setFront } from '../../src/cards/card-el';

describe('card-el', () => {
  beforeEach(() => {
    document.body.innerHTML = '';
  });

  it('renders a red heart Ace with red color class', () => {
    const el = createFront({ suit: 'Heart', rank: 'Ace' });
    expect(el.className).toContain('card-red');
    expect(el.textContent).toBe('A♥');
  });

  it('renders a black spade 10 with black color class', () => {
    const el = createFront({ suit: 'Spade', rank: 'Ten' });
    expect(el.className).toContain('card-black');
    expect(el.textContent).toBe('10♠');
  });

  it('back card has card-back class and no text', () => {
    const el = createBack();
    expect(el.className).toContain('card-back');
    expect(el.textContent).toBe('');
  });

  it('setFront mutates an existing element', () => {
    const el = createBack();
    setFront(el, { suit: 'Diamond', rank: 'Two' });
    expect(el.className).toContain('card-red');
    expect(el.textContent).toBe('2♦');
  });
});
