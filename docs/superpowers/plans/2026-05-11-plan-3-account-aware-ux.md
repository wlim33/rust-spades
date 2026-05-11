# spades-ts — Plan 3: Account-Aware UX Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add account features on top of Plan 2's gameplay parity: email + password sign-up/sign-in, Google + GitHub OAuth, signed-in header chrome, own-settings page (rename), public profile pages, player game history. Anonymous play continues to work unchanged.

**Architecture:** A single `session` store (`currentUser: Signal<User | null>`) drives the header and route guards. Auth/profile pages are conventional forms with mocked-API component tests; OAuth uses the server's redirect-to-provider flow plus a `__oauth_pending` cookie handshake. The in-game seat name picks the signed-in user's display name when present; falls back to the existing anonymous `PUT /games/:gid/players/:pid/name` path otherwise.

**Tech Stack:** Built on Plan 2. No new deps.

**Reference spec:** `/Users/wlim/Projects/spades-ts/docs/superpowers/specs/2026-05-11-spades-ts-design.md` (§ 3 modules `state/session.ts`, `routes/{login,signup,oauth-complete,settings,profile}.ts`; § 4 route guards)

**Server endpoint contract (from rust-spades):**

| Method | Path                           | Notes                                                                  |
| ------ | ------------------------------ | ---------------------------------------------------------------------- |
| POST   | `/auth/register`               | `{email, password, username}` → `UserResponse`                         |
| POST   | `/auth/login`                  | `{email, password}` → `UserResponse`                                   |
| POST   | `/auth/logout`                 | 204                                                                    |
| GET    | `/auth/me`                     | `UserResponse` or 401                                                  |
| GET    | `/auth/oauth/{provider}/login` | Server redirect to provider (browser navigation, not fetch)            |
| POST   | `/auth/oauth/complete`         | `{username}` → `UserResponse` (uses HttpOnly `__oauth_pending` cookie) |
| PATCH  | `/users/me`                    | `{display_name?}` → `UserResponse`                                     |
| GET    | `/users/{username}`            | Profile data                                                           |
| GET    | `/users/{username}/games`      | Game history                                                           |

**Deferred (server has them, we are intentionally not wiring them up yet):**

- `GET /auth/verify-email`
- `POST /auth/password-reset/request`
- `POST /auth/password-reset/confirm`

---

## Files this plan creates or modifies

| Path                                 | Action | Responsibility                                                               |
| ------------------------------------ | ------ | ---------------------------------------------------------------------------- |
| `src/state/session.ts`               | create | `currentUser` signal + login/signup/logout/refresh/startOauth                |
| `src/state/user-types.ts`            | create | Shared `User` / `UserResponse` / `ProfileResponse` / `GameHistoryItem` types |
| `src/ui/components/header.ts`        | modify | Sign-in button vs. avatar menu                                               |
| `src/ui/components/avatar-menu.ts`   | create | Dropdown for `/me`, profile, sign out                                        |
| `src/ui/components/form-field.ts`    | create | Reusable input + label + error                                               |
| `src/routes/login.ts`                | create | Email/password + OAuth buttons                                               |
| `src/routes/signup.ts`               | create | Email/password/username form                                                 |
| `src/routes/oauth-complete.ts`       | create | Username picker for OAuth pending                                            |
| `src/routes/settings.ts`             | create | `/me` — display-name edit + sign out (auth-gated)                            |
| `src/routes/profile.ts`              | create | `/u/:username` — public profile + history                                    |
| `src/main.ts`                        | modify | Register new routes; hydrate session before routing                          |
| `src/routes/play.ts`                 | modify | Prefer signed-in display name                                                |
| `src/lib/storage.ts`                 | modify | Add `oauth-in-progress` flag helpers + `oauth-next` flag                     |
| `tests/unit/session.spec.ts`         | create | login/logout/refresh against mocked fetch                                    |
| `tests/component/header.spec.ts`     | create | Sign-in vs avatar                                                            |
| `tests/component/form-field.spec.ts` | create | Validation states                                                            |
| `tests/component/login.spec.ts`      | create | Submit, error, redirect-on-success                                           |
| `tests/component/signup.spec.ts`     | create | Submit, validation, error                                                    |
| `tests/component/settings.spec.ts`   | create | Rename flow                                                                  |
| `tests/component/profile.spec.ts`    | create | Renders shape + loading + not-found                                          |
| `tests/e2e/auth.spec.ts`             | create | Signup → /me → logout → login → /me                                          |
| `tests/e2e/profile.spec.ts`          | create | Completed game shows in `/u/:username`                                       |

---

## Task 1: User types + storage helpers

**Files:**

- Create: `src/state/user-types.ts`
- Modify: `src/lib/storage.ts`

- [ ] **Step 1: Create `src/state/user-types.ts`**

Use the generated `paths`/`components` from `src/api/schema.d.ts` as the source of truth where available; re-export under the names the frontend uses. If the user's Phase 0 oasgen patch doesn't cover one of these, replace the generated reference with a hand-written interface that matches the server's `UserResponse` (see `rust-spades/.../handlers_auth.rs` for fields: `id`, `username`, `email`, `display_name?`, `email_verified`, `created_at`).

```ts
import type { components } from '../api/schema';

// If any of these isn't present in the generated schema after Phase 0,
// open the file and inline the interface directly here. The fields match
// rust-spades' UserResponse / ProfileResponse / GameHistoryItem shapes.
export type User = components['schemas']['UserResponse'];
export type ProfileResponse = components['schemas']['ProfileResponse'];
export type GameHistoryItem = components['schemas']['GameHistoryItem'];
```

If any reference is undefined, switch the offending line to:

```ts
export type User = {
  id: string;
  username: string;
  email: string;
  display_name?: string | null;
  email_verified: boolean;
  created_at: string;
};
```

(Same pattern for `ProfileResponse` and `GameHistoryItem`. Use the server source for exact fields.)

- [ ] **Step 2: Extend `src/lib/storage.ts`**

Append:

```ts
const OAUTH_IN_PROGRESS_KEY = 'spades_oauth_in_progress';
const OAUTH_NEXT_KEY = 'spades_oauth_next';

export function markOauthInProgress(provider: 'google' | 'github', next: string): void {
  try {
    localStorage.setItem(OAUTH_IN_PROGRESS_KEY, provider);
    localStorage.setItem(OAUTH_NEXT_KEY, next);
  } catch {
    // ignore
  }
}

export function consumeOauthInProgress(): { provider: string; next: string } | null {
  try {
    const provider = localStorage.getItem(OAUTH_IN_PROGRESS_KEY);
    const next = localStorage.getItem(OAUTH_NEXT_KEY);
    localStorage.removeItem(OAUTH_IN_PROGRESS_KEY);
    localStorage.removeItem(OAUTH_NEXT_KEY);
    if (!provider) return null;
    return { provider, next: next ?? '/' };
  } catch {
    return null;
  }
}
```

