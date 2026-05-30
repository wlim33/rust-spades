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

/** Requests that hang longer than this abort with a 408 ApiError. */
export const REQUEST_TIMEOUT_MS = 12_000;

/**
 * Low-level request used by SSE/WS helpers and for endpoints not yet typed.
 * Routes that have generated types should use `api.GET/POST/...` directly.
 *
 * Aborts with a 408 `ApiError` after {@link REQUEST_TIMEOUT_MS} so a hung
 * connection can't pin a request open forever. A caller-supplied
 * `init.signal` is still honoured for earlier cancellation.
 */
export async function request<T>(path: string, init: RequestInit): Promise<T> {
  const controller = new AbortController();
  let timedOut = false;
  const timer = setTimeout(() => {
    timedOut = true;
    controller.abort();
  }, REQUEST_TIMEOUT_MS);

  // Forward a caller-provided signal so callers can still cancel early.
  const caller = init.signal;
  const onCallerAbort = (): void => controller.abort();
  if (caller) {
    if (caller.aborted) controller.abort();
    else caller.addEventListener('abort', onCallerAbort, { once: true });
  }

  try {
    const res = await fetch(`${API_URL}${path}`, {
      ...init,
      credentials: 'include',
      headers: {
        'Content-Type': 'application/json',
        ...(init.headers ?? {}),
      },
      signal: controller.signal,
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
  } catch (e) {
    if (timedOut) throw new ApiError(408, 'Request timed out');
    throw e;
  } finally {
    clearTimeout(timer);
    if (caller) caller.removeEventListener('abort', onCallerAbort);
  }
}
