# State-Cue Tokens & Lobby Team Buttons Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the lobby's border-accent team cards with single gauge buttons per team (background fill = occupancy, icons = identity, sounds = events), built on new state-cue tokens.

**Architecture:** New `--team-*-fill` area-tint tokens in `tokens.css`; a `teamButton` lit-html template component (`button.ts` idiom — no shadow DOM, classes in `design.css`); `sound.ts` grows `seatTick`/`gameStart` sharing an extracted `playNote` helper; `lobby.ts` swaps presentation only and wires sound + `announce()` into its existing SSE handlers.

**Tech Stack:** lit-html, @preact/signals-core, vitest (`unit` node env / `component` happy-dom env), Playwright e2e, plain CSS custom properties.

**Spec:** `docs/superpowers/specs/2026-06-12-state-cue-tokens-design.md`

**Conventions that apply to every task:**
- Run web commands as `pnpm -C web <script>` from the repo root.
- Cargo is untouched; no Rust changes anywhere in this plan.
- Commit with pathspecs (`git commit -- <files>`) — the repo often has unrelated WIP staged.

---

### Task 1: State-cue tokens

**Files:**
- Modify: `web/src/ui/tokens.css`

- [x] **Step 1: Add the state-cue block and deprecation note**

In `web/src/ui/tokens.css`, replace the existing team-keel comment + tokens (lines 18–20):

```css
  /* Team keels: tuned by eye (range 30-40%); higher = louder team hue. */
  --team-1: color-mix(in oklab, var(--accent) 30%, var(--fg-muted));
  --team-2: color-mix(in oklab, var(--accent-2) 30%, var(--fg-muted));
```

with:

```css
  /* Team keels: tuned by eye (range 30-40%); higher = louder team hue.
     DEPRECATED: border keels give way to --team-*-fill background cues
     (docs/superpowers/specs/2026-06-12-state-cue-tokens-design.md). Remove
     once the seat chips and scoreboard migrate. */
  --team-1: color-mix(in oklab, var(--accent) 30%, var(--fg-muted));
  --team-2: color-mix(in oklab, var(--accent-2) 30%, var(--fg-muted));

  /* State cues: state lives in the background plane; border color is
     structure only. Team colors as area fills are tints of the accents over
     the raised surface, so --fg text stays readable on top of a filled
     region. Both themes resolve through var(--accent)/var(--surface-raised),
     so no dark override is needed unless contrast retuning demands one. */
  --team-1-fill: color-mix(in oklab, var(--accent) 22%, var(--surface-raised));
  --team-2-fill: color-mix(in oklab, var(--accent-2) 22%, var(--surface-raised));
  /* Gauge motion: slower than --dur so a fill change reads as liquid rising,
     not a UI flicker. */
  --dur-cue: 400ms;
```

- [x] **Step 2: Verify formatting and lint pass**

Run: `pnpm -C web format && pnpm -C web lint`
Expected: prettier rewrites nothing unexpected; eslint exits 0.

- [x] **Step 3: Commit**

```bash
git add web/src/ui/tokens.css
git commit -m "feat(web): add state-cue fill tokens; deprecate team keels" -- web/src/ui/tokens.css
```

---

### Task 2: `user-fill` icon

**Files:**
- Create: `web/src/ui/icons/user-fill.svg`

- [x] **Step 1: Add the SVG**

`web/src/ui/icons/user-fill.svg` (Remix Icon "user-fill", same set/format as the existing `user-line.svg` — `fill="currentColor"`, 24×24 viewBox, single path, no width/height attributes):

```svg
<svg viewBox="0 0 24 24" fill="currentColor" xmlns="http://www.w3.org/2000/svg"><path d="M4 22C4 17.5817 7.58172 14 12 14C16.4183 14 20 17.5817 20 22H4ZM12 13C8.685 13 6 10.315 6 7C6 3.685 8.685 1 12 1C15.315 1 18 3.685 18 7C18 10.315 15.315 13 12 13Z"/></svg>
```

