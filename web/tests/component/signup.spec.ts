import { describe, it, expect, beforeEach, vi, afterEach } from 'vitest';
import { signup } from '../../src/routes/signup';
import { session } from '../../src/state/session';
import type { User } from '../../src/state/user-types';

const mockUser: User = {
  id: 'u1',
  username: 'alice',
  email: 'a@x',
  email_verified: false,
};

describe('signup route', () => {
  beforeEach(() => {
    document.body.innerHTML = '<main id="root"></main>';
    session.currentUser.value = null;
  });
  afterEach(() => vi.restoreAllMocks());

  it('renders email, username, password fields', () => {
    const cleanup = signup.render({}, { path: '/signup', search: new URLSearchParams() });
    expect(document.querySelector<HTMLInputElement>('#email')).not.toBeNull();
    expect(document.querySelector<HTMLInputElement>('#username')).not.toBeNull();
    expect(document.querySelector<HTMLInputElement>('#password')).not.toBeNull();
    cleanup();
  });

  it('rejects empty submit with inline error', async () => {
    vi.spyOn(session, 'signupWithPassword');
    const cleanup = signup.render({}, { path: '/signup', search: new URLSearchParams() });
    document.querySelector<HTMLButtonElement>('[data-testid=submit]')!.click();
    await Promise.resolve();
    expect(session.signupWithPassword).not.toHaveBeenCalled();
    expect(document.querySelector('[data-testid=form-error]')?.textContent).toContain('required');
    cleanup();
  });

  it('on success navigates to /', async () => {
    vi.spyOn(session, 'signupWithPassword').mockImplementation(async () => {
      session.currentUser.value = mockUser;
    });
    const pushSpy = vi.spyOn(history, 'pushState').mockImplementation(() => {});

    const cleanup = signup.render({}, { path: '/signup', search: new URLSearchParams() });
    const emailInput = document.querySelector('#email') as HTMLInputElement;
    emailInput.value = 'a@x';
    emailInput.dispatchEvent(new Event('input'));
    const usernameInput = document.querySelector('#username') as HTMLInputElement;
    usernameInput.value = 'alice';
    usernameInput.dispatchEvent(new Event('input'));
    const passwordInput = document.querySelector('#password') as HTMLInputElement;
    passwordInput.value = 'pwpwpwpw';
    passwordInput.dispatchEvent(new Event('input'));
    document.querySelector<HTMLButtonElement>('[data-testid=submit]')!.click();

    await Promise.resolve();
    await Promise.resolve();
    expect(pushSpy).toHaveBeenCalledWith(null, '', '/');
    cleanup();
  });
});
