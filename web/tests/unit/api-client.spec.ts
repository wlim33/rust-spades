import { describe, it, expect, vi, beforeEach } from 'vitest';
import { ApiError, request } from '../../src/api/client';

describe('api client', () => {
  beforeEach(() => {
    vi.unstubAllGlobals();
  });

  it('throws ApiError on 4xx with parsed JSON message', async () => {
    vi.stubGlobal(
      'fetch',
      vi.fn(
        async () =>
          new Response(JSON.stringify({ error: 'bad name' }), {
            status: 400,
            headers: { 'content-type': 'application/json' },
          }),
      ),
    );
    const err = await request('/games/foo', { method: 'GET' }).catch((e: unknown) => e);
    expect(err).toBeInstanceOf(ApiError);
    expect(err).toMatchObject({ status: 400, message: 'bad name' });
  });

  it('throws ApiError on 5xx with statusText fallback', async () => {
    vi.stubGlobal(
      'fetch',
      vi.fn(
        async () =>
          new Response('boom', {
            status: 503,
            statusText: 'Service Unavailable',
          }),
      ),
    );
    const err = await request('/games/foo', { method: 'GET' }).catch((e: unknown) => e);
    expect(err).toBeInstanceOf(ApiError);
    expect(err).toMatchObject({ status: 503, message: 'Service Unavailable' });
  });

  it('returns parsed JSON on 2xx', async () => {
    vi.stubGlobal(
      'fetch',
      vi.fn(
        async () =>
          new Response(JSON.stringify({ ok: true }), {
            status: 200,
            headers: { 'content-type': 'application/json' },
          }),
      ),
    );
    const data = await request<{ ok: boolean }>('/games/foo', { method: 'GET' });
    expect(data).toEqual({ ok: true });
  });

  it('sends credentials: include', async () => {
    const spy = vi.fn(
      async () =>
        new Response('null', { status: 200, headers: { 'content-type': 'application/json' } }),
    );
    vi.stubGlobal('fetch', spy);
    await request('/foo', { method: 'GET' });
    const init = (spy.mock.calls[0] as unknown as [string, RequestInit])[1];
    expect(init.credentials).toBe('include');
  });
});
