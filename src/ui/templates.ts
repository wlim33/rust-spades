import { html, type TemplateResult } from 'lit-html';
import { header } from './components/header';
import { footer } from './components/footer';
import { toastStack } from './components/toast';

export function appShell(children: TemplateResult): TemplateResult {
  return html`<div class="app-shell">
    ${header()}
    <section class="page">${children}</section>
    ${footer()} ${toastStack()}
  </div>`;
}
