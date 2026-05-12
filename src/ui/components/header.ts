import { html, type TemplateResult } from 'lit-html';
import { session } from '../../state/session';
import { avatarMenu } from './avatar-menu';

export function header(): TemplateResult {
  const user = session.currentUser.value;
  return html`<header class="site-header">
    <a class="site-title" href="/" data-link>Spades</a>
    <nav class="site-nav">
      ${user
        ? avatarMenu(user)
        : html`<a class="btn btn--secondary" href="/login" data-link data-testid="sign-in"
            >Sign in</a
          >`}
    </nav>
  </header>`;
}
