import { describe, it, expect } from 'vitest';
import { cardFaceUrl } from '../../src/cards/card-el';

describe('cardFaceUrl', () => {
  it('maps Ten of Hearts to /cards/TH.svg', () => {
    expect(cardFaceUrl({ rank: 'Ten', suit: 'Heart' })).toBe('/cards/TH.svg');
  });
  it('maps Ace of Spades to /cards/AS.svg', () => {
    expect(cardFaceUrl({ rank: 'Ace', suit: 'Spade' })).toBe('/cards/AS.svg');
  });
  it('maps number + court ranks and all suits', () => {
    expect(cardFaceUrl({ rank: 'Two', suit: 'Club' })).toBe('/cards/2C.svg');
    expect(cardFaceUrl({ rank: 'King', suit: 'Diamond' })).toBe('/cards/KD.svg');
    expect(cardFaceUrl({ rank: 'Jack', suit: 'Heart' })).toBe('/cards/JH.svg');
  });
});