- [ ] **Step 3: Type-check**

Run: `pnpm tsc --noEmit -p tsconfig.json`
Expected: succeeds.

- [ ] **Step 4: Commit**

```bash
git add src/state/user-types.ts src/lib/storage.ts
git commit -m "feat: user types + oauth-in-progress storage helpers"
```

---

## Task 2: Session store

**Files:**

- Create: `src/state/session.ts`, `tests/unit/session.spec.ts`

- [ ] **Step 1: Write failing test**

`tests/unit/session.spec.ts`:

```ts
import { describe, it, expect, vi, beforeEach } from 'vitest';
import { session } from '../../src/state/session';

describe('session store', () => {
  beforeEach(() => {
    vi.unstubAllGlobals();
    session.currentUser.value = null;
  });

  it('refresh() populates currentUser on 200', async () => {
    vi.stubGlobal(
      'fetch',
      vi.fn(
        async () =>
          new Response(
            JSON.stringify({
              id: 'u1',
              username: 'alice',
              email: 'a@x',
              email_verified: true,
              created_at: '2026-01-01',
            }),
            {
              status: 200,
              headers: { 'content-type': 'application/json' },
            },
          ),
      ),
    );
    await session.refresh();
    expect(session.currentUser.value?.username).toBe('alice');
  });

  it('refresh() leaves currentUser null on 401', async () => {
    vi.stubGlobal(
      'fetch',
      vi.fn(async () => new Response('unauthenticated', { status: 401 })),
    );
    await session.refresh();
    expect(session.currentUser.value).toBe(null);
  });

  it('loginWithPassword() sets currentUser on 200', async () => {
    vi.stubGlobal(
      'fetch',
      vi.fn(
        async () =>
          new Response(
            JSON.stringify({
              id: 'u1',
              username: 'alice',
              email: 'a@x',
              email_verified: true,
              created_at: '2026-01-01',
            }),
            {
              status: 200,
              headers: { 'content-type': 'application/json' },
            },
          ),
      ),
    );
    await session.loginWithPassword('a@x', 'pw');
    expect(session.currentUser.value?.username).toBe('alice');
  });

  it('loginWithPassword() throws ApiError on 401', async () => {
    vi.stubGlobal(
      'fetch',
      vi.fn(
        async () =>
          new Response(JSON.stringify({ error: 'bad creds' }), {
            status: 401,
            headers: { 'content-type': 'application/json' },
          }),
      ),
    );
    await expect(session.loginWithPassword('a@x', 'wrong')).rejects.toMatchObject({ status: 401 });
    expect(session.currentUser.value).toBe(null);
  });

  it('logout() clears currentUser', async () => {
    session.currentUser.value = {
      id: 'u1',
      username: 'alice',
      email: 'a@x',
      email_verified: true,
      created_at: '2026-01-01',
    };
    vi.stubGlobal(
      'fetch',
      vi.fn(async () => new Response('', { status: 204 })),
    );
    await session.logout();
    expect(session.currentUser.value).toBe(null);
  });
});
```

- [ ] **Step 2: Run to verify it fails**

Run: `pnpm test:unit`
Expected: FAIL — module not found.

- [ ] **Step 3: Implement `src/state/session.ts`**

```ts
import { signal } from '@preact/signals-core';
import { ApiError, request } from '../api/client';
import { markOauthInProgress } from '../lib/storage';
import type { User } from './user-types';
import { API_URL } from '../lib/util';

const currentUser = signal<User | null>(null);

async function refresh(): Promise<void> {
  try {
    const me = await request<User>('/auth/me', { method: 'GET' });
    currentUser.value = me;
  } catch (e) {
    if (e instanceof ApiError && e.status === 401) {
      currentUser.value = null;
      return;
    }
    throw e;
  }
}

async function loginWithPassword(email: string, password: string): Promise<void> {
  const user = await request<User>('/auth/login', {
    method: 'POST',
    body: JSON.stringify({ email, password }),
  });
  currentUser.value = user;
}

async function signupWithPassword(args: {
  email: string;
  password: string;
  username: string;
}): Promise<void> {
  const user = await request<User>('/auth/register', {
    method: 'POST',
    body: JSON.stringify(args),
  });
  currentUser.value = user;
}

async function logout(): Promise<void> {
  await request<void>('/auth/logout', { method: 'POST' });
  currentUser.value = null;
}

function startOauth(provider: 'google' | 'github', next = '/'): void {
  markOauthInProgress(provider, next);
  // Browser navigation, not SPA — server redirects to provider.
  window.location.assign(`${API_URL}/auth/oauth/${provider}/login`);
}

async function completeOauth(username: string): Promise<void> {
  const user = await request<User>('/auth/oauth/complete', {
    method: 'POST',
    body: JSON.stringify({ username }),
  });
  currentUser.value = user;
}

async function updateDisplayName(displayName: string | null): Promise<void> {
  const user = await request<User>('/users/me', {
    method: 'PATCH',
    body: JSON.stringify({ display_name: displayName }),
  });
  currentUser.value = user;
}

export const session = {
  currentUser,
  refresh,
  loginWithPassword,
  signupWithPassword,
  logout,
  startOauth,
  completeOauth,
  updateDisplayName,
};
```

- [ ] **Step 4: Run test**

Run: `pnpm test:unit`
Expected: all 5 cases pass.

- [ ] **Step 5: Commit**

```bash
git add src/state/session.ts tests/unit/session.spec.ts
git commit -m "feat: session store (login/signup/logout/oauth)"
```

---

## Task 3: Hydrate session at boot

**Files:**

- Modify: `src/main.ts`

- [ ] **Step 1: Update `src/main.ts`**

Replace the bottom of the file (the router setup):

