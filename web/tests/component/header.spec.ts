import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest';
import { render } from 'lit-html';
import { header } from '../../src/ui/components/header';
import { session } from '../../src/state/session';
import type { User } from '../../src/state/user-types';
import { themeState } from '../../src/state/theme';

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

describe('header theme toggle', () => {
  beforeEach(() => {
    document.body.innerHTML = '<main id="root"></main>';
    themeState.set('light');
  });
  afterEach(() => themeState.set('light'));

  it('renders a theme toggle button', () => {
    render(header(), document.getElementById('root')!);
    expect(document.querySelector('[data-testid=theme-toggle]')).not.toBeNull();
  });

  it('clicking the toggle flips the theme on <html>', () => {
    render(header(), document.getElementById('root')!);
    (document.querySelector('[data-testid=theme-toggle]') as HTMLButtonElement).click();
    expect(document.documentElement.getAttribute('data-theme')).toBe('dark');
  });
});
