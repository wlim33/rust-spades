import { html, type TemplateResult } from 'lit-html';

export function authCard(opts: { title: string; children: TemplateResult }): TemplateResult {
  return html`<section class="auth-card" data-testid="auth-card">
    <div class="auth-card__brand"><span class="auth-card__pip">♠</span> Spades</div>
    <h2>${opts.title}</h2>
    ${opts.children}
  </section>`;
}
