import { html, type TemplateResult } from 'lit-html';
import { header } from './components/header';

export function appShell(children: TemplateResult): TemplateResult {
  return html`<div class="app-shell">
    ${header()}
    <section class="page">${children}</section>
  </div>`;
}
