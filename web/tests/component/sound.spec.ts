import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest';

class FakeGainNode {
  gain = {
    setValueAtTime: vi.fn(),
    linearRampToValueAtTime: vi.fn(),
    exponentialRampToValueAtTime: vi.fn(),
  };
  connect = vi.fn((target: unknown) => target);
}

class FakeOscillator {
  type = '';
  frequency = { value: 0 };
  start = vi.fn();
  stop = vi.fn();
  connect = vi.fn((target: unknown) => target);
}

class FakeAudioContext {
  state = 'running';
  currentTime = 0;
  destination = {};
  oscillators: FakeOscillator[] = [];
  resume = vi.fn();
  createOscillator(): FakeOscillator {
    const o = new FakeOscillator();
    this.oscillators.push(o);
    return o;
  }
  createGain(): FakeGainNode {
    return new FakeGainNode();
  }
}

/** In-memory localStorage stub for happy-dom / Node 26 environments. */
function makeLocalStorage(): Storage {
  const store: Record<string, string> = {};
  return {
    getItem: (k: string) => store[k] ?? null,
    setItem: (k: string, v: string) => {
      store[k] = v;
    },
    removeItem: (k: string) => {
      delete store[k];
    },
    clear: () => {
      for (const k of Object.keys(store)) delete store[k];
    },
    get length() {
      return Object.keys(store).length;
    },
    key: (i: number) => Object.keys(store)[i] ?? null,
  } as Storage;
}

let ctx: FakeAudioContext;

beforeEach(() => {
  vi.resetModules();
  ctx = new FakeAudioContext();
  vi.stubGlobal(
    'AudioContext',
    vi.fn(function () {
      return ctx;
    }),
  );
  vi.stubGlobal('localStorage', makeLocalStorage());
  localStorage.clear(); // sound pref defaults to on
});

afterEach(() => {
  vi.unstubAllGlobals();
});

describe('seatTick', () => {
  it('rises up the A-major arpeggio with each filled seat', async () => {
    const { seatTick } = await import('../../src/lib/sound');
    seatTick(1);
    seatTick(2);
    seatTick(3);
    seatTick(4);
    expect(ctx.oscillators.map((o) => o.frequency.value)).toEqual([440, 554.37, 659.25, 880]);
  });

  it('stays silent when the sound pref is off', async () => {
    localStorage.setItem('spades_sound', 'off');
    const { seatTick } = await import('../../src/lib/sound');
    seatTick(1);
    expect(ctx.oscillators).toHaveLength(0);
  });

  it('stays silent while the context is not running (autoplay policy)', async () => {
    ctx.state = 'suspended';
    const { seatTick } = await import('../../src/lib/sound');
    seatTick(1);
    expect(ctx.oscillators).toHaveLength(0);
  });
});

describe('gameStart', () => {
  it('plays a three-note rising flourish', async () => {
    const { gameStart } = await import('../../src/lib/sound');
    gameStart();
    expect(ctx.oscillators.map((o) => o.frequency.value)).toEqual([659.25, 880, 1108.73]);
  });
});

describe('chime', () => {
  it('still plays its two-note figure after the playNote refactor', async () => {
    const { chime } = await import('../../src/lib/sound');
    chime();
    expect(ctx.oscillators.map((o) => o.frequency.value)).toEqual([659.25, 880]);
  });
});
