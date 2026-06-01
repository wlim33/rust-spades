import { describe, it, expect, beforeEach } from 'vitest';
import { html, render } from 'lit-html';
import { authCard } from '../../src/ui/components/auth-card';
import { oauthButtons } from '../../src/ui/components/oauth-buttons';

describe('authCard', () => {
  beforeEach(() => {
    document.body.innerHTML = '<main id="root"></main>';
  });
  it('renders the brand, title, and children', () => {
    render(
      authCard({ title: 'Sign in', children: html`<p data-testid="kid">x</p>` }),
      document.getElementById('root')!,
    );
    const card = document.querySelector('.auth-card')!;
    expect(card).not.toBeNull();
    expect(card.querySelector('.auth-card__brand')?.textContent).toContain('Spades');
    expect(card.querySelector('h2')?.textContent).toBe('Sign in');
    expect(card.querySelector('[data-testid=kid]')).not.toBeNull();
  });
});

describe('oauthButtons', () => {
  beforeEach(() => {
    document.body.innerHTML = '<main id="root"></main>';
  });
  it('renders a divider + Google + GitHub buttons', () => {
    render(oauthButtons({ next: '/' }), document.getElementById('root')!);
    expect(document.querySelector('.auth-divider')).not.toBeNull();
    const btns = document.querySelectorAll('button.btn--secondary');
    expect(btns.length).toBe(2);
    expect(btns[0]!.textContent).toContain('Google');
    expect(btns[1]!.textContent).toContain('GitHub');
  });
});
