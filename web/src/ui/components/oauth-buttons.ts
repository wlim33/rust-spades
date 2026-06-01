import { html, type TemplateResult } from 'lit-html';
import { session } from '../../state/session';
import { icon } from '../icon';

export function oauthButtons(opts: { next: string }): TemplateResult {
  // Single root element: a multi-root template returned from a nested `${fn()}`
  // binding does not render reliably across renderers, so wrap the group.
  return html`<div class="auth-oauth">
    <div class="auth-divider">or</div>
    <button
      class="btn btn--secondary btn--block"
      type="button"
      @click=${() => session.startOauth('google', opts.next)}
    >
      ${icon('google-fill')} Continue with Google
    </button>
    <button
      class="btn btn--secondary btn--block"
      type="button"
      @click=${() => session.startOauth('github', opts.next)}
    >
      ${icon('github-fill')} Continue with GitHub
    </button>
  </div>`;
}
