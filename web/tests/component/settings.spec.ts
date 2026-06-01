import { describe, it, expect, beforeEach, vi, afterEach } from 'vitest';
import { settings } from '../../src/routes/settings';
import { session } from '../../src/state/session';
import type { User } from '../../src/state/user-types';

const mockUser: User = {
  id: 'u1',
  username: 'alice',
  email: 'a@x',
  email_verified: true,
};

describe('settings route', () => {
  beforeEach(() => {
    document.body.innerHTML = '<main id="root"></main>';
    session.currentUser.value = { ...mockUser };
  });
  afterEach(() => vi.restoreAllMocks());

  it('redirects to /login?next=/me when anonymous', () => {
    session.currentUser.value = null;
    const pushSpy = vi.spyOn(history, 'pushState').mockImplementation(() => {});
    const cleanup = settings.render({}, { path: '/me', search: new URLSearchParams() });
    expect(pushSpy).toHaveBeenCalledWith(null, '', '/login?next=/me');
    cleanup();
  });

  it('renders email, current_password, and new_password fields', () => {
    const cleanup = settings.render({}, { path: '/me', search: new URLSearchParams() });
    const emailInput = document.querySelector<HTMLInputElement>('#email')!;
    expect(emailInput).not.toBeNull();
    expect(emailInput.value).toBe('a@x');
    expect(document.querySelector('#current_password')).not.toBeNull();
    expect(document.querySelector('#new_password')).not.toBeNull();
    expect(document.querySelector('.form-page.panel')).not.toBeNull();
    cleanup();
  });

  it('save calls updateEmail when email changed', async () => {
    const upd = vi.spyOn(session, 'updateEmail').mockImplementation(async (newEmail) => {
      session.currentUser.value = { ...session.currentUser.value!, email: newEmail };
    });
    const cleanup = settings.render({}, { path: '/me', search: new URLSearchParams() });

    const emailInput = document.querySelector<HTMLInputElement>('#email')!;
    emailInput.value = 'newalice@x';
    emailInput.dispatchEvent(new Event('input'));

    const pwInput = document.querySelector<HTMLInputElement>('#current_password')!;
    pwInput.value = 'hunter2';
    pwInput.dispatchEvent(new Event('input'));

    document.querySelector<HTMLButtonElement>('[data-testid=save]')!.click();
    await Promise.resolve();
    await Promise.resolve();
    expect(upd).toHaveBeenCalledWith('newalice@x', 'hunter2');
    expect(document.querySelector('.field-success')?.textContent).toContain('Saved');
    cleanup();
  });
});
