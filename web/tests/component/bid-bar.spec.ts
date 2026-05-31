import { describe, it, expect, beforeEach, vi } from 'vitest';
import { render } from 'lit-html';
import { bidBar } from '../../src/ui/components/bid-bar';

describe('bidBar', () => {
  beforeEach(() => {
    document.body.innerHTML = '<main id="root"></main>';
  });

  it('renders Nil + 1..13 (14 buttons)', () => {
    render(bidBar({ onBet: () => {} }), document.getElementById('root')!);
    const btns = document.querySelectorAll('.spades-bet');
    expect(btns.length).toBe(14);
    expect(btns[0]!.textContent?.trim()).toBe('Nil');
    expect(btns[13]!.textContent?.trim()).toBe('13');
  });

  it('calls onBet with the chosen amount (Nil = 0)', () => {
    const onBet = vi.fn();
    render(bidBar({ onBet }), document.getElementById('root')!);
    (document.querySelector('.spades-bet') as HTMLButtonElement).click();
    expect(onBet).toHaveBeenCalledWith(0);
  });
});
