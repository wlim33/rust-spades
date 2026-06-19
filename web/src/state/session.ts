import { signal } from '@preact/signals-core';
import { ApiError, request } from '../api/client';
import { markOauthInProgress } from '../lib/storage';
import type { User } from './user-types';
import { API_URL } from '../lib/util';

const currentUser = signal<User | null>(null);

// Hydration state for the first session refresh. `hydrated` resolves once the
// initial /auth/me settles (success OR failure); `isHydrating()` is true only
// while that first refresh is in flight. Auth-gated routes use these to avoid
// bouncing a signed-in user to /login before the cold-load refresh resolves.
let hydrating = false;
let settledOnce = false;
let resolveHydrated!: () => void;
const hydrated = new Promise<void>((r) => {
  resolveHydrated = r;
});

function isHydrating(): boolean {
  return hydrating;
}

async function refresh(): Promise<void> {
  hydrating = true;
  try {
    const me = await request<User>('/auth/me', { method: 'GET' });
    currentUser.value = me;
  } catch (e) {
    if (e instanceof ApiError && e.status === 401) {
      currentUser.value = null;
    } else {
      // Network / 5xx: the auth state is unknown. Leave currentUser as-is and
      // never throw — boot is best-effort and must not hinge on this, and the
      // global unhandledrejection net shouldn't fire for a hydration blip.
      console.error('session refresh failed', e);
    }
  } finally {
    hydrating = false;
    if (!settledOnce) {
      settledOnce = true;
      resolveHydrated();
    }
  }
}

async function loginWithPassword(email: string, password: string): Promise<void> {
  const user = await request<User>('/auth/login', {
    method: 'POST',
    body: JSON.stringify({ login: email, password }),
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
  window.location.assign(`${API_URL}/auth/oauth/${provider}/login`);
}

async function completeOauth(username: string): Promise<void> {
  const user = await request<User>('/auth/oauth/complete', {
    method: 'POST',
    body: JSON.stringify({ username }),
  });
  currentUser.value = user;
}

async function updateEmail(email: string, currentPassword: string): Promise<void> {
  const user = await request<User>('/users/me', {
    method: 'PATCH',
    body: JSON.stringify({ email, current_password: currentPassword }),
  });
  currentUser.value = user;
}

async function updatePassword(currentPassword: string, newPassword: string): Promise<void> {
  const user = await request<User>('/users/me', {
    method: 'PATCH',
    body: JSON.stringify({ current_password: currentPassword, new_password: newPassword }),
  });
  currentUser.value = user;
}

export const session = {
  currentUser,
  hydrated,
  isHydrating,
  refresh,
  loginWithPassword,
  signupWithPassword,
  logout,
  startOauth,
  completeOauth,
  updateEmail,
  updatePassword,
};
