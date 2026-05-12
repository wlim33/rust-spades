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
            { status: 200, headers: { 'content-type': 'application/json' } },
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
            { status: 200, headers: { 'content-type': 'application/json' } },
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
      vi.fn(async () => new Response(null, { status: 204 })),
    );
    await session.logout();
    expect(session.currentUser.value).toBe(null);
  });
});
