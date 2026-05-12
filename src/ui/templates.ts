import { html, type TemplateResult } from 'lit-html';
import { header } from './components/header';
import { toastStack } from './components/toast';

export function appShell(children: TemplateResult): TemplateResult {
  return html`<div class="app-shell">
    ${header()}
    <section class="page">${children}</section>
    ${toastStack()}
  </div>`;
}
