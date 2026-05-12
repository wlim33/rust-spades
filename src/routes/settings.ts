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

    const displayName = signal(session.currentUser.value.display_name ?? '');
    const saving = signal(false);
    const error = signal<string | null>(null);
    const saved = signal(false);

    const onSave = async (): Promise<void> => {
      if (saving.value) return;
      saving.value = true;
      error.value = null;
      saved.value = false;
      try {
        await session.updateDisplayName(displayName.value.trim() || null);
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
        <section class="form-page">
          <h2>Settings</h2>
          <p>Signed in as <strong>${u.username}</strong> (${u.email})</p>
          ${error.value
            ? html`<p data-testid="form-error" class="field-error">${error.value}</p>`
            : nothing}
          ${saved.value ? html`<p style="color: var(--color-accent)">Saved.</p>` : nothing}
          ${formField({
            id: 'display_name',
            label: 'Display name (shown in games)',
            value: displayName.value,
            maxLength: 20,
            placeholder: u.username,
            onInput: (e) => {
              displayName.value = (e.target as HTMLInputElement).value;
              saved.value = false;
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