No registry edit needed — `ui/icon.ts` globs `./icons/*.svg` at build time, so the name `user-fill` is available to `icon()` automatically.

- [x] **Step 2: Commit**

```bash
git add web/src/ui/icons/user-fill.svg
git commit -m "feat(web): vendor user-fill icon (Remix)" -- web/src/ui/icons/user-fill.svg
```

---

### Task 3: `seatTick` + `gameStart` sounds

**Files:**
- Modify: `web/src/lib/sound.ts`
- Test: `web/tests/component/sound.spec.ts` (component project: happy-dom provides the `localStorage` that `getSoundPref` reads; `AudioContext` is stubbed)

- [x] **Step 1: Write the failing tests**

Create `web/tests/component/sound.spec.ts`. The fakes capture scheduled oscillator frequencies; `vi.resetModules()` + dynamic import defeat the module-level `ctx` cache in `sound.ts`. `connect()` returns its target because the implementation chains `osc.connect(gain).connect(ac.destination)`.

```ts
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

let ctx: FakeAudioContext;

beforeEach(() => {
  vi.resetModules();
  ctx = new FakeAudioContext();
  vi.stubGlobal('AudioContext', vi.fn(() => ctx));
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
```

- [x] **Step 2: Run the tests to verify they fail**

Run: `pnpm -C web test -- --project component sound`
Expected: FAIL — `seatTick`/`gameStart` are not exported.

- [x] **Step 3: Implement**

Rewrite `web/src/lib/sound.ts` (the `chime` doc comment and autoplay-guard comment carry over onto the shared helper):

```ts
import { getSoundPref } from './storage';

let ctx: AudioContext | null = null;

function getCtx(): AudioContext | null {
  try {
    ctx ??= new AudioContext();
    if (ctx.state === 'suspended') void ctx.resume();
    return ctx;
  } catch {
    return null;
  }
}

/**
 * Pref + autoplay gate shared by every cue. A suspended context (autoplay
 * policy) would queue notes and replay them garbled after the first user
 * gesture — skip instead. getCtx() has already requested resume(), so the
 * next cue after a gesture plays.
 */
function readyCtx(): AudioContext | null {
  if (!getSoundPref()) return null;
  const ac = getCtx();
  if (!ac || ac.state !== 'running') return null;
  return ac;
}

/** One soft sine note `at` seconds from now: fast attack, ~0.3 s decay. */
function playNote(ac: AudioContext, freq: number, at: number): void {
  const t0 = ac.currentTime + at;
  const osc = ac.createOscillator();
  const gain = ac.createGain();
  osc.type = 'sine';
  osc.frequency.value = freq;
  gain.gain.setValueAtTime(0, t0);
  gain.gain.linearRampToValueAtTime(0.08, t0 + 0.02);
  gain.gain.exponentialRampToValueAtTime(0.0001, t0 + 0.3);
  osc.connect(gain).connect(ac.destination);
  osc.start(t0);
  osc.stop(t0 + 0.32);
}

/**
 * Soft two-note turn chime (E5 -> A5). Best-effort: any failure — including
 * autoplay policy before the first user gesture — is silently ignored.
 */
export function chime(): void {
  const ac = readyCtx();
  if (!ac) return;
  try {
    playNote(ac, 659.25, 0);
    playNote(ac, 880, 0.12);
  } catch {
    // audio is an enhancement, never an error
  }
}

/* Lobby fill ladder: A-major arpeggio, one step per filled seat. The 4th
   seat lands on the octave, so a full lobby resolves just as the game
   becomes startable. */
const SEAT_PITCH = [440, 554.37, 659.25, 880] as const;

/** One rising tick per seat filled in the lobby (1-indexed total count). */
export function seatTick(filledSeats: 1 | 2 | 3 | 4): void {
  const ac = readyCtx();
  if (!ac) return;
  try {
    playNote(ac, SEAT_PITCH[filledSeats - 1], 0);
  } catch {
    // audio is an enhancement, never an error
  }
}

/** Three-note rising flourish (E5 -> A5 -> C#6): the game-launch cue. */
export function gameStart(): void {
  const ac = readyCtx();
  if (!ac) return;
  try {
    playNote(ac, 659.25, 0);
    playNote(ac, 880, 0.12);
    playNote(ac, 1108.73, 0.24);
  } catch {
    // audio is an enhancement, never an error
  }
}
```