```ts
import './ui/design.css';
import { createRouter } from './router';
import { home } from './routes/home';
import { play } from './routes/play';
import { create } from './routes/create';
import { notFound } from './routes/notfound';
import { session } from './state/session';
import { consumeOauthInProgress } from './lib/storage';

void (async () => {
  // Best-effort: hydrate the session before mounting the first route.
  // If the user just returned from an OAuth provider, route to the username-picker
  // when the server set the __oauth_pending cookie (signaled by /auth/me returning 401
  // while we have a recent oauth-in-progress marker).
  const oauthMarker = consumeOauthInProgress();
  await session.refresh();

  const router = createRouter({
    '/': home,
    '/create': create,
    '/play/:shortId': play,
    '/login': (await import('./routes/login')).login,
    '/signup': (await import('./routes/signup')).signup,
    '/auth/oauth/complete': (await import('./routes/oauth-complete')).oauthComplete,
    '/me': (await import('./routes/settings')).settings,
    '/u/:username': (await import('./routes/profile')).profile,
    '*': notFound,
  });

  document.addEventListener('click', (e) => {
    const target = (e.target as HTMLElement).closest('a[data-link]') as HTMLAnchorElement | null;
    if (!target) return;
    if (target.target === '_blank' || e.metaKey || e.ctrlKey) return;
    const url = new URL(target.href);
    if (url.origin !== location.origin) return;
    e.preventDefault();
    history.pushState(null, '', url.pathname + url.search);
    router.handle(url.pathname + url.search);
  });

  window.addEventListener('popstate', () => {
    router.handle(location.pathname + location.search);
  });

  // If we have an oauth marker and we're NOT signed in, the server is awaiting
  // a username pick. Detour to /auth/oauth/complete.
  if (oauthMarker && session.currentUser.value === null) {
    history.replaceState(null, '', '/auth/oauth/complete');
  }

  router.listen();
})();
```

- [ ] **Step 2: Type-check**

