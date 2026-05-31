import { html, type TemplateResult } from 'lit-html';

export function bidBar(opts: { onBet: (amount: number) => void }): TemplateResult {
  return html`<div class="spades-bets">
    ${Array.from(
      { length: 14 },
      (_, n) =>
        html`<button
          type="button"
          class="spades-bet${n === 0 ? ' spades-bet--nil' : ''}"
          @click=${() => opts.onBet(n)}
        >
          ${n === 0 ? 'Nil' : n}
        </button>`,
    )}
  </div>`;
}
