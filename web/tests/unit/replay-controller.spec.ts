import { describe, it, expect } from 'vitest';
import { tnCardToApp } from '../../src/replay/types';

describe('tnCardToApp', () => {
  it('maps single-char syms to app card', () => {
    expect(tnCardToApp({ suit: 'S', rank: 'A' })).toEqual({ suit: 'Spade', rank: 'Ace' });
    expect(tnCardToApp({ suit: 'C', rank: 'T' })).toEqual({ suit: 'Club', rank: 'Ten' });
    expect(tnCardToApp({ suit: 'H', rank: '2' })).toEqual({ suit: 'Heart', rank: 'Two' });
    expect(tnCardToApp({ suit: 'D', rank: 'K' })).toEqual({ suit: 'Diamond', rank: 'King' });
  });

  it('throws on unmappable card', () => {
    expect(() => tnCardToApp({ suit: 'X', rank: 'A' })).toThrow('unmappable card');
    expect(() => tnCardToApp({ suit: 'S', rank: 'Z' })).toThrow('unmappable card');
  });
});