- [x] **Step 4: Run the tests to verify they pass**

Run: `pnpm -C web test -- --project component sound`
Expected: PASS (5 tests).

- [x] **Step 5: Commit**

```bash
git add web/src/lib/sound.ts web/tests/component/sound.spec.ts
git commit -m "feat(web): lobby seat-tick ladder and game-start flourish" -- web/src/lib/sound.ts web/tests/component/sound.spec.ts
```

---

### Task 4: `teamButton` component + gauge CSS

**Files:**
- Create: `web/src/ui/components/team-button.ts`
- Modify: `web/src/ui/design.css` (new `.team-btn` block; do NOT touch `.team-card` yet — the lobby still renders it until Task 5)
- Test: `web/tests/component/team-button.spec.ts`

- [x] **Step 1: Write the failing test**

Create `web/tests/component/team-button.spec.ts` — the (members, joinable) state matrix:

```ts
import { describe, it, expect, beforeEach, vi } from 'vitest';
import { render } from 'lit-html';
import { teamButton, type TeamMember } from '../../src/ui/components/team-button';

function mount(opts: {
  members: TeamMember[];
  joinable: boolean;
  onJoin?: () => void;
}): HTMLButtonElement {
  render(
    teamButton({
      teamNo: '1',
      label: 'Team A',
      capacity: 2,
      onJoin: opts.onJoin ?? (() => {}),
      members: opts.members,
      joinable: opts.joinable,
    }),
    document.getElementById('root')!,
  );
  return document.querySelector<HTMLButtonElement>('.team-btn')!;
}

describe('teamButton', () => {
  beforeEach(() => {
    document.body.innerHTML = '<main id="root"></main>';
  });

  it.each([
    // members, joinable, data-fill, disabled, aria-label
    [[], true, '0', false, 'Join Team A, 0 of 2 seats filled'],
    [[{ name: 'Ada', mine: false }], true, '1', false, 'Join Team A, 1 of 2 seats filled'],
    [
      [
        { name: 'Ada', mine: false },
        { name: 'Bo', mine: false },
      ],
      false,
      '2',
      true,
      'Team A, 2 of 2 seats filled',
    ],
    // viewer already seated elsewhere: open team renders but is not joinable
    [[], false, '0', true, 'Team A, 0 of 2 seats filled'],
  ] as [TeamMember[], boolean, string, boolean, string][])(
    'members=%j joinable=%s -> fill=%s disabled=%s',
    (members, joinable, fill, disabled, label) => {
      const btn = mount({ members, joinable });
      expect(btn.getAttribute('data-fill')).toBe(fill);
      expect(btn.disabled).toBe(disabled);
      expect(btn.getAttribute('aria-label')).toBe(label);
      expect(btn.getAttribute('data-team')).toBe('1');
    },
  );

  it('renders a filled-icon row per member and an open row per empty seat', () => {
    const btn = mount({ members: [{ name: 'Ada', mine: false }], joinable: true });
    const slots = btn.querySelectorAll('.team-btn__slot');
    expect(slots).toHaveLength(2);
    expect(slots[0]!.textContent).toContain('Ada');
    expect(slots[0]!.querySelector('.icon')).toBeTruthy();
    expect(slots[1]!.classList.contains('team-btn__slot--open')).toBe(true);
    expect(slots[1]!.textContent).toContain('Open');
  });

  it('bolds my own row via the mine modifier', () => {
    const btn = mount({ members: [{ name: 'Me', mine: true }], joinable: false });
    expect(btn.querySelector('.team-btn__slot--mine')!.textContent).toContain('Me');
  });

  it('fires onJoin on click only while joinable', () => {
    const onJoin = vi.fn();
    mount({ members: [], joinable: true, onJoin }).click();
    expect(onJoin).toHaveBeenCalledTimes(1);
    const disabled = mount({ members: [], joinable: false, onJoin });
    disabled.click();
    expect(onJoin).toHaveBeenCalledTimes(1); // native disabled swallows the click
  });
});
```

