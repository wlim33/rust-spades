import { html, type TemplateResult } from 'lit-html';
import { header } from './components/header';
import { footer } from './components/footer';
import { toastStack } from './components/toast';

export type AppShellOpts = {
  /** Game screens: page fills 100dvh, no footer, no page scroll. */
  fit?: boolean;
};

export function appShell(children: TemplateResult, opts: AppShellOpts = {}): TemplateResult {
  return html`<div class="app-shell">
    ${header()}
    <section class="page${opts.fit ? ' page--fit' : ''}">${children}</section>
    ${opts.fit ? null : footer()} ${toastStack()}
  </div>`;
}
