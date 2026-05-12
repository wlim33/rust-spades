import createClient from 'openapi-fetch';
import type { paths } from './schema';
import { API_URL } from '../lib/util';

export class ApiError extends Error {
  constructor(
    public readonly status: number,
    message: string,
  ) {
    super(message);
    this.name = 'ApiError';
  }
}

export const api = createClient<paths>({
  baseUrl: API_URL,
  credentials: 'include',
});

/**
 * Low-level request used by SSE/WS helpers and for endpoints not yet typed.
 * Routes that have generated types should use `api.GET/POST/...` directly.
 */
export async function request<T>(path: string, init: RequestInit): Promise<T> {
  const res = await fetch(`${API_URL}${path}`, {
    ...init,
    credentials: 'include',
    headers: {
      'Content-Type': 'application/json',
      ...(init.headers ?? {}),
    },
  });
  if (!res.ok) {
    let message = res.statusText;
    try {
      const body = (await res.clone().json()) as { error?: string; message?: string };
      message = body.error ?? body.message ?? message;
    } catch {
      // body wasn't JSON; keep statusText
    }
    throw new ApiError(res.status, message);
  }
  const contentType = res.headers.get('content-type') ?? '';
  if (contentType.includes('application/json')) {
    return (await res.json()) as T;
  }
  return undefined as T;
}