- [x] **Step 2: Run the test to verify it fails**

Run: `pnpm -C web test -- --project component team-button`
Expected: FAIL — module `../../src/ui/components/team-button` does not exist.

- [x] **Step 3: Implement the component**

Create `web/src/ui/components/team-button.ts`:

```ts
import { html, type TemplateResult } from 'lit-html';
import { icon } from '../icon';

export type TeamMember = { name: string; mine: boolean };

/**
 * One button per team: the background gauge (CSS ::before keyed off
 * data-fill) carries occupancy, icon+name rows carry identity. Disabled when
 * the viewer can't join (already seated, or team full) but keeps rendering —
 * the gauge still rises as others join.
 */
export function teamButton(opts: {
  teamNo: '1' | '2';
  label: string;
  members: TeamMember[];
  capacity: number;
  joinable: boolean;
  onJoin: () => void;
}): TemplateResult {
  const filled = opts.members.length;
  const seats = `${filled} of ${opts.capacity} seats filled`;
  const aria = opts.joinable ? `Join ${opts.label}, ${seats}` : `${opts.label}, ${seats}`;
  const openSlots = Math.max(0, opts.capacity - filled);
  return html`<button
    type="button"
    class="team-btn"
    data-team=${opts.teamNo}
    data-fill=${filled}
    ?disabled=${!opts.joinable}
    aria-label=${aria}
    @click=${opts.onJoin}
  >
    <span class="team-btn__label">${opts.label}</span>
    ${opts.members.map(
      (m) =>
        html`<span class="team-btn__slot${m.mine ? ' team-btn__slot--mine' : ''}">
          ${icon('user-fill')} ${m.name}
        </span>`,
    )}
    ${Array.from(
      { length: openSlots },
      () => html`<span class="team-btn__slot team-btn__slot--open">
        ${icon('user-line')} Open
      </span>`,
    )}
  </button>`;
}
```

- [x] **Step 4: Run the test to verify it passes**

Run: `pnpm -C web test -- --project component team-button`
Expected: PASS.

- [x] **Step 5: Add the gauge CSS**

In `web/src/ui/design.css`, directly above the `.team-card` block (which Task 5 deletes), add:

