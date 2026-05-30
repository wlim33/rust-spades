import { html, type TemplateResult } from 'lit-html';
import { session } from '../../state/session';
import { themeState } from '../../state/theme';
import { avatarMenu } from './avatar-menu';
import { icon } from '../icon';

function themeToggle(): TemplateResult {
  const dark = themeState.theme.value === 'dark';
  return html`<button
    class="theme-toggle"
    type="button"
    data-testid="theme-toggle"
    aria-label=${dark ? 'Switch to light theme' : 'Switch to dark theme'}
    @click=${() => themeState.toggle()}
  >
    ${icon(dark ? 'sun-line' : 'moon-line')}
  </button>`;
}

export function header(): TemplateResult {
  const user = session.currentUser.value;
  return html`<header class="site-header">
    <a class="site-title" href="/" data-link>Spades</a>
    <nav class="site-nav">
      ${themeToggle()}
      ${user
        ? avatarMenu(user)
        : html`<a class="site-nav__link" href="/login" data-link data-testid="sign-in">Sign in</a>`}
    </nav>
  </header>`;
}
