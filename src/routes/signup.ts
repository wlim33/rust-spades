import { html, render, nothing } from 'lit-html';
import { effect, signal } from '@preact/signals-core';
import { appShell } from '../ui/templates';
import { formField } from '../ui/components/form-field';
import { button } from '../ui/components/button';
import { session } from '../state/session';
import { ApiError } from '../api/client';
import { navigateTo } from '../lib/util';
import type { RouteModule } from '../router';

export const signup: RouteModule = {
  render: () => {
    const root = document.getElementById('root');
    if (!root) return () => {};

    const email = signal('');
    const username = signal('');
    const password = signal('');
    const error = signal<string | null>(null);
    const submitting = signal(false);

    const validate = (): string | null => {
      if (!email.value.trim() || !username.value.trim() || !password.value) {
        return 'All fields are required.';
      }
      if (password.value.length < 8) return 'Password must be at least 8 characters.';
      if (!/^[a-zA-Z0-9_]{2,20}$/.test(username.value)) {
        return 'Username must be 2-20 letters/numbers/underscores.';
      }
      return null;
    };

    const onSubmit = async (): Promise<void> => {
      if (submitting.value) return;
      const v = validate();
      if (v) {
        error.value = v;
        return;
      }
      submitting.value = true;
      error.value = null;
      try {
        await session.signupWithPassword({
          email: email.value,
          password: password.value,
          username: username.value,
        });
        navigateTo('/');
      } catch (e) {
        error.value =
          e instanceof ApiError ? e.message : e instanceof Error ? e.message : 'Sign up failed.';
      } finally {
        submitting.value = false;
      }
    };

    const template = () =>
      appShell(html`
        <section class="form-page">
          <h2>Sign up</h2>
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
              id: 'username',
              label: 'Username',
              value: username.value,
              autocomplete: 'username',
              maxLength: 20,
              onInput: (e) => {
                username.value = (e.target as HTMLInputElement).value;
              },
            })}
            ${formField({
              id: 'password',
              label: 'Password',
              type: 'password',
              value: password.value,
              autocomplete: 'new-password',
              onInput: (e) => {
                password.value = (e.target as HTMLInputElement).value;
              },
            })}
            <div class="form-actions">
              ${button({
                label: submitting.value ? 'Creating account…' : 'Sign up',
                onClick: () => {},
                variant: 'primary',
                disabled: submitting.value,
              })}
            </div>
          </form>
          <p>Have an account? <a href="/login" data-link>Sign in</a></p>
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