```css
/* Team gauge button: the ::before layer is the occupancy gauge — background
   plane carries state, border is structure only. Content stacks above it. */
.team-btn {
  appearance: none;
  font: inherit;
  position: relative;
  overflow: hidden;
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  text-align: center;
  gap: var(--space-2);
  padding: var(--space-3);
  border-radius: var(--radius-md);
  border: 1px solid var(--border-strong);
  background: var(--surface-raised);
  color: var(--fg);
  cursor: pointer;
  box-shadow: var(--shadow-1);
  transition:
    transform var(--dur) var(--ease),
    box-shadow var(--dur) var(--ease);
}
.team-btn[data-team='1'] {
  --team-fill: var(--team-1-fill);
}
.team-btn[data-team='2'] {
  --team-fill: var(--team-2-fill);
}
.team-btn::before {
  content: '';
  position: absolute;
  inset: auto 0 0 0;
  height: 0%;
  background: var(--team-fill);
  transition: height var(--dur-cue) var(--ease);
}
/* Heights enumerate capacity 2 — the only capacity that exists; the TS
   capacity param feeds the aria-label, not this. */
.team-btn[data-fill='1']::before {
  height: 50%;
}
.team-btn[data-fill='2']::before {
  height: 100%;
}
.team-btn > * {
  position: relative;
}
.team-btn:hover:not([disabled]) {
  transform: translateY(-2px);
  box-shadow: var(--shadow-2);
}
.team-btn:active:not([disabled]) {
  transform: translateY(1px);
}
/* Disabled keeps the raised look and a live gauge; only the affordance goes.
   (No opacity dim — that would mute the fill cue itself.) */
.team-btn[disabled] {
  cursor: default;
}
.team-btn__label {
  font-size: var(--text-lg);
  font-weight: 600;
}
.team-btn__slot {
  display: inline-flex;
  align-items: center;
  gap: var(--space-2);
}
.team-btn__slot--open {
  color: var(--fg-muted);
}
.team-btn__slot--mine {
  font-weight: 600;
}
@media (prefers-reduced-motion: reduce) {
  .team-btn::before {
    transition: none;
  }
}
```

- [x] **Step 6: Lint, format, full component suite**

Run: `pnpm -C web format && pnpm -C web lint && pnpm -C web test -- --project component`
Expected: all green.

- [x] **Step 7: Commit**

```bash
git add web/src/ui/components/team-button.ts web/src/ui/design.css web/tests/component/team-button.spec.ts
git commit -m "feat(web): team gauge button component" -- web/src/ui/components/team-button.ts web/src/ui/design.css web/tests/component/team-button.spec.ts
```

---

### Task 5: Lobby integration

**Files:**
- Modify: `web/src/routes/lobby.ts`
- Modify: `web/src/ui/design.css` (delete the `.team-card*` rules)
- Modify: `web/tests/component/lobby.spec.ts`
- Modify: `web/tests/e2e/pages/lobby-page.ts:8`

- [x] **Step 1: Update the lobby component tests (failing first)**

