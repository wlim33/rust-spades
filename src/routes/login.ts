import { html, render, nothing } from 'lit-html';
import { effect, signal } from '@preact/signals-core';
import { appShell } from '../ui/templates';
import { formField } from '../ui/components/form-field';
import { button } from '../ui/components/button';
import { session } from '../state/session';
import { ApiError } from '../api/client';
import { navigateTo } from '../lib/util';
import type { RouteModule } from '../router';

export const login: RouteModule = {
  render: (_params, ctx) => {
    const root = document.getElementById('root');
    if (!root) return () => {};

    const email = signal('');
    const password = signal('');
    const error = signal<string | null>(null);
    const submitting = signal(false);
    const next = ctx.search.get('next') ?? '/';

    const onSubmit = async (): Promise<void> => {
      if (submitting.value) return;
      submitting.value = true;
      error.value = null;
      try {
        await session.loginWithPassword(email.value, password.value);
        navigateTo(next);
      } catch (e) {
        error.value =
          e instanceof ApiError ? e.message : e instanceof Error ? e.message : 'Login failed.';
      } finally {
        submitting.value = false;
      }
    };

    const template = () =>
      appShell(html`
        <section class="form-page">
          <h2>Sign in</h2>
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
              id: 'email',
              label: 'Email',
              type: 'email',
              value: email.value,
              autocomplete: 'email',
              onInput: (e) => {
                email.value = (e.target as HTMLInputElement).value;
              },
            })}
            ${formField({
              id: 'password',
              label: 'Password',
              type: 'password',
              value: password.value,
              autocomplete: 'current-password',
              onInput: (e) => {
                password.value = (e.target as HTMLInputElement).value;
              },
            })}
            <div class="form-actions">
              ${button({
                label: submitting.value ? 'Signing in…' : 'Sign in',
                onClick: () => {},
                variant: 'primary',
                disabled: submitting.value,
              })}
            </div>
          </form>
          <p class="oauth-divider">or</p>
          ${button({
            label: 'Sign in with Google',
            onClick: () => session.startOauth('google', next),
            variant: 'secondary',
          })}
          ${button({
            label: 'Sign in with GitHub',
            onClick: () => session.startOauth('github', next),
            variant: 'secondary',
          })}
          <p>No account? <a href="/signup" data-link>Sign up</a></p>
        </section>
      `);

    const tagSubmit = (): void => {
      const btn = root.querySelector<HTMLButtonElement>('.form-actions .btn--primary');
      if (btn) {
        if (!btn.hasAttribute('data-testid')) btn.setAttribute('data-testid', 'submit');
        btn.setAttribute('type', 'submit');
      }
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
