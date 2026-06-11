import { html, type TemplateResult } from 'lit-html';

export function footer(): TemplateResult {
  return html`<footer class="site-footer">
    <span>rust-spades</span>
    <span>·</span>
    <a href="https://github.com/wlim33/rust-spades" target="_blank" rel="noopener noreferrer"
      >source</a
    >
    <span>·</span>
    <span class="footer-version">${__BUILD_VERSION__}</span>
  </footer>`;
}
