import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest';
import { render } from 'lit-html';
import { header } from '../../src/ui/components/header';
import { session } from '../../src/state/session';
import type { User } from '../../src/state/user-types';

describe('header', () => {
  beforeEach(() => {
    document.body.innerHTML = '<main id="root"></main>';
    session.currentUser.value = null;
  });
  afterEach(() => vi.restoreAllMocks());

  it('shows Sign in when anonymous', () => {
    render(header(), document.getElementById('root')!);
    expect(document.querySelector('[data-testid=sign-in]')).not.toBeNull();
    expect(document.querySelector('[data-testid=avatar-menu]')).toBeNull();
  });

  it('shows username in avatar menu summary when signed in', () => {
    session.currentUser.value = {
      id: 'u1',
      username: 'alice',
      email: 'a@x',
      email_verified: true,
    } as User;
    render(header(), document.getElementById('root')!);
    const menu = document.querySelector('[data-testid=avatar-menu]')!;
    expect(menu.querySelector('summary')?.textContent?.trim()).toBe('alice');
  });

  it('shows username when signed in', () => {
    session.currentUser.value = {
      id: 'u1',
      username: 'alice',
      email: 'a@x',
      email_verified: true,
    } as User;
    render(header(), document.getElementById('root')!);
    expect(document.querySelector('[data-testid=avatar-menu] summary')?.textContent?.trim()).toBe(
      'alice',
    );
  });
});
