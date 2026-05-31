import { describe, it, expect, beforeEach } from 'vitest';
import { createFront, createBack, setFront } from '../../src/cards/card-el';

describe('card-el', () => {
  beforeEach(() => {
    document.body.innerHTML = '';
  });

  it('renders a front card as an image face with the right src + aria-label', () => {
    const el = createFront({ suit: 'Heart', rank: 'Ace' });
    const img = el.querySelector('img.card-face') as HTMLImageElement;
    expect(img).not.toBeNull();
    expect(img.getAttribute('src')).toBe('/cards/AH.svg');
    expect(el.className).toContain('card-front');
    expect(el.getAttribute('aria-label')).toBe('Ace of Hearts');
    expect(el.getAttribute('role')).toBe('button');
  });

  it('back card has card-back class and no face image', () => {
    const el = createBack();
    expect(el.className).toContain('card-back');
    expect(el.querySelector('img')).toBeNull();
  });

  it('setFront swaps the face image + aria-label on an existing element', () => {
    const el = createBack();
    setFront(el, { suit: 'Diamond', rank: 'Two' });
    expect((el.querySelector('img.card-face') as HTMLImageElement).getAttribute('src')).toBe('/cards/2D.svg');
    expect(el.getAttribute('aria-label')).toBe('Two of Diamonds');
  });
});
