import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { openGameWs } from '../../src/api/ws';

class MockWebSocket {
  static instances: MockWebSocket[] = [];
  url: string;
  readyState = 0;
  onopen: (() => void) | null = null;
  onclose: (() => void) | null = null;
  onmessage: ((e: { data: string }) => void) | null = null;
  onerror: ((e: unknown) => void) | null = null;
  constructor(url: string) {
    this.url = url;
    MockWebSocket.instances.push(this);
  }
  close(): void {
    this.readyState = 3;
    this.onclose?.();
  }
}

const latest = (): MockWebSocket => MockWebSocket.instances[MockWebSocket.instances.length - 1]!;

describe('openGameWs reconnect', () => {
  beforeEach(() => {
    MockWebSocket.instances = [];
    vi.useFakeTimers();
    vi.stubGlobal('WebSocket', MockWebSocket as unknown as typeof WebSocket);
    // Deterministic jitter.
    vi.spyOn(Math, 'random').mockReturnValue(0.5);
  });
  afterEach(() => {
    vi.useRealTimers();
    vi.unstubAllGlobals();
    vi.restoreAllMocks();
  });

  it('reconnects after a backoff delay when the socket closes unexpectedly', async () => {
    openGameWs('g1', 'p1', { onEvent: () => {} });
    expect(MockWebSocket.instances).toHaveLength(1);

    latest().onclose?.(); // server drops us
    // Should not reconnect synchronously — it waits for the backoff.
    expect(MockWebSocket.instances).toHaveLength(1);

    await vi.advanceTimersByTimeAsync(1000);
    expect(MockWebSocket.instances).toHaveLength(2);
  });

  it('does not give up on the first close, only after exhausting retries', async () => {
    const onClose = vi.fn();
    openGameWs('g1', 'p1', { onEvent: () => {}, onClose });

    latest().onclose?.();
    expect(onClose).not.toHaveBeenCalled(); // reconnecting, not giving up

    for (let i = 0; i < 8; i++) {
      await vi.advanceTimersByTimeAsync(30_000);
      latest().onclose?.();
    }
    expect(onClose).toHaveBeenCalled();
    // Bounded: one initial socket plus a capped number of retries.
    expect(MockWebSocket.instances.length).toBeLessThanOrEqual(8);
  });

  it('does not reconnect after the caller closes the handle', async () => {
    const handle = openGameWs('g1', 'p1', { onEvent: () => {} });
    handle.close();
    await vi.advanceTimersByTimeAsync(30_000);
    expect(MockWebSocket.instances).toHaveLength(1);
  });
});
