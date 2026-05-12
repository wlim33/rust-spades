import { describe, it, expect, vi, beforeEach } from 'vitest';
import { openSse } from '../../src/api/sse';

function makeStreamingResponse(chunks: string[]): Response {
  const encoder = new TextEncoder();
  const stream = new ReadableStream<Uint8Array>({
    async start(controller) {
      for (const c of chunks) {
        controller.enqueue(encoder.encode(c));
        await new Promise((r) => setTimeout(r, 0));
      }
      controller.close();
    },
  });
  return new Response(stream, { status: 200, headers: { 'content-type': 'text/event-stream' } });
}

describe('openSse', () => {
  beforeEach(() => vi.unstubAllGlobals());

  it('parses event + data pairs across chunk boundaries', async () => {
    vi.stubGlobal(
      'fetch',
      vi.fn(async () =>
        makeStreamingResponse([
          'event: queue_status\ndata: {"waiti',
          'ng":2}\n\nevent: game_start\ndata: {"game_id":"abc"}\n\n',
        ]),
      ),
    );

    const events: Array<{ type: string; data: string }> = [];
    await new Promise<void>((resolve) => {
      const sse = openSse('/matchmaking/seek', undefined, {
        onEvent: (type, data) => {
          events.push({ type, data });
          if (events.length === 2) resolve();
        },
      });
      // eventually
      setTimeout(() => sse.close(), 1000);
    });

    expect(events).toEqual([
      { type: 'queue_status', data: '{"waiting":2}' },
      { type: 'game_start', data: '{"game_id":"abc"}' },
    ]);
  });

  it('close() is idempotent and suppresses AbortError', async () => {
    vi.stubGlobal(
      'fetch',
      vi.fn(async (_url, init) => {
        const signal = (init as RequestInit).signal as AbortSignal;
        const stream = new ReadableStream<Uint8Array>({
          start(controller) {
            signal.addEventListener('abort', () =>
              controller.error(new DOMException('aborted', 'AbortError')),
            );
          },
        });
        return new Response(stream, {
          status: 200,
          headers: { 'content-type': 'text/event-stream' },
        });
      }),
    );

    const errors: unknown[] = [];
    const sse = openSse('/x', undefined, {
      onEvent: () => {},
      onError: (e) => errors.push(e),
    });
    sse.close();
    sse.close();
    await new Promise((r) => setTimeout(r, 10));
    expect(errors.length).toBe(0);
  });
});
