import { html, render, nothing } from 'lit-html';
import { effect, signal } from '@preact/signals-core';
import { appShell } from '../ui/templates';
import { formField } from '../ui/components/form-field';
import { button } from '../ui/components/button';
import { session } from '../state/session';
import { ApiError } from '../api/client';
import { navigateTo } from '../lib/util';
import type { RouteModule } from '../router';

export const oauthComplete: RouteModule = {
  render: () => {
    const root = document.getElementById('root');
    if (!root) return () => {};

    const username = signal('');
    const error = signal<string | null>(null);
    const submitting = signal(false);

    const onSubmit = async (): Promise<void> => {
      if (submitting.value) return;
      if (!/^[a-zA-Z0-9_]{2,20}$/.test(username.value)) {
        error.value = 'Username must be 2-20 letters/numbers/underscores.';
        return;
      }
      submitting.value = true;
      error.value = null;
      try {
        await session.completeOauth(username.value);
        try {
          sessionStorage.removeItem('spades_oauth_lingering');
        } catch {
          // ignore
        }
        navigateTo('/');
      } catch (e) {
        error.value = e instanceof ApiError ? e.message : 'Could not complete sign-in.';
      } finally {
        submitting.value = false;
      }
    };

    const template = () =>
      appShell(html`
        <section class="form-page">
          <h2>Choose a username</h2>
          <p>You're almost in. Pick a public username to finish creating your account.</p>
          ${error.value
            ? html`<p data-testid="form-error" class="field-error">${error.value}</p>`
            : nothing}
          <form
            @submit=${(e: Event) => {
              e.preventDefault();
              void onSubmit();
            }}
          >
            ${formField({
              id: 'username',
              label: 'Username',
              value: username.value,
              autocomplete: 'username',
              maxLength: 20,
              onInput: (e) => {
                username.value = (e.target as HTMLInputElement).value;
              },
            })}
            <div class="form-actions">
              ${button({
                label: submitting.value ? 'Finishing…' : 'Continue',
                onClick: () => {},
                variant: 'primary',
                disabled: submitting.value,
              })}
            </div>
          </form>
        </section>
      `);

    const tagSubmit = (): void => {
      const btn = root.querySelector<HTMLButtonElement>('.form-actions .btn--primary');
      if (btn) btn.setAttribute('type', 'submit');
    };

    const dispose = effect(() => {
      render(template(), root);
      tagSubmit();
    });

    return () => {
      dispose();
      render(nothing, root);
    };
  },
};
