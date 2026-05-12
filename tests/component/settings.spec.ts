import { describe, it, expect, beforeEach, vi, afterEach } from 'vitest';
import { settings } from '../../src/routes/settings';
import { session } from '../../src/state/session';
import type { User } from '../../src/state/user-types';

const mockUser: User = {
  id: 'u1',
  username: 'alice',
  email: 'a@x',
  display_name: 'Alice',
  email_verified: true,
  created_at: '2026',
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

  it('renders display name field with current value', () => {
    const cleanup = settings.render({}, { path: '/me', search: new URLSearchParams() });
    const input = document.querySelector<HTMLInputElement>('#display_name')!;
    expect(input.value).toBe('Alice');
    cleanup();
  });

  it('saving display name calls session.updateDisplayName', async () => {
    const upd = vi.spyOn(session, 'updateDisplayName').mockImplementation(async (n) => {
      if (n != null) {
        session.currentUser.value = { ...session.currentUser.value!, display_name: n };
      }
    });
    const cleanup = settings.render({}, { path: '/me', search: new URLSearchParams() });
    const input = document.querySelector<HTMLInputElement>('#display_name')!;
    input.value = 'AliceP';
    input.dispatchEvent(new Event('input'));
    document.querySelector<HTMLButtonElement>('[data-testid=save]')!.click();
    await Promise.resolve();
    await Promise.resolve();
    expect(upd).toHaveBeenCalledWith('AliceP');
    cleanup();
  });
});