In `web/tests/component/lobby.spec.ts`, add module mocks directly under the imports (sound and SSE are mocked for the whole file; existing tests don't touch them):

```ts
import { seatTick, gameStart } from '../../src/lib/sound';
import { openSse } from '../../src/api/sse';

vi.mock('../../src/lib/sound', () => ({
  chime: vi.fn(),
  seatTick: vi.fn(),
  gameStart: vi.fn(),
}));
vi.mock('../../src/api/sse', () => ({
  openSse: vi.fn(() => ({ close: vi.fn() })),
}));
```

Replace the first three tests (`renders one card per team…`, `offers both teams…`, `hides a full team’s join option`) with:

```ts
  it('renders one gauge button per team with two open slots each', () => {
    renderLobby(makeArgs());
    const btns = [...document.querySelectorAll('.team-grid .team-btn')];
    expect(btns).toHaveLength(2);
    expect(btns.map((b) => b.getAttribute('data-team'))).toEqual(['1', '2']);
    expect(btns.map((b) => b.getAttribute('data-fill'))).toEqual(['0', '0']);
    expect(document.querySelectorAll('.team-btn__slot--open')).toHaveLength(4);
  });

  it('offers both teams as joinable when open', () => {
    renderLobby(makeArgs());
    const joins = [...document.querySelectorAll<HTMLButtonElement>('.team-btn:not([disabled])')];
    expect(joins.map((b) => b.getAttribute('aria-label'))).toEqual([
      'Join Team A, 0 of 2 seats filled',
      'Join Team B, 0 of 2 seats filled',
    ]);
  });

  it('disables a full team but keeps showing its members and fill', () => {
    const args = makeArgs();
    args.initialStatus.seats = [
      { seat: 'A', player_id: 'p1', name: 'P1' },
      { seat: 'C', player_id: 'p2', name: 'P2' },
    ];
    renderLobby(args);
    const full = document.querySelector<HTMLButtonElement>('.team-btn[data-team="1"]')!;
    expect(full.disabled).toBe(true);
    expect(full.getAttribute('data-fill')).toBe('2');
    expect(full.getAttribute('aria-label')).toBe('Team A, 2 of 2 seats filled: P1, P2');
    expect(full.textContent).toContain('P1');
    expect(document.querySelector<HTMLButtonElement>('.team-btn[data-team="2"]')!.disabled).toBe(
      false,
    );
  });
```

Append a new test for the sound/announce wiring (drives the join SSE by capturing the handler the route passes to the mocked `openSse`):

```ts
  it('ticks on seat fills, stays silent on leaves, flourishes on game start', () => {
    renderLobby(makeArgs());
    // Join Team A to open the SSE and capture its event handler.
    document.querySelector<HTMLButtonElement>('.team-btn[data-team="1"]')!.click();
    const nameInput = document.querySelector<HTMLInputElement>('.join-modal input')!;
    nameInput.value = 'Me';
    nameInput.dispatchEvent(new Event('input', { bubbles: true }));
    [...document.querySelectorAll<HTMLButtonElement>('.join-modal .btn')]
      .find((b) => b.textContent?.trim() === 'Join')!
      .click();

    const sseOpts = vi.mocked(openSse).mock.calls.at(-1)![2];
    const seatUpdate = (seats: unknown) =>
      sseOpts.onEvent('seat_update', JSON.stringify({ seats }));

    seatUpdate([{ seat: 'A', player_id: 'p1', name: 'Me' }]);
    expect(seatTick).toHaveBeenCalledWith(1);

    seatUpdate([
      { seat: 'A', player_id: 'p1', name: 'Me' },
      { seat: 'B', player_id: 'p2', name: 'Ada' },
    ]);
    expect(seatTick).toHaveBeenCalledWith(2);
    // announce() writes to its shared polite live region — the tick's
    // screen-reader twin.
    expect(document.querySelector('[role="status"]')!.textContent).toBe('Ada joined Team B');

    // A leave: count decreases, no new tick.
    vi.mocked(seatTick).mockClear();
    seatUpdate([{ seat: 'A', player_id: 'p1', name: 'Me' }]);
    expect(seatTick).not.toHaveBeenCalled();

    sseOpts.onEvent('game_start', JSON.stringify({ game_id: 'g1', player_id: 'p1' }));
    expect(gameStart).toHaveBeenCalledTimes(1);
  });
```

- [x] **Step 2: Run the lobby tests to verify the new ones fail**

Run: `pnpm -C web test -- --project component lobby`
Expected: the three rewritten render tests and the sound test FAIL (no `.team-btn` yet); the copy-link test still passes.

- [x] **Step 3: Update `lobby.ts`**

In `web/src/routes/lobby.ts`:

a) Swap imports — remove `button` usage *for the team grid only* (the modal and cancel buttons keep using it), and add:

```ts
import { teamButton } from '../ui/components/team-button';
import { seatTick, gameStart } from '../lib/sound';
import { announce } from '../ui/announce';
```

b) Add a seat-update applier next to `teamOccupants` (sound + screen-reader announcement are twins of the visual gauge change):

```ts
  const applySeatUpdate = (next: ChallengeSeat[]): void => {
    const prev = seats.value;
    const filled = (l: ChallengeSeat[]): number => l.filter((s) => s !== null).length;
    const nextFilled = filled(next);
    if (nextFilled > filled(prev)) {
      seatTick(Math.min(nextFilled, 4) as 1 | 2 | 3 | 4);
      for (const s of next) {
        if (s && !prev.some((p) => p?.player_id === s.player_id)) {
          const team = TEAMS.find((t) => (t.seats as readonly string[]).includes(s.seat));
          announce(`${s.name ?? 'A player'} joined Team ${team?.id ?? ''}`);
        }
      }
    }
    seats.value = next;
  };
```

