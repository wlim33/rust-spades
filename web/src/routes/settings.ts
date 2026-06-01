import { html, render, nothing } from 'lit-html';
import { effect, signal } from '@preact/signals-core';
import { appShell } from '../ui/templates';
import { formField } from '../ui/components/form-field';
import { button } from '../ui/components/button';
import { session } from '../state/session';
import { navigateTo } from '../lib/util';
import type { RouteModule } from '../router';

export const settings: RouteModule = {
  render: () => {
    const root = document.getElementById('root');
    if (!root) return () => {};

    // Auth gate: if not signed in, redirect to login.
    if (!session.currentUser.value) {
      navigateTo('/login?next=/me');
      return () => {};
    }

    const email = signal(session.currentUser.value.email);
    const currentPassword = signal('');
    const newPassword = signal('');
    const saving = signal(false);
    const error = signal<string | null>(null);
    const saved = signal(false);

    const onSave = async (): Promise<void> => {
      if (saving.value) return;
      saving.value = true;
      error.value = null;
      saved.value = false;
      try {
        const u = session.currentUser.value!;
        const wantsEmailChange = email.value !== u.email;
        const wantsPasswordChange = newPassword.value.length > 0;
        if (!wantsEmailChange && !wantsPasswordChange) {
          error.value = 'No changes to save.';
          return;
        }
        if (!currentPassword.value) {
          error.value = 'Current password is required for any change.';
          return;
        }
        if (wantsEmailChange) {
          await session.updateEmail(email.value, currentPassword.value);
        }
        if (wantsPasswordChange) {
          await session.updatePassword(currentPassword.value, newPassword.value);
        }
        currentPassword.value = '';
        newPassword.value = '';
        saved.value = true;
      } catch (e) {
        error.value = e instanceof Error ? e.message : 'Could not save.';
      } finally {
        saving.value = false;
      }
    };

    const template = () => {
      const u = session.currentUser.value;
      if (!u) return appShell(html`<p>Redirecting…</p>`);
      return appShell(html`
        <section class="form-page panel">
          <h2>Settings</h2>
          <p>Signed in as <strong>${u.username}</strong> (${u.email})</p>
          ${error.value
            ? html`<p data-testid="form-error" class="field-error">${error.value}</p>`
            : nothing}
          ${saved.value ? html`<p class="field-success">Saved.</p>` : nothing}
          ${formField({
            id: 'email',
            label: 'Email',
            type: 'email',
            value: email.value,
            autocomplete: 'email',
            onInput: (e) => {
              email.value = (e.target as HTMLInputElement).value;
              saved.value = false;
            },
          })}
          ${formField({
            id: 'current_password',
            label: 'Current password',
            type: 'password',
            value: currentPassword.value,
            autocomplete: 'current-password',
            onInput: (e) => {
              currentPassword.value = (e.target as HTMLInputElement).value;
            },
          })}
          ${formField({
            id: 'new_password',
            label: 'New password (leave blank to keep current)',
            type: 'password',
            value: newPassword.value,
            autocomplete: 'new-password',
            onInput: (e) => {
              newPassword.value = (e.target as HTMLInputElement).value;
            },
          })}
          <div class="form-actions">
            ${button({
              label: saving.value ? 'Saving…' : 'Save',
              onClick: () => void onSave(),
              variant: 'primary',
              disabled: saving.value,
            })}
            ${button({
              label: 'Sign out',
              variant: 'secondary',
              onClick: () => {
                void session.logout().then(() => navigateTo('/'));
              },
            })}
          </div>
        </section>
      `);
    };

    const tagSave = (): void => {
      const btns = root.querySelectorAll<HTMLButtonElement>('.form-actions .btn');
      if (btns[0]) btns[0].setAttribute('data-testid', 'save');
    };

    const dispose = effect(() => {
      render(template(), root);
      tagSave();
    });

    return () => {
      dispose();
      render(nothing, root);
    };
  },
};
