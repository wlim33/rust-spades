import { html, type TemplateResult } from 'lit-html';
import { header } from './components/header';

export function appShell(children: TemplateResult): TemplateResult {
  return html`${header()}
    <section class="page">${children}</section>`;
}
