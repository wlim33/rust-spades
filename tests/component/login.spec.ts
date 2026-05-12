import { describe, it, expect, beforeEach, vi, afterEach } from 'vitest';
import { login } from '../../src/routes/login';
import { session } from '../../src/state/session';
import type { User } from '../../src/state/user-types';

const mockUser: User = {
  id: 'u1',
  username: 'alice',
  email: 'a@x',
  email_verified: true,
  created_at: '2026',
};

describe('login route', () => {
  beforeEach(() => {
    document.body.innerHTML = '<main id="root"></main>';
    session.currentUser.value = null;
  });
  afterEach(() => vi.restoreAllMocks());

  it('renders email + password fields and a submit', () => {
    const cleanup = login.render({}, { path: '/login', search: new URLSearchParams() });
    expect(document.querySelector<HTMLInputElement>('#email')).not.toBeNull();
    expect(document.querySelector<HTMLInputElement>('#password')).not.toBeNull();
    expect(document.querySelector('[data-testid=submit]')).not.toBeNull();
    cleanup();
  });

  it('on submit calls session.loginWithPassword and navigates to next', async () => {
    const loginSpy = vi.spyOn(session, 'loginWithPassword').mockImplementation(async () => {
      session.currentUser.value = mockUser;
    });
    const pushSpy = vi.spyOn(history, 'pushState').mockImplementation(() => {});

    const cleanup = login.render(
      {},
      { path: '/login?next=/me', search: new URLSearchParams('next=/me') },
    );
    const emailInput = document.querySelector('#email') as HTMLInputElement;
    emailInput.value = 'a@x';
    emailInput.dispatchEvent(new Event('input'));
    const passwordInput = document.querySelector('#password') as HTMLInputElement;
    passwordInput.value = 'pw';
    passwordInput.dispatchEvent(new Event('input'));
    document.querySelector<HTMLButtonElement>('[data-testid=submit]')!.click();

    await Promise.resolve();
    await Promise.resolve();

    expect(loginSpy).toHaveBeenCalledWith('a@x', 'pw');
    expect(pushSpy).toHaveBeenCalledWith(null, '', '/me');
    cleanup();
  });

  it('displays the server error on 401', async () => {
    vi.spyOn(session, 'loginWithPassword').mockRejectedValue(
      Object.assign(new Error('bad creds'), { status: 401 }),
    );
    const cleanup = login.render({}, { path: '/login', search: new URLSearchParams() });
    document.querySelector<HTMLButtonElement>('[data-testid=submit]')!.click();
    await Promise.resolve();
    await Promise.resolve();
    await Promise.resolve();
    expect(document.querySelector('[data-testid=form-error]')?.textContent).toContain('bad creds');
    cleanup();
  });
});
