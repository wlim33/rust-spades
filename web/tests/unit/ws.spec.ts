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

  it('reconnects when the socket goes silent while activity is expected', async () => {
    // The wedge: a turn-transfer event is lost, the socket stays OPEN (so no
    // onclose, no backoff reconnect), and the server now waits on us — nothing
    // arrives to recover. The idle watchdog breaks the deadlock.
    openGameWs('g1', 'p1', {
      onEvent: () => {},
      idleReconnectMs: 10_000,
      expectingActivity: () => true,
    });
    latest().onopen?.(); // socket opens → watchdog armed
    expect(MockWebSocket.instances).toHaveLength(1);

    await vi.advanceTimersByTimeAsync(10_000); // silence past the idle window
    expect(MockWebSocket.instances).toHaveLength(2); // forced a fresh connection
  });

  it('resets the idle timer on every received frame', async () => {
    openGameWs('g1', 'p1', {
      onEvent: () => {},
      idleReconnectMs: 10_000,
      expectingActivity: () => true,
    });
    const ws = latest();
    ws.onopen?.();

    await vi.advanceTimersByTimeAsync(6000);
    ws.onmessage?.({ data: JSON.stringify({ event: 'state_changed', seq: 1 }) }); // activity
    await vi.advanceTimersByTimeAsync(6000); // 12s total, but only 6s since the frame
    expect(MockWebSocket.instances).toHaveLength(1);

    await vi.advanceTimersByTimeAsync(4000); // now 10s since the last frame
    expect(MockWebSocket.instances).toHaveLength(2);
  });

  it('does not churn the socket when silence is expected (our turn / game over)', async () => {
    let activityExpected = false;
    openGameWs('g1', 'p1', {
      onEvent: () => {},
      idleReconnectMs: 10_000,
      expectingActivity: () => activityExpected,
    });
    latest().onopen?.();

    await vi.advanceTimersByTimeAsync(10_000);
    expect(MockWebSocket.instances).toHaveLength(1); // our turn: silence is normal

    // It keeps watching — once a move is expected elsewhere, silence triggers recovery.
    activityExpected = true;
    await vi.advanceTimersByTimeAsync(10_000);
    expect(MockWebSocket.instances).toHaveLength(2);
  });

  it('runs no watchdog when idleReconnectMs is unset (backward compatible)', async () => {
    openGameWs('g1', 'p1', { onEvent: () => {} });
    latest().onopen?.();
    await vi.advanceTimersByTimeAsync(60_000);
    expect(MockWebSocket.instances).toHaveLength(1);
  });

  it('flags only the first message after each open as a snapshot', async () => {
    // The server's initial snapshot carries a "next expected" seq; the consumer
    // needs to know which message is that snapshot so it can seed the seq cursor
    // without dropping the first real event.
    const seen: boolean[] = [];
    openGameWs('g1', 'p1', { onEvent: (_data, isSnapshot) => void seen.push(isSnapshot) });
    const ws = latest();
    ws.onopen?.();
    ws.onmessage?.({ data: JSON.stringify({ event: 'state_changed', seq: 5 }) });
    ws.onmessage?.({ data: JSON.stringify({ event: 'state_changed', seq: 6 }) });
    await vi.advanceTimersByTimeAsync(0);
    expect(seen).toEqual([true, false]);
  });
});
