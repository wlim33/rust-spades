import { html, type TemplateResult } from 'lit-html';

export function header(): TemplateResult {
  return html`<header class="site-header">
    <a class="site-title" href="/" data-link>Spades</a>
    <nav class="site-nav">
      <!-- sign-in slot (filled in Plan 3) -->
    </nav>
  </header>`;
}
