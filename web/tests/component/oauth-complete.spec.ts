import { describe, it, expect, beforeEach, vi, afterEach } from 'vitest';
import { oauthComplete } from '../../src/routes/oauth-complete';
import { session } from '../../src/state/session';

describe('oauth-complete route', () => {
  beforeEach(() => {
    document.body.innerHTML = '<main id="root"></main>';
    session.currentUser.value = null;
  });
  afterEach(() => vi.restoreAllMocks());

  it('renders username field and a submit', () => {
    const cleanup = oauthComplete.render(
      {},
      { path: '/oauth/complete', search: new URLSearchParams() },
    );
    expect(document.querySelector<HTMLInputElement>('#username')).not.toBeNull();
    expect(document.querySelector('[data-testid=submit]')).not.toBeNull();
    expect(document.querySelector('.auth-card h2')).not.toBeNull();
    cleanup();
  });

  it('shows intro paragraph', () => {
    const cleanup = oauthComplete.render(
      {},
      { path: '/oauth/complete', search: new URLSearchParams() },
    );
    expect(document.querySelector('.auth-card p')?.textContent).toContain('almost in');
    cleanup();
  });

  it('shows validation error for invalid username', async () => {
    const cleanup = oauthComplete.render(
      {},
      { path: '/oauth/complete', search: new URLSearchParams() },
    );
    const usernameInput = document.querySelector('#username') as HTMLInputElement;
    usernameInput.value = 'x';
    usernameInput.dispatchEvent(new Event('input'));
    document.querySelector<HTMLButtonElement>('[data-testid=submit]')!.click();
    await Promise.resolve();
    await Promise.resolve();
    expect(document.querySelector('[data-testid=form-error]')?.textContent).toContain('Username');
    cleanup();
  });

  it('on success calls session.completeOauth and navigates to /', async () => {
    const completeSpy = vi.spyOn(session, 'completeOauth').mockResolvedValue(undefined);
    const pushSpy = vi.spyOn(history, 'pushState').mockImplementation(() => {});

    const cleanup = oauthComplete.render(
      {},
      { path: '/oauth/complete', search: new URLSearchParams() },
    );
    const usernameInput = document.querySelector('#username') as HTMLInputElement;
    usernameInput.value = 'alice';
    usernameInput.dispatchEvent(new Event('input'));
    document.querySelector<HTMLButtonElement>('[data-testid=submit]')!.click();

    await Promise.resolve();
    await Promise.resolve();

    expect(completeSpy).toHaveBeenCalledWith('alice');
    expect(pushSpy).toHaveBeenCalledWith(null, '', '/');
    cleanup();
  });
});
