import type { APIRequestContext } from '@playwright/test';
import { uniqueUser, type TestUser } from './identity';

/**
 * Registers a user via POST /auth/register. Pass a context-bound request
 * (page.request / context.request) so the session cookie lands in the
 * browser context that will navigate the app.
 */
export async function registerUser(
  request: APIRequestContext,
  user: TestUser = uniqueUser(),
): Promise<TestUser> {
  const res = await request.post('/auth/register', {
    data: { username: user.username, email: user.email, password: user.password },
  });
  if (res.status() === 429) {
    throw new Error(
      'register rate-limited (HTTP 429). See plan "Known constraints": too many ' +
        'registrations from 127.0.0.1 (burst 20 / 3-per-min).',
    );
  }
  if (!res.ok()) {
    throw new Error(`register failed: ${res.status()} ${await res.text()}`);
  }
  return user;
}