Run: `pnpm tsc --noEmit -p tsconfig.json`
Expected: succeeds. (Will fail until Tasks 4-7 land — that's OK, you can stage this commit after Task 7 or comment out the dynamic imports temporarily.)

If you want to keep this commit standalone, replace each missing route import with a placeholder route:

```ts
const stub: import('./router').RouteModule = { render: () => () => {} };
// '/login': stub,
```

- [ ] **Step 3: Commit (defer until Task 7 if you want a green tree at every commit)**

```bash
git add src/main.ts
git commit -m "feat: hydrate session and register auth routes"
```

---

## Task 4: FormField component + tests

**Files:**

- Create: `src/ui/components/form-field.ts`, `tests/component/form-field.spec.ts`

- [ ] **Step 1: Write failing test**

`tests/component/form-field.spec.ts`:

```ts
import { describe, it, expect, beforeEach } from 'vitest';
import { html, render } from 'lit-html';
import { formField } from '../../src/ui/components/form-field';

describe('formField', () => {
  beforeEach(() => {
    document.body.innerHTML = '<main id="root"></main>';
  });

  it('renders a labeled input with id and value', () => {
    render(
      html`${formField({
        id: 'email',
        label: 'Email',
        type: 'email',
        value: 'a@x',
        onInput: () => {},
      })}`,
      document.getElementById('root')!,
    );
    const input = document.querySelector<HTMLInputElement>('#email')!;
    expect(input).not.toBeNull();
    expect(input.type).toBe('email');
    expect(input.value).toBe('a@x');
    expect(document.querySelector('label[for=email]')?.textContent?.trim()).toBe('Email');
  });

  it('shows error message when provided', () => {
    render(
      html`${formField({
        id: 'email',
        label: 'Email',
        value: '',
        onInput: () => {},
        error: 'Required',
      })}`,
      document.getElementById('root')!,
    );
    expect(document.querySelector('[data-testid=field-error]')?.textContent?.trim()).toBe(
      'Required',
    );
  });
});
```

- [ ] **Step 2: Run to verify it fails**

Run: `pnpm test:component`
Expected: FAIL.

- [ ] **Step 3: Implement `src/ui/components/form-field.ts`**

```ts
import { html, type TemplateResult } from 'lit-html';

export type FormFieldOpts = {
  id: string;
  label: string;
  value: string;
  onInput: (e: Event) => void;
  type?: 'text' | 'email' | 'password';
  placeholder?: string;
  autocomplete?: string;
  maxLength?: number;
  error?: string | null;
  disabled?: boolean;
};

export function formField(opts: FormFieldOpts): TemplateResult {
  return html`<div class="form-field">
    <label for=${opts.id}>${opts.label}</label>
    <input
      id=${opts.id}
      name=${opts.id}
      type=${opts.type ?? 'text'}
      .value=${opts.value}
      placeholder=${opts.placeholder ?? ''}
      autocomplete=${opts.autocomplete ?? 'off'}
      maxlength=${opts.maxLength ?? 200}
      ?disabled=${opts.disabled ?? false}
      @input=${opts.onInput}
    />
    ${opts.error
      ? html`<span data-testid="field-error" class="field-error">${opts.error}</span>`
      : null}
  </div>`;
}
```

- [ ] **Step 4: Add form-field styles to `src/ui/design.css`**

Append:

```css
.form-field {
  display: flex;
  flex-direction: column;
  gap: var(--space-1);
  margin-bottom: var(--space-3);
  width: 100%;
}
.form-field label {
  font-size: var(--font-size-sm);
  color: var(--color-muted);
}
.form-field input {
  padding: var(--space-2);
  border-radius: var(--radius-sm);
  border: 1px solid rgba(0, 0, 0, 0.15);
  font: inherit;
}
.form-field .field-error {
  font-size: var(--font-size-sm);
  color: var(--color-danger);
}
.form-page {
  display: flex;
  flex-direction: column;
  align-items: center;
  max-width: 360px;
  width: 100%;
}
.form-page h2 {
  margin-top: 0;
}
.form-actions {
  display: flex;
  gap: var(--space-2);
  width: 100%;
}
.form-actions .btn {
  flex: 1;
}
.oauth-divider {
  text-align: center;
  color: var(--color-muted);
  margin: var(--space-3) 0;
}
```

- [ ] **Step 5: Run test**

Run: `pnpm test:component`
Expected: 2 cases pass.

- [ ] **Step 6: Commit**

```bash
git add src/ui/components/form-field.ts src/ui/design.css tests/component/form-field.spec.ts
git commit -m "feat: FormField component"
```

---

## Task 5: Login route

**Files:**

- Create: `src/routes/login.ts`, `tests/component/login.spec.ts`

- [ ] **Step 1: Write failing test**

`tests/component/login.spec.ts`:

```ts
import { describe, it, expect, beforeEach, vi, afterEach } from 'vitest';
import { login } from '../../src/routes/login';
import { session } from '../../src/state/session';

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
    expect(
      document.querySelector('button[type=submit], button[data-testid=submit]'),
    ).not.toBeNull();
    cleanup();
  });

  it('on submit calls session.loginWithPassword and navigates to next', async () => {
    const loginSpy = vi.spyOn(session, 'loginWithPassword').mockImplementation(async () => {
      session.currentUser.value = {
        id: 'u1',
        username: 'alice',
        email: 'a@x',
        email_verified: true,
        created_at: '2026',
      };
    });
    const pushSpy = vi.spyOn(history, 'pushState');

    const cleanup = login.render(
      {},
      { path: '/login?next=/me', search: new URLSearchParams('next=/me') },
    );
    (document.querySelector('#email') as HTMLInputElement).value = 'a@x';
    (document.querySelector('#email') as HTMLInputElement).dispatchEvent(new Event('input'));
    (document.querySelector('#password') as HTMLInputElement).value = 'pw';
    (document.querySelector('#password') as HTMLInputElement).dispatchEvent(new Event('input'));
    document.querySelector<HTMLButtonElement>('[data-testid=submit]')!.click();

    // Wait a microtask for the async submit to flush
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
```

- [ ] **Step 2: Run to verify it fails**

Run: `pnpm test:component`
Expected: FAIL.

- [ ] **Step 3: Implement `src/routes/login.ts`**

```ts
import { html, render } from 'lit-html';
import { appShell } from '../ui/templates';
import { formField } from '../ui/components/form-field';
import { button } from '../ui/components/button';
import { session } from '../state/session';
import { ApiError } from '../api/client';
import { navigateTo } from '../lib/util';
import type { RouteModule } from '../router';

export const login: RouteModule = {
  render: (_params, ctx) => {
    const root = document.getElementById('root');
    if (!root) return () => {};

    let email = '';
    let password = '';
    let error: string | null = null;
    let submitting = false;
    const next = ctx.search.get('next') ?? '/';

    const onSubmit = async (): Promise<void> => {
      if (submitting) return;
      submitting = true;
      error = null;
      rerender();
      try {
        await session.loginWithPassword(email, password);
        navigateTo(next);
      } catch (e) {
        error =
          e instanceof ApiError ? e.message : e instanceof Error ? e.message : 'Login failed.';
      } finally {
        submitting = false;
        rerender();
      }
    };

    const template = (): ReturnType<typeof html> =>
      appShell(html`
        <section class="form-page">
          <h2>Sign in</h2>
          ${error ? html`<p data-testid="form-error" class="field-error">${error}</p>` : null}
          <form
            @submit=${(e: Event) => {
              e.preventDefault();
              void onSubmit();
            }}
          >
            ${formField({
              id: 'email',
              label: 'Email',
              type: 'email',
              value: email,
              autocomplete: 'email',
              onInput: (e) => {
                email = (e.target as HTMLInputElement).value;
              },
            })}
            ${formField({
              id: 'password',
              label: 'Password',
              type: 'password',
              value: password,
              autocomplete: 'current-password',
              onInput: (e) => {
                password = (e.target as HTMLInputElement).value;
              },
            })}
            <div class="form-actions">
              ${button({
                label: submitting ? 'Signing in…' : 'Sign in',
                onClick: () => {},
                variant: 'primary',
                disabled: submitting,
              })}
            </div>
          </form>
          <p class="oauth-divider">or</p>
          ${button({
            label: 'Sign in with Google',
            onClick: () => session.startOauth('google', next),
            variant: 'secondary',
          })}
          ${button({
            label: 'Sign in with GitHub',
            onClick: () => session.startOauth('github', next),
            variant: 'secondary',
          })}
          <p>No account? <a href="/signup" data-link>Sign up</a></p>
        </section>
      `);

    // Inject data-testid into the primary submit button by wrapping the rerender.
    // (The `button` helper doesn't accept arbitrary attrs; the form's only primary
    // button is the submit, so we tag it via querySelector after render.)
    const tagSubmit = (): void => {
      const btn = root.querySelector<HTMLButtonElement>('.form-actions .btn--primary');
      if (btn && !btn.hasAttribute('data-testid')) btn.setAttribute('data-testid', 'submit');
      if (btn) btn.setAttribute('type', 'submit');
    };

    const rerender = (): void => {
      render(template(), root);
      tagSubmit();
    };
    rerender();

    return () => render(html``, root);
  },
};
```

- [ ] **Step 4: Run test**

Run: `pnpm test:component`
Expected: 3 cases pass.

- [ ] **Step 5: Commit**

```bash
git add src/routes/login.ts tests/component/login.spec.ts
git commit -m "feat: login route with email/password + OAuth buttons"
```

---

## Task 6: Signup route

**Files:**

- Create: `src/routes/signup.ts`, `tests/component/signup.spec.ts`

- [ ] **Step 1: Write failing test**

`tests/component/signup.spec.ts`:

```ts
import { describe, it, expect, beforeEach, vi, afterEach } from 'vitest';
import { signup } from '../../src/routes/signup';
import { session } from '../../src/state/session';

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
      session.currentUser.value = {
        id: 'u1',
        username: 'alice',
        email: 'a@x',
        email_verified: false,
        created_at: '2026',
      };
    });
    const pushSpy = vi.spyOn(history, 'pushState');

    const cleanup = signup.render({}, { path: '/signup', search: new URLSearchParams() });
    (document.querySelector('#email') as HTMLInputElement).value = 'a@x';
    (document.querySelector('#email') as HTMLInputElement).dispatchEvent(new Event('input'));
    (document.querySelector('#username') as HTMLInputElement).value = 'alice';
    (document.querySelector('#username') as HTMLInputElement).dispatchEvent(new Event('input'));
    (document.querySelector('#password') as HTMLInputElement).value = 'pwpwpwpw';
    (document.querySelector('#password') as HTMLInputElement).dispatchEvent(new Event('input'));
    document.querySelector<HTMLButtonElement>('[data-testid=submit]')!.click();

    await Promise.resolve();
    await Promise.resolve();
    expect(pushSpy).toHaveBeenCalledWith(null, '', '/');
    cleanup();
  });
});
```

- [ ] **Step 2: Implement `src/routes/signup.ts`**

```ts
import { html, render } from 'lit-html';
import { appShell } from '../ui/templates';
import { formField } from '../ui/components/form-field';
import { button } from '../ui/components/button';
import { session } from '../state/session';
import { ApiError } from '../api/client';
import { navigateTo } from '../lib/util';
import type { RouteModule } from '../router';

export const signup: RouteModule = {
  render: () => {
    const root = document.getElementById('root');
    if (!root) return () => {};
    let email = '';
    let username = '';
    let password = '';
    let error: string | null = null;
    let submitting = false;

    const validate = (): string | null => {
      if (!email.trim() || !username.trim() || !password) return 'All fields are required.';
      if (password.length < 8) return 'Password must be at least 8 characters.';
      if (!/^[a-zA-Z0-9_]{2,20}$/.test(username))
        return 'Username must be 2-20 letters/numbers/underscores.';
      return null;
    };

    const onSubmit = async (): Promise<void> => {
      if (submitting) return;
      error = validate();
      if (error) {
        rerender();
        return;
      }
      submitting = true;
      rerender();
      try {
        await session.signupWithPassword({ email, password, username });
        navigateTo('/');
      } catch (e) {
        error =
          e instanceof ApiError ? e.message : e instanceof Error ? e.message : 'Sign up failed.';
      } finally {
        submitting = false;
        rerender();
      }
    };

    const template = (): ReturnType<typeof html> =>
      appShell(html`
        <section class="form-page">
          <h2>Sign up</h2>
          ${error ? html`<p data-testid="form-error" class="field-error">${error}</p>` : null}
          <form
            @submit=${(e: Event) => {
              e.preventDefault();
              void onSubmit();
            }}
          >
            ${formField({
              id: 'email',
              label: 'Email',
              type: 'email',
              value: email,
              autocomplete: 'email',
              onInput: (e) => {
                email = (e.target as HTMLInputElement).value;
              },
            })}
            ${formField({
              id: 'username',
              label: 'Username',
              value: username,
              autocomplete: 'username',
              maxLength: 20,
              onInput: (e) => {
                username = (e.target as HTMLInputElement).value;
              },
            })}
            ${formField({
              id: 'password',
              label: 'Password',
              type: 'password',
              value: password,
              autocomplete: 'new-password',
              onInput: (e) => {
                password = (e.target as HTMLInputElement).value;
              },
            })}
            <div class="form-actions">
              ${button({
                label: submitting ? 'Creating account…' : 'Sign up',
                onClick: () => {},
                variant: 'primary',
                disabled: submitting,
              })}
            </div>
          </form>
          <p>Have an account? <a href="/login" data-link>Sign in</a></p>
        </section>
      `);

    const tagSubmit = (): void => {
      const btn = root.querySelector<HTMLButtonElement>('.form-actions .btn--primary');
      if (btn && !btn.hasAttribute('data-testid')) btn.setAttribute('data-testid', 'submit');
      if (btn) btn.setAttribute('type', 'submit');
    };
    const rerender = (): void => {
      render(template(), root);
      tagSubmit();
    };
    rerender();
    return () => render(html``, root);
  },
};
```

- [ ] **Step 3: Run test**

Run: `pnpm test:component`
Expected: 3 cases pass.

- [ ] **Step 4: Commit**

```bash
git add src/routes/signup.ts tests/component/signup.spec.ts
git commit -m "feat: signup route with client-side validation"
```

---

## Task 7: OAuth complete route

**Files:**

- Create: `src/routes/oauth-complete.ts`

No component test — the route's behavior is "if `session.completeOauth(username)` succeeds, navigate; if not, show the error inline" which is mechanically identical to login/signup. Covered manually + E2E (Task 10) doesn't exercise OAuth.

- [ ] **Step 1: Implement `src/routes/oauth-complete.ts`**

```ts
import { html, render } from 'lit-html';
import { appShell } from '../ui/templates';
import { formField } from '../ui/components/form-field';
import { button } from '../ui/components/button';
import { session } from '../state/session';
import { ApiError } from '../api/client';
import { navigateTo } from '../lib/util';
import type { RouteModule } from '../router';

export const oauthComplete: RouteModule = {
  render: () => {
    const root = document.getElementById('root');
    if (!root) return () => {};

    let username = '';
    let error: string | null = null;
    let submitting = false;

    const onSubmit = async (): Promise<void> => {
      if (submitting) return;
      if (!/^[a-zA-Z0-9_]{2,20}$/.test(username)) {
        error = 'Username must be 2-20 letters/numbers/underscores.';
        rerender();
        return;
      }
      submitting = true;
      error = null;
      rerender();
      try {
        await session.completeOauth(username);
        navigateTo('/');
      } catch (e) {
        error = e instanceof ApiError ? e.message : 'Could not complete sign-in.';
      } finally {
        submitting = false;
        rerender();
      }
    };

    const template = (): ReturnType<typeof html> =>
      appShell(html`
        <section class="form-page">
          <h2>Choose a username</h2>
          <p>You're almost in. Pick a public username to finish creating your account.</p>
          ${error ? html`<p class="field-error">${error}</p>` : null}
          <form
            @submit=${(e: Event) => {
              e.preventDefault();
              void onSubmit();
            }}
          >
            ${formField({
              id: 'username',
              label: 'Username',
              value: username,
              autocomplete: 'username',
              maxLength: 20,
              onInput: (e) => {
                username = (e.target as HTMLInputElement).value;
              },
            })}
            <div class="form-actions">
              ${button({
                label: submitting ? 'Finishing…' : 'Continue',
                onClick: () => {},
                variant: 'primary',
                disabled: submitting,
              })}
            </div>
          </form>
        </section>
      `);

    const rerender = (): void => {
      render(template(), root);
      const btn = root.querySelector<HTMLButtonElement>('.form-actions .btn--primary');
      if (btn) btn.setAttribute('type', 'submit');
    };
    rerender();
    return () => render(html``, root);
  },
};
```

- [ ] **Step 2: Type-check**

Run: `pnpm tsc --noEmit -p tsconfig.json`
Expected: succeeds.

- [ ] **Step 3: Commit**

```bash
git add src/routes/oauth-complete.ts
git commit -m "feat: oauth-complete username picker"
```

---

## Task 8: Header updates + avatar menu

**Files:**

- Modify: `src/ui/components/header.ts`, `src/ui/design.css`
- Create: `src/ui/components/avatar-menu.ts`, `tests/component/header.spec.ts`

- [ ] **Step 1: Create `src/ui/components/avatar-menu.ts`**

```ts
import { html, type TemplateResult } from 'lit-html';
import { session } from '../../state/session';
import { navigateTo } from '../../lib/util';
import type { User } from '../../state/user-types';

export function avatarMenu(user: User): TemplateResult {
  return html`<details class="avatar-menu" data-testid="avatar-menu">
    <summary class="avatar-menu__btn">${user.display_name || user.username}</summary>
    <ul class="avatar-menu__list">
      <li><a href=${`/u/${user.username}`} data-link>My profile</a></li>
      <li><a href="/me" data-link>Settings</a></li>
      <li>
        <button
          type="button"
          @click=${() => {
            void session.logout().then(() => navigateTo('/'));
          }}
        >
          Sign out
        </button>
      </li>
    </ul>
  </details>`;
}
```

- [ ] **Step 2: Rewrite `src/ui/components/header.ts`**

```ts
import { html, type TemplateResult } from 'lit-html';
import { session } from '../../state/session';
import { avatarMenu } from './avatar-menu';

export function header(): TemplateResult {
  const user = session.currentUser.value;
  return html`<header class="site-header">
    <a class="site-title" href="/" data-link>Spades</a>
    <nav class="site-nav">
      ${user
        ? avatarMenu(user)
        : html`<a class="btn btn--secondary" href="/login" data-link data-testid="sign-in"
            >Sign in</a
          >`}
    </nav>
  </header>`;
}
```

The header reads `session.currentUser.value` directly; routes re-render via their own effects when state changes, so the header re-renders as part of `appShell`.

- [ ] **Step 3: Add avatar-menu styles to `src/ui/design.css`**

Append:

```css
.avatar-menu {
  position: relative;
}
.avatar-menu__btn {
  cursor: pointer;
  padding: var(--space-2) var(--space-3);
  border-radius: var(--radius-md);
  background: rgba(0, 0, 0, 0.06);
}
.avatar-menu__list {
  position: absolute;
  right: 0;
  top: calc(100% + 4px);
  background: white;
  border: 1px solid rgba(0, 0, 0, 0.1);
  border-radius: var(--radius-md);
  list-style: none;
  margin: 0;
  padding: var(--space-2);
  min-width: 160px;
  z-index: 5;
}
.avatar-menu__list li {
  margin: 0;
}
.avatar-menu__list a,
.avatar-menu__list button {
  display: block;
  width: 100%;
  padding: var(--space-2);
  background: none;
  border: none;
  text-align: left;
  cursor: pointer;
  font: inherit;
  color: inherit;
  text-decoration: none;
}
.avatar-menu__list a:hover,
.avatar-menu__list button:hover {
  background: rgba(0, 0, 0, 0.06);
}
```

- [ ] **Step 4: Write failing test**

`tests/component/header.spec.ts`:

```ts
import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest';
import { html, render } from 'lit-html';
import { header } from '../../src/ui/components/header';
import { session } from '../../src/state/session';

describe('header', () => {
  beforeEach(() => {
    document.body.innerHTML = '<main id="root"></main>';
    session.currentUser.value = null;
  });
  afterEach(() => vi.restoreAllMocks());

  it('shows Sign in when anonymous', () => {
    render(html`${header()}`, document.getElementById('root')!);
    expect(document.querySelector('[data-testid=sign-in]')).not.toBeNull();
    expect(document.querySelector('[data-testid=avatar-menu]')).toBeNull();
  });

  it('shows avatar menu when signed in, with display_name preferred', () => {
    session.currentUser.value = {
      id: 'u1',
      username: 'alice',
      email: 'a@x',
      display_name: 'Alice',
      email_verified: true,
      created_at: '2026',
    };
    render(html`${header()}`, document.getElementById('root')!);
    const menu = document.querySelector('[data-testid=avatar-menu]')!;
    expect(menu.querySelector('summary')?.textContent?.trim()).toBe('Alice');
  });

  it('falls back to username when no display_name', () => {
    session.currentUser.value = {
      id: 'u1',
      username: 'alice',
      email: 'a@x',
      email_verified: true,
      created_at: '2026',
    };
    render(html`${header()}`, document.getElementById('root')!);
    expect(document.querySelector('[data-testid=avatar-menu] summary')?.textContent?.trim()).toBe(
      'alice',
    );
  });
});
```

- [ ] **Step 5: Run test**

Run: `pnpm test:component`
Expected: 3 cases pass.

- [ ] **Step 6: Commit**

```bash
git add src/ui/components/header.ts src/ui/components/avatar-menu.ts src/ui/design.css tests/component/header.spec.ts
git commit -m "feat: header chrome with sign-in / avatar menu"
```

---

## Task 9: Settings route (`/me`)

**Files:**

- Create: `src/routes/settings.ts`, `tests/component/settings.spec.ts`

- [ ] **Step 1: Write failing test**

`tests/component/settings.spec.ts`:

```ts
import { describe, it, expect, beforeEach, vi, afterEach } from 'vitest';
import { settings } from '../../src/routes/settings';
import { session } from '../../src/state/session';

describe('settings route', () => {
  beforeEach(() => {
    document.body.innerHTML = '<main id="root"></main>';
    session.currentUser.value = {
      id: 'u1',
      username: 'alice',
      email: 'a@x',
      display_name: 'Alice',
      email_verified: true,
      created_at: '2026',
    };
  });
  afterEach(() => vi.restoreAllMocks());

  it('redirects to /login?next=/me when anonymous', () => {
    session.currentUser.value = null;
    const pushSpy = vi.spyOn(history, 'pushState');
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
      if (n != null) session.currentUser.value = { ...session.currentUser.value!, display_name: n };
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
```

- [ ] **Step 2: Implement `src/routes/settings.ts`**

```ts
import { html, render } from 'lit-html';
import { effect } from '@preact/signals-core';
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
    if (!session.currentUser.value) {
      navigateTo('/login?next=/me');
      return () => {};
    }

    let displayName = session.currentUser.value.display_name ?? '';
    let saving = false;
    let error: string | null = null;
    let saved = false;

    const onSave = async (): Promise<void> => {
      if (saving) return;
      saving = true;
      error = null;
      saved = false;
      rerender();
      try {
        await session.updateDisplayName(displayName.trim() || null);
        saved = true;
      } catch (e) {
        error = e instanceof Error ? e.message : 'Could not save.';
      } finally {
        saving = false;
        rerender();
      }
    };

    const template = (): ReturnType<typeof html> => {
      const u = session.currentUser.value;
      if (!u) return html``;
      return appShell(html`
        <section class="form-page">
          <h2>Settings</h2>
          <p>Signed in as <strong>${u.username}</strong> (${u.email})</p>
          ${error ? html`<p class="field-error">${error}</p>` : null}
          ${saved ? html`<p style="color: var(--color-accent)">Saved.</p>` : null}
          ${formField({
            id: 'display_name',
            label: 'Display name (shown in games)',
            value: displayName,
            maxLength: 20,
            placeholder: u.username,
            onInput: (e) => {
              displayName = (e.target as HTMLInputElement).value;
            },
          })}
          <div class="form-actions">
            ${button({
              label: saving ? 'Saving…' : 'Save',
              onClick: () => void onSave(),
              variant: 'primary',
              disabled: saving,
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

    const rerender = (): void => {
      render(template(), root);
      tagSave();
    };

    const dispose = effect(() => {
      // Re-render whenever currentUser changes (e.g., after save).
      void session.currentUser.value;
      rerender();
    });

    return () => {
      dispose();
      render(html``, root);
    };
  },
};
```

- [ ] **Step 3: Run test**

Run: `pnpm test:component`
Expected: 3 cases pass.

- [ ] **Step 4: Commit**

```bash
git add src/routes/settings.ts tests/component/settings.spec.ts
git commit -m "feat: /me settings — display name + sign out"
```

---

## Task 10: Profile route (`/u/:username`)

**Files:**

- Create: `src/routes/profile.ts`, `tests/component/profile.spec.ts`

- [ ] **Step 1: Write failing test**

`tests/component/profile.spec.ts`:

```ts
import { describe, it, expect, beforeEach, vi, afterEach } from 'vitest';
import { profile } from '../../src/routes/profile';

describe('profile route', () => {
  beforeEach(() => {
    document.body.innerHTML = '<main id="root"></main>';
    vi.unstubAllGlobals();
  });
  afterEach(() => vi.restoreAllMocks());

  it('renders the username and games list on success', async () => {
    const calls: string[] = [];
    vi.stubGlobal(
      'fetch',
      vi.fn(async (url: string) => {
        calls.push(url);
        if (url.endsWith('/users/alice')) {
          return new Response(
            JSON.stringify({
              username: 'alice',
              display_name: 'Alice',
              created_at: '2026',
              games_played: 7,
              games_won: 4,
            }),
            { status: 200, headers: { 'content-type': 'application/json' } },
          );
        }
        if (url.endsWith('/users/alice/games')) {
          return new Response(
            JSON.stringify([
              {
                game_id: 'g1',
                started_at: '2026-05-01',
                ended_at: '2026-05-01',
                team: 'A',
                won: true,
                score: 510,
              },
            ]),
            { status: 200, headers: { 'content-type': 'application/json' } },
          );
        }
        return new Response('not found', { status: 404 });
      }),
    );
    const cleanup = profile.render(
      { username: 'alice' },
      { path: '/u/alice', search: new URLSearchParams() },
    );
    await new Promise((r) => setTimeout(r, 0));
    await new Promise((r) => setTimeout(r, 0));
    expect(document.body.textContent).toContain('alice');
    expect(document.body.textContent).toContain('g1');
    cleanup();
  });

  it('shows not-found on 404', async () => {
    vi.stubGlobal(
      'fetch',
      vi.fn(async () => new Response('not found', { status: 404 })),
    );
    const cleanup = profile.render(
      { username: 'ghost' },
      { path: '/u/ghost', search: new URLSearchParams() },
    );
    await new Promise((r) => setTimeout(r, 0));
    await new Promise((r) => setTimeout(r, 0));
    expect(document.body.textContent).toContain('not found');
    cleanup();
  });
});
```

- [ ] **Step 2: Implement `src/routes/profile.ts`**

```ts
import { html, render } from 'lit-html';
import { appShell } from '../ui/templates';
import { request, ApiError } from '../api/client';
import type { ProfileResponse, GameHistoryItem } from '../state/user-types';
import type { RouteModule } from '../router';

export const profile: RouteModule<{ username: string }> = {
  render: (params) => {
    const root = document.getElementById('root');
    if (!root) return () => {};

    let prof: ProfileResponse | null = null;
    let games: GameHistoryItem[] = [];
    let loading = true;
    let notFound = false;
    let error: string | null = null;

    const renderState = (): void => {
      render(
        appShell(html`
          <section class="profile-page">
            ${loading
              ? html`<p>Loading…</p>`
              : notFound
                ? html`<h2>Not found</h2>
                    <p>No player named <code>${params.username}</code>.</p>`
                : error
                  ? html`<p class="field-error">${error}</p>`
                  : prof
                    ? html`
                        <h2>${prof.display_name || prof.username}</h2>
                        <p>
                          ${'games_played' in prof
                            ? `${(prof as { games_played: number }).games_played} games played`
                            : null}
                        </p>
                        <h3>Recent games</h3>
                        ${games.length === 0
                          ? html`<p>No games yet.</p>`
                          : html`<ul class="profile-games">
                              ${games.map(
                                (g) =>
                                  html`<li>
                                    <a href=${`/play/${g.game_id}`} data-link>${g.game_id}</a>
                                    <span> — ${g.won ? 'Won' : 'Lost'} (Team ${g.team})</span>
                                  </li>`,
                              )}
                            </ul>`}
                      `
                    : null}
          </section>
        `),
        root,
      );
    };

    renderState();
    void (async () => {
      try {
        prof = await request<ProfileResponse>(`/users/${encodeURIComponent(params.username)}`, {
          method: 'GET',
        });
        games = await request<GameHistoryItem[]>(
          `/users/${encodeURIComponent(params.username)}/games`,
          { method: 'GET' },
        );
      } catch (e) {
        if (e instanceof ApiError && e.status === 404) notFound = true;
        else error = e instanceof Error ? e.message : 'Failed to load profile.';
      } finally {
        loading = false;
        renderState();
      }
    })();

    return () => render(html``, root);
  },
};
```

- [ ] **Step 3: Add profile styles to `src/ui/design.css`**

Append:

```css
.profile-page {
  width: 100%;
  max-width: 480px;
}
.profile-games {
  list-style: none;
  padding: 0;
}
.profile-games li {
  padding: var(--space-2) 0;
  border-bottom: 1px solid rgba(0, 0, 0, 0.06);
}
```

- [ ] **Step 4: Run test**

Run: `pnpm test:component`
Expected: 2 cases pass.

- [ ] **Step 5: Commit**

```bash
git add src/routes/profile.ts src/ui/design.css tests/component/profile.spec.ts
git commit -m "feat: public profile + game history"
```

---

## Task 11: In-game seat name prefers signed-in display name

**Files:**

- Modify: `src/routes/play.ts`

When a signed-in user joins a game, the server already attaches their display name. But the local "south" seat label in `play.ts` falls back to "You" / `playerNames[i]` from the state response. If a user is signed in **and** their session-derived name isn't yet reflected in `playerNames` (e.g., they renamed during a game), prefer `session.currentUser.value.display_name`.

- [ ] **Step 1: Modify `seatName` in `src/routes/play.ts`**

Find the `seatName` helper inside `renderInGame` and replace with:

```ts
const seatName = (idx: number): string => {
  const fromState = store.playerNames.value[idx];
  if (idx === myIdx() && session.currentUser.value?.display_name) {
    return session.currentUser.value.display_name;
  }
  return fromState ?? `Seat ${idx + 1}`;
};
```

Add at the top of `play.ts`:

```ts
import { session } from '../state/session';
```

- [ ] **Step 2: Manual smoke**

Sign in (after Task 8/9 wiring), start an AI game, verify your name shows on the south seat label.

- [ ] **Step 3: Commit**

```bash
git add src/routes/play.ts
git commit -m "feat: in-game seat uses signed-in display name"
```

---

## Task 12: E2E — signup → /me → logout → login

**Files:**

- Create: `tests/e2e/auth.spec.ts`

This test requires a running rust-spades with the auth tables. Use a unique email per run to avoid collisions (timestamp-suffixed).

- [ ] **Step 1: Write the test**

```ts
import { test, expect } from './setup';

test('signup, view /me, logout, login again', async ({ page }) => {
  const stamp = Date.now();
  const email = `e2e-${stamp}@example.com`;
  const username = `e2e_${stamp}`;

  await page.goto('/signup');
  await page.locator('#email').fill(email);
  await page.locator('#username').fill(username);
  await page.locator('#password').fill('correcthorse');
  await page.locator('[data-testid=submit]').click();

  // After signup, we're navigated to /. Header should show the avatar menu.
  await page.waitForURL(/\/$/);
  await expect(page.locator('[data-testid=avatar-menu] summary')).toHaveText(username);

  // Visit /me
  await page.goto('/me');
  await expect(page.locator('#display_name')).toBeVisible();
  await expect(page.locator('body')).toContainText(email);

  // Sign out
  await page.getByRole('button', { name: 'Sign out' }).click();
  await page.waitForURL(/\/$/);
  await expect(page.locator('[data-testid=sign-in]')).toBeVisible();

  // Log back in
  await page.goto('/login');
  await page.locator('#email').fill(email);
  await page.locator('#password').fill('correcthorse');
  await page.locator('[data-testid=submit]').click();

  await page.waitForURL(/\/$/);
  await expect(page.locator('[data-testid=avatar-menu] summary')).toHaveText(username);
});
```

- [ ] **Step 2: Run it**

Run (rust-spades on :3000 with auth enabled): `pnpm test:e2e -- auth`
Expected: passes.

- [ ] **Step 3: Commit**

```bash
git add tests/e2e/auth.spec.ts
git commit -m "test: e2e auth happy path"
```

---

## Task 13: E2E — public profile shows a completed game

**Files:**

- Create: `tests/e2e/profile.spec.ts`

This is the trickiest E2E because a game completion needs four players reaching the win threshold. The simplest path is to bypass UI play by calling the server's transition endpoint repeatedly. To stay frontend-focused, we can rely on a low `max_points` (the smallest the server accepts) and a single-human AI game — but rust-spades' AI games still take many tricks to complete.

**Decision:** Defer this test if you don't have a server-side helper. If rust-spades has a `/test/seed` or similar endpoint behind a flag, use it. If not, mark this task **OPTIONAL** and ship Plan 3 without it. Plan 4 (polish) is a fine place to revisit once the project has either a test seam or a deterministic short game mode.

- [ ] **Step 1 (if a server seed exists)**

```ts
import { test, expect } from './setup';

test('profile lists a completed game', async ({ page, request }) => {
  const stamp = Date.now();
  const username = `prof_${stamp}`;
  // Seed: create user + insert a completed game. Adjust to the server's actual contract.
  const seeded = await request.post('/test/seed-completed-game', {
    data: { username, email: `${username}@example.com`, password: 'correcthorse' },
  });
  expect(seeded.ok()).toBeTruthy();

  await page.goto(`/u/${username}`);
  await expect(page.locator('.profile-games li')).toHaveCount(1, { timeout: 5_000 });
});
```

- [ ] **Step 2 (if no server seed)**

Skip this task; add it to Plan 4. Document the gap in the commit:

```bash
echo "TODO(plan-4): E2E profile + history once server has a seed endpoint or short-game mode." >> docs/superpowers/plans/2026-05-11-plan-3-account-aware-ux.md
git add docs/superpowers/plans/2026-05-11-plan-3-account-aware-ux.md
git commit -m "docs: note deferred profile e2e"
```

---

## Self-review

**Spec coverage (Phase 3 of the design doc):**

- `state/session.ts` → Task 2 ✓
- Header chrome (sign in / avatar) → Task 8 ✓
- `routes/login.ts`, `routes/signup.ts` → Tasks 5-6 ✓
- `routes/oauth-complete.ts` → Task 7 ✓
- `routes/settings.ts` → Task 9 ✓
- `routes/profile.ts` → Task 10 ✓
- Signed-in display name in-game → Task 11 ✓
- E2E auth happy path → Task 12 ✓
- E2E profile + history → Task 13 (optional/deferred) ⚠

**Out of scope, per spec:** email verification, password reset.

**Placeholder scan:**

- Task 1 Step 1 says "If any of these isn't present in the generated schema after Phase 0, switch the offending line" — this is conditional, not a placeholder; the fallback code is fully spelled out.
- Task 13 explicitly marks itself optional with a defined deferral path. Acceptable.
- No other "TODO"/"fill in"/"similar to X" patterns.

**Type consistency:**

- `User` exported from `src/state/user-types.ts`; used identically by `session.ts`, `header.ts`, `avatar-menu.ts`, `settings.ts`. The `display_name` field is `string | null | undefined` — all consumers treat `null`/`undefined`/empty-string equivalently as "use username".
- `RouteModule<P>` generic — `profile` uses `<{ username: string }>`; others omit the type param (defaults to `Record<string, string>`).
- `session.startOauth(provider, next)` — `provider` typed `'google' | 'github'`; `login.ts` calls with literal types matching.

**Open caveats for the reviewer:**

- The "tag the submit button with `data-testid`" trick in `login.ts`/`signup.ts`/`settings.ts` is a workaround because the `button({})` helper doesn't accept arbitrary attrs. Two cleaner options: (a) extend `button({})` with an `attrs?: Record<string, string>` slot — small refactor; or (b) build the submit `<button type="submit">` inline in those routes. I went with the workaround to keep the helper API tight. Feel free to flip to (a) — it would shave a small amount of code from each form.
- `oauth-complete.ts` has no component test. The flow is identical structurally to login/signup and depends on the HttpOnly cookie + server state — best validated manually + an integration test against a real provider stub if you ever add one. Not worth a brittle mock.
- The OAuth-pending detection in `main.ts` uses a localStorage sentinel (`spades_oauth_in_progress`) set just before the browser navigates away. If the user opens the OAuth flow in a different tab/window or clears storage mid-flow, the detour to `/auth/oauth/complete` won't fire automatically — they'll land on `/` and need to navigate manually. Acceptable given OAuth-pending users see no avatar menu (no session) so the only useful thing they can do _is_ finish signup. If this bites in practice, the fix is a tiny "complete sign-in" banner on `/` when `/auth/me` 401s and the user has any pending evidence; defer to Plan 4 if it shows up.
- `Task 11`'s seat-name change is conservative: it prefers signed-in `display_name` only for the local player. Other seats stay driven by `playerNames` from the server. Matches existing behavior — the server controls all names except the user's own current label.
