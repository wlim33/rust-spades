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
  refresh,
  loginWithPassword,
  signupWithPassword,
  logout,
  startOauth,
  completeOauth,
  updateEmail,
  updatePassword,
};
