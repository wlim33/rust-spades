import { html, type TemplateResult } from 'lit-html';
import { session } from '../../state/session';
import { navigateTo } from '../../lib/util';
import type { User } from '../../state/user-types';

export function avatarMenu(user: User): TemplateResult {
  return html`<details class="avatar-menu" data-testid="avatar-menu">
    <summary class="avatar-menu__btn">${user.username}</summary>
    <ul class="avatar-menu__list">
      <li><a href=${`/u/${user.username}`} data-link>My profile</a></li>
      <li><a href="/me" data-link>Settings</a></li>
      <li>
        <button
          type="button"
          @click=${() => {
            void session.logout().then(() => navigateTo('/'));
          }}
        >
          Sign out
        </button>
      </li>
    </ul>
  </details>`;
}