c) In the join SSE `onEvent` handler, replace `seats.value = parsed.seats as ChallengeSeat[];` with `applySeatUpdate(parsed.seats as ChallengeSeat[]);` and add `gameStart();` as the first line of the `game_start` branch.

d) In `lobbyTemplate`, delete the `defaultTeam` const and replace the whole `team-grid` div contents with:

```ts
        <div class="team-grid">
          ${TEAMS.map((t) => {
            const members = teamOccupants(t.id).map((m) => ({
              name: m.name ?? 'Player',
              mine: m.player_id === myPlayerId.value,
            }));
            return teamButton({
              teamNo: t.no,
              label: `Team ${t.id}`,
              members,
              capacity: t.seats.length,
              joinable: !myPlayerId.value && members.length < t.seats.length,
              onJoin: () => onJoinClick(t.id),
            });
          })}
        </div>
```

(The `[data-team]` attribute moves onto the button itself, so the `.team-grid [data-team]` keel-variable rules become dead — removed in Step 4.)

- [x] **Step 4: Delete the dead team-card CSS**

In `web/src/ui/design.css`, delete these now-unreferenced rules:
- `.team-grid [data-team='1']` / `.team-grid [data-team='2']` (the `--team` keel indirection)
- `.team-card`, `.team-card strong`, `.team-card__members`, `.team-card__members .mine`, `.team-card__open`

Keep `.team-grid` itself (still the two-column layout) and the deprecated `--team-1`/`--team-2` tokens (seat chips/scoreboard still use them).

- [x] **Step 5: Run the component suite**

Run: `pnpm -C web test -- --project component`
Expected: all PASS, including the rewritten lobby specs.

- [x] **Step 6: Update the e2e page object**

In `web/tests/e2e/pages/lobby-page.ts`, change line 8 from:

```ts
    await this.page.locator('.team-card .btn').first().click({ timeout: 10_000 });
```

to:

```ts
    await this.page.locator('.team-btn:not([disabled])').first().click({ timeout: 10_000 });
```

- [x] **Step 7: Run e2e**

Run: `pnpm -C web test:e2e`
Expected: PASS (auto-starts the backend; needs the one-time `playwright install chromium` already done).

- [x] **Step 8: Commit**

```bash
git add web/src/routes/lobby.ts web/src/ui/design.css web/tests/component/lobby.spec.ts web/tests/e2e/pages/lobby-page.ts
git commit -m "feat(web): lobby team gauge buttons with seat ticks and announcements" -- web/src/routes/lobby.ts web/src/ui/design.css web/tests/component/lobby.spec.ts web/tests/e2e/pages/lobby-page.ts
```

---

### Task 6: Full verification + manual pass

**Files:** none (verification only)

- [x] **Step 1: Full gate**

Run: `make check`
Expected: fmt, clippy, cargo tests (untouched, still green), web unit/component tests, lint all pass.

- [x] **Step 2: Manual check**

Run the backend and Vite UI as two separate background tasks (NOT `make dev` — it self-kills under the runner; see memory note). Then in two browser profiles:

1. Create a challenge, land in the lobby — creator auto-joins; gauge on their team rises to 50%, one tick sounds (A4… actually the creator's own join is tick #1: 440 Hz).
2. Join from the second profile — first profile sees the gauge rise and hears the next tick up the ladder.
3. Verify: full team's button is disabled but its gauge stays visible; your own row is bold; dark theme fill contrast is acceptable (retune the 22% mix or add a dark override if `--fg` text fails on the filled region); `prefers-reduced-motion: reduce` (emulate via devtools) snaps the gauge with no transition.
4. Fill all four seats — the 4th tick lands the octave, the game-start flourish plays, everyone navigates to the table.

- [x] **Step 3: Done — hand back for review**

No version bump, no OpenAPI changes (server untouched), no coverage-baseline change (Rust untouched).
