import { API_URL } from '../lib/util';

export type SseHandle = { close(): void };

export type SseOptions = {
  method?: 'GET' | 'POST';
  onEvent: (type: string, data: string) => void;
  onError?: (err: unknown) => void;
};

export function openSse<Body>(path: string, body: Body | undefined, opts: SseOptions): SseHandle {
  const controller = new AbortController();
  let closed = false;

  const close = (): void => {
    if (closed) return;
    closed = true;
    controller.abort();
  };

  void (async () => {
    try {
      const res = await fetch(`${API_URL}${path}`, {
        method: opts.method ?? 'POST',
        signal: controller.signal,
        credentials: 'include',
        headers: { 'Content-Type': 'application/json' },
        ...(body !== undefined && { body: JSON.stringify(body) }),
      });
      if (!res.ok || !res.body) {
        throw new Error(`SSE ${path}: ${res.status} ${res.statusText}`);
      }
      const reader = res.body.getReader();
      const decoder = new TextDecoder();
      let buffer = '';
      let eventType: string | null = null;
      while (true) {
        const { done, value } = await reader.read();
        if (done) break;
        buffer += decoder.decode(value, { stream: true });
        const lines = buffer.split('\n');
        buffer = lines.pop() ?? '';
        for (const line of lines) {
          if (line.startsWith('event:')) {
            eventType = line.slice(6).trim();
          } else if (line.startsWith('data:') && eventType) {
            opts.onEvent(eventType, line.slice(5).trim());
            eventType = null;
          } else if (line === '') {
            eventType = null;
          }
        }
      }
    } catch (e) {
      if (closed) return;
      if (e instanceof DOMException && e.name === 'AbortError') return;
      opts.onError?.(e);
    }
  })();

  return { close };
}
