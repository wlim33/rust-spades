# Gameplay Juice + Turn-Plane Cue Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make in-game card movement feel "juicy" (spring flight + landing impact + springy hover) and replace the active-seat accent outline with a team-colored background-plane turn cue.

**Architecture:** All motion stays inside the existing hand-rolled rAF tweener (`cards/animation.ts` + `cards/orchestrator.ts`) — no new dependency — and is gated by the existing `skipAnims()` reduced-motion/hidden/no-`rAF` guard. The turn cue is pure CSS in `ui/design.css` + two new tokens in `ui/tokens.css`, continuing the state-cue migration (state in the background plane, borders structural).

**Tech Stack:** TypeScript, lit-html, vite/vitest 4 (unit = node env, component = happy-dom), plain CSS custom properties. No Rust/server/OpenAPI changes.

**Spec:** `docs/superpowers/specs/2026-06-20-gameplay-juice-and-turn-plane-design.md`

## Global Constraints

- **Reduced-motion gating:** every new piece of motion lives under `@media (prefers-reduced-motion: no-preference)` (CSS) or behind the orchestrator's existing `skipAnims()` guard (JS). Never introduce ungated motion.
- **Token discipline:** colors and sizes come from `tokens.css` custom properties (no raw hex / no raw px for design values). Animation-internal literals (durations in ms, scale factors, cubic-bezier control points) are inline, matching the existing `seat-pulse` / `card-will-play` keyframe convention.
- **Accent restraint:** `--accent` is reserved for genuinely interactive elements. The turn cue uses team washes/tints (`--team-*-glow` / `--team-*-fill`), not `--accent`.
- **Do not touch:** WS event ordering + `seq` guard, the FIFO animation chain + `generation` invalidation, the POST-inside-play-step ordering, `disableInteraction` immediacy, `.trick-container { position: relative }`, the responsive height chain, and `--card-w`/`--card-h`. `setPos` / the transform contract stays unchanged (the land-pop is pure CSS).
- **Keel tokens stay:** `--team-1` / `--team-2` remain in `tokens.css` — the scoreboard (`.spades-scoreboard__team`) still consumes them and is out of scope.
- **Gate:** web changes pass `pnpm -C web format:check`, `pnpm -C web lint` (eslint `--max-warnings=0`), `pnpm -C web test` (unit + component), and `pnpm -C web build` (tsc typecheck). `make check` is the full pre-push gate (cargo crates are unaffected → coverage baseline unchanged).
- **Commits:** stage explicit paths only (the repo sometimes has unrelated WIP staged). Conventional-commit messages.

## File Structure

| File | Responsibility | Tasks |
|------|----------------|-------|
| `web/src/cards/animation.ts` | Add `backOut` overshoot ease to the `EASE` map | 1 |
| `web/tests/unit/animation.spec.ts` | Unit-test `backOut` endpoints + overshoot | 1 |
| `web/src/cards/orchestrator.ts` | Flight uses `backOut`@280ms; toggle `card-land` on the live-play seam | 2, 3 |
| `web/src/ui/design.css` | `card-land` keyframe; hover spring; turn-cue plane + chip fill; drop keel; re-express pulse | 3, 4, 5 |
| `web/src/ui/tokens.css` | `--team-1-glow` / `--team-2-glow` on-felt washes | 5 |

---

## Task 1: Add `backOut` overshoot ease

**Files:**
- Modify: `web/src/cards/animation.ts:5-12` (the `EASE` map)
- Test: `web/tests/unit/animation.spec.ts`

**Interfaces:**
- Consumes: nothing.
- Produces: `EASE.backOut: (t: number) => number` — 0 at t=0, 1 at t=1, rises slightly above 1 mid-to-late (overshoot) before settling. The `EASE` key union gains `'backOut'`. Consumed by Task 2.

- [ ] **Step 1: Write the failing tests**

Add to `web/tests/unit/animation.spec.ts` inside the `describe('easings', …)` block (after the `quartIn` test):

```ts
  it('backOut is 0 at 0 and exactly 1 at 1', () => {
    expect(EASE.backOut(0)).toBeCloseTo(0, 10);
    expect(EASE.backOut(1)).toBe(1);
  });
  it('backOut overshoots past 1 before settling', () => {
    expect(EASE.backOut(0.6)).toBeGreaterThan(1);
  });
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `pnpm -C web exec vitest run --project=unit tests/unit/animation.spec.ts`
Expected: FAIL — `EASE.backOut is not a function` (TypeError) on the new cases.

- [ ] **Step 3: Implement the ease**

In `web/src/cards/animation.ts`, replace the `EASE` declaration (lines 5-12):

```ts
export const EASE: Record<'linear' | 'quartIn' | 'quartOut', EaseFn> = {
  linear: (t) => t,
  quartOut: (t) => {
    const u = t - 1;
    return 1 - u * u * u * u;
  },
  quartIn: (t) => t * t * t * t,
};
```

with:

```ts
// Overshoot-and-settle: the value rises past 1 in the back half, then eases
// back to exactly 1 — gives a flight a small "spring" landing. OVERSHOOT is the
// tuning knob (the classic back-ease constant is 1.70158; lower = gentler).
const OVERSHOOT = 1.2;
const BACK_C3 = OVERSHOOT + 1;

export const EASE: Record<'linear' | 'quartIn' | 'quartOut' | 'backOut', EaseFn> = {
  linear: (t) => t,
  quartOut: (t) => {
    const u = t - 1;
    return 1 - u * u * u * u;
  },
  quartIn: (t) => t * t * t * t,
  backOut: (t) => {
    const u = t - 1;
    return 1 + BACK_C3 * u * u * u + OVERSHOOT * u * u;
  },
};
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `pnpm -C web exec vitest run --project=unit tests/unit/animation.spec.ts`
Expected: PASS (all easings tests, including the two new `backOut` cases).

- [ ] **Step 5: Commit**

```bash
git add web/src/cards/animation.ts web/tests/unit/animation.spec.ts
git commit -m "feat(web): add backOut overshoot ease for card flight spring"
```

---

## Task 2: Spring the play flight

**Files:**
- Modify: `web/src/cards/orchestrator.ts:153-160` (`flyToSlot`'s `animateTo` call)
- Test (regression): `web/tests/component/orchestrator.spec.ts` (unchanged — it awaits `whenIdle()`, asserts no durations)

**Interfaces:**
- Consumes: `EASE.backOut` from Task 1 (via the `ease` option string `'backOut'`).
- Produces: no API change. Both south plays and opponent plays route through `flyToSlot`, so both gain the spring.

- [ ] **Step 1: Change the flight ease and duration**

In `web/src/cards/orchestrator.ts`, in `flyToSlot`, change the `animateTo` options (currently `duration: 250, ease: 'quartOut'`):

```ts
    const targetRect = slotEl.getBoundingClientRect();
    await animateTo(flying, {
      x: targetRect.left - srcRect.left,
      y: targetRect.top - srcRect.top,
      duration: 280,
      ease: 'backOut',
      cancelled: () => gen !== this.generation,
    });
```

- [ ] **Step 2: Run the orchestrator regression test**

Run: `pnpm -C web exec vitest run --project=component tests/component/orchestrator.spec.ts`
Expected: PASS (queue-ordering behavior unchanged; the test awaits `whenIdle()` and asserts on placement, not timing).

- [ ] **Step 3: Typecheck**

Run: `pnpm -C web exec tsc -p tsconfig.json --noEmit`
Expected: no errors (the `'backOut'` string now matches the widened `EASE` key union; an unknown ease key would be a compile error).

- [ ] **Step 4: Commit**

```bash
git add web/src/cards/orchestrator.ts
git commit -m "feat(web): play-flight overshoots and settles into the trick slot"
```

---

## Task 3: Landing impact pop

**Files:**
- Modify: `web/src/ui/design.css` (add a `card-land` keyframe near the card rules, ~after line 317)
- Modify: `web/src/cards/orchestrator.ts:161-164` (`flyToSlot` tail — toggle the class after the slot is revealed)

**Interfaces:**
- Consumes: nothing new.
- Produces: a self-removing `.card-land` CSS class applied to the trick-slot card the instant a flight settles. Fires **only** in `flyToSlot` (the live-play seam) — silent placements (`placeCardInTrick`, `setupImmediate`, `completeTrick` backfill) do not pop.

- [ ] **Step 1: Add the keyframe (CSS)**

In `web/src/ui/design.css`, immediately after the `.card-placeholder { opacity: 0; }` rule (line ~315-317), add:

```css
/* Landing impact: a one-shot scale-pop + deeper shadow as a flown card settles
   into its trick slot. Scale-only — the resting slot card sits at translate(0,0),
   so the animation composes cleanly with the inline transform, which reasserts
   on animationend (the orchestrator removes the class). */
@media (prefers-reduced-motion: no-preference) {
  .card.card-land {
    animation: card-land 160ms var(--ease);
  }
  @keyframes card-land {
    0%,
    100% {
      transform: scale(1);
    }
    40% {
      transform: scale(1.06);
      box-shadow: 4px 4px 0 rgb(var(--shadow-color) / 0.3);
    }
  }
}
```

- [ ] **Step 2: Toggle the class on the live-play seam (JS)**

In `web/src/cards/orchestrator.ts`, in `flyToSlot`, after the existing flight teardown. Change:

```ts
    flying.remove();
    this.flyingClones.delete(flying);
    slotEl.style.visibility = '';
```

to:

```ts
    flying.remove();
    this.flyingClones.delete(flying);
    slotEl.style.visibility = '';
    // Landing impact: a brief scale-pop on the now-visible slot card. The class
    // is self-removing so a card never gets stuck mid-pop; under reduced motion
    // the keyframe is empty (gated in CSS) and this is a harmless no-op.
    slotEl.classList.add('card-land');
    slotEl.addEventListener('animationend', () => slotEl.classList.remove('card-land'), {
      once: true,
    });
```

- [ ] **Step 3: Run the orchestrator regression test**

Run: `pnpm -C web exec vitest run --project=component tests/component/orchestrator.spec.ts`
Expected: PASS. (happy-dom runs the flight; the extra class toggle adds no queue step and the tests assert on card placement/counts, not `className`.)

- [ ] **Step 4: Typecheck + format-check the touched files**

Run: `pnpm -C web exec tsc -p tsconfig.json --noEmit && pnpm -C web exec prettier --check src/ui/design.css src/cards/orchestrator.ts`
Expected: no type errors; prettier reports both files formatted. If prettier complains, run `pnpm -C web exec prettier --write src/ui/design.css src/cards/orchestrator.ts` and re-check.

- [ ] **Step 5: Commit**

```bash
git add web/src/ui/design.css web/src/cards/orchestrator.ts
git commit -m "feat(web): card gives a landing pop when it settles into the trick"
```

---

## Task 4: Springy hand-card hover

**Files:**
- Modify: `web/src/ui/design.css` (add a `no-preference` override for `.hand-container .card.cm-clickable`, after the `.hand-container .card` rule ~line 390)

**Interfaces:**
- Consumes: nothing.
- Produces: no API change — playable hand cards spring on hover instead of gliding linearly.

- [ ] **Step 1: Add the hover-spring override (CSS)**

In `web/src/ui/design.css`, after the `.hand-container .card { … transition: … }` block (ends ~line 390), add:

```css
/* Playable cards spring on hover/pickup instead of gliding linearly. Only the
   transform timing changes; top / margin-left / box-shadow keep the base
   transition so the resting lift cue is unaffected. Gated for reduced motion —
   the base .hand-container .card transition above is the reduced-motion case. */
@media (prefers-reduced-motion: no-preference) {
  .hand-container .card.cm-clickable {
    transition:
      transform 220ms cubic-bezier(0.34, 1.56, 0.64, 1),
      margin-left var(--dur) var(--ease),
      top var(--dur) var(--ease),
      box-shadow var(--dur) var(--ease);
  }
}
```

- [ ] **Step 2: Format-check**

Run: `pnpm -C web exec prettier --check src/ui/design.css`
Expected: formatted. (If not, `prettier --write` then re-check.)

- [ ] **Step 3: Manual smoke (optional but recommended here)**

If `make dev` is already running, hover a legal card in your hand: it should lift with a slight overshoot/settle rather than a flat glide. (Full manual pass is Task 6.)

- [ ] **Step 4: Commit**

```bash
git add web/src/ui/design.css
git commit -m "feat(web): springy hover lift on playable hand cards"
```

---

## Task 5: Turn-cue plane (drop the outline + keel)

**Files:**
- Modify: `web/src/ui/tokens.css:31-35` (add glow tokens in the State-cues block)
- Modify: `web/src/ui/design.css` — `.spades-seat` base (line ~1135), remove keel rules (lines ~1172-1179), replace active-chip + active-outline + pulse (lines ~1192-1215)

**Interfaces:**
- Consumes: existing `--team-1-fill` / `--team-2-fill`, `--dur-cue`, seat-geometry classes (`.seat-north/south/east/west`, `.active`).
- Produces: `--team-1-glow` / `--team-2-glow` tokens; the active seat is conveyed by a team-colored background plane + chip fill (no outline, no keel).

- [ ] **Step 1: Add the glow tokens**

In `web/src/ui/tokens.css`, in the `:root` State-cues block, after the `--team-2-fill` line (line ~32), add:

```css
  /* Active-seat "lit plane": transparent team washes for the on-felt seat zone.
     Distinct from --team-*-fill (an opaque tint for chips on the cream surface);
     these sit over green felt, so they are low-alpha washes of the team hue.
     Alpha (~16%) is a tuning knob — set by eye on the felt. */
  --team-1-glow: color-mix(in oklab, var(--team-1) 16%, transparent);
  --team-2-glow: color-mix(in oklab, var(--team-2) 16%, transparent);
```

- [ ] **Step 2: Give `.spades-seat` a transitionable background plane**

In `web/src/ui/design.css`, replace the `.spades-seat` rule (lines ~1135-1140):

```css
.spades-seat {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: var(--space-2);
}
```

with:

```css
.spades-seat {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: var(--space-2);
  /* The active-turn cue lights this plane; transparent at rest, team wash when
     active, fading over --dur-cue like the lobby gauges. */
  background: transparent;
  border-radius: var(--radius-md);
  transition: background-color var(--dur-cue) var(--ease);
}
```

- [ ] **Step 3: Remove the team keel from the seat chips**

In `web/src/ui/design.css`, delete the four keel rules (lines ~1172-1179):

```css
.seat-north .spades-seat-chip,
.seat-south .spades-seat-chip {
  border-bottom-color: var(--team-1);
}
.seat-east .spades-seat-chip,
.seat-west .spades-seat-chip {
  border-bottom-color: var(--team-2);
}
```

(The chip keeps `border: 1px solid var(--border); border-bottom-width: 2px;` → a neutral 2px structural bottom border.)

- [ ] **Step 4: Replace the active chip-fill, the outline, and the pulse**

In `web/src/ui/design.css`, replace this contiguous block (lines ~1192-1215):

```css
.spades-seat.active .spades-seat-chip {
  background: var(--surface);
}
.spades-seat.active .spades-seat-label {
  color: var(--fg);
  font-weight: 600;
}
.spades-seat.active {
  outline: 2px solid var(--accent);
  outline-offset: 2px;
  border-radius: var(--radius-md);
}
@media (prefers-reduced-motion: no-preference) {
  /* Pulse the player's own chip when the turn lands; settles to the solid outline. */
  .seat-south.active {
    animation: seat-pulse 700ms var(--ease) 3;
  }
  @keyframes seat-pulse {
    50% {
      outline-offset: 5px;
      outline-color: color-mix(in oklab, var(--accent) 55%, transparent);
    }
  }
}
```

with:

```css
/* Active seat: the team plane lights up (no outline). Chip fills with the
   readable team tint; label bolds. Team is chosen by seat geometry (N/S = team 1,
   E/W = team 2), the same mapping the removed keel used. */
.seat-north.active,
.seat-south.active {
  background: var(--team-1-glow);
}
.seat-east.active,
.seat-west.active {
  background: var(--team-2-glow);
}
.seat-north.active .spades-seat-chip,
.seat-south.active .spades-seat-chip {
  background: var(--team-1-fill);
}
.seat-east.active .spades-seat-chip,
.seat-west.active .spades-seat-chip {
  background: var(--team-2-fill);
}
.spades-seat.active .spades-seat-label {
  color: var(--fg);
  font-weight: 600;
}
@media (prefers-reduced-motion: no-preference) {
  /* Your turn (south = team 1): brighten the plane and settle, in place of the
     old outline-offset pulse. */
  .seat-south.active {
    animation: turn-plane-pulse 700ms var(--ease) 2;
  }
  @keyframes turn-plane-pulse {
    50% {
      background-color: color-mix(in oklab, var(--team-1) 28%, transparent);
    }
  }
}
```

- [ ] **Step 5: Verify the keel tokens are now scoreboard-only**

Run: `grep -n "var(--team-1)\b\|var(--team-2)\b" web/src/ui/design.css`
Expected: matches **only** inside `.spades-scoreboard__team` (the scoreboard keel + its `data-team='2'` override) and the new `turn-plane-pulse` keyframe (which mixes `--team-1` into a wash). No remaining `.spades-seat-chip` keel. If a seat-chip keel match remains, delete it.

- [ ] **Step 6: Typecheck (no TS change, but build catches CSS-referenced regressions early) + format-check**

Run: `pnpm -C web exec prettier --check src/ui/tokens.css src/ui/design.css`
Expected: both formatted. (`prettier --write` then re-check if needed.)

- [ ] **Step 7: Commit**

```bash
git add web/src/ui/tokens.css web/src/ui/design.css
git commit -m "feat(web): convey active turn with a team-colored plane, retire seat keel"
```

---

## Task 6: Full verification + tuning pass

**Files:** none (verification, plus owner tuning of two knobs).

**Interfaces:** none.

- [ ] **Step 1: Run the full web gate**

Run:
```bash
pnpm -C web format:check && pnpm -C web lint && pnpm -C web test && pnpm -C web build
```
Expected: all green — prettier clean, eslint 0 warnings, unit+component suites pass, `tsc` typechecks and `vite build` succeeds.

- [ ] **Step 2: Manual visual pass — default motion, light theme**

Run `make dev` (backend :3000 + Vite :5173 as separate background tasks — `make dev` self-kills under the runner; see project memory). Start an AI game and reach PLAYING. Verify:
- Play a card: it **overshoots slightly and settles** into the center slot, then gives a small **scale-pop** ("thock") on landing.
- Opponents' plays show the same flight spring + pop.
- Hovering a **legal** card in your hand springs up (slight overshoot), not a flat glide.
- The seat whose turn it is shows a **team-colored plane** (no accent outline); your own turn additionally **pulses the plane** twice and settles. North/South planes are team-1 hued, East/West team-2.
- Seat chips have **no team keel** (neutral bottom border); team identity still reads from the scoreboard "Team A/B" + seat position.

- [ ] **Step 3: Manual visual pass — dark theme + reduced motion**

- Toggle dark theme (Settings route / theme toggle) and re-check contrast: `--fg` text on the active chip (`--team-*-fill`) and the seat label over the felt wash stay legible.
- Enable reduced motion (macOS: System Settings → Accessibility → Display → Reduce motion; or Chrome DevTools → Rendering → "Emulate prefers-reduced-motion"). Verify: cards **place instantly** (no flight/pop), the active plane is **steady** (no pulse), hover does not overshoot. Nothing should look broken.
- Check fit-mode at a short viewport (DevTools, ~700px tall): the seat background plane must not introduce overflow/scroll in the `.page--fit` chain.

- [ ] **Step 4: Tuning checkpoint (owner's call)**

Two values read very differently by eye — adjust to taste, then re-run Step 1's gate if changed:
- **`OVERSHOOT`** in `web/src/cards/animation.ts` (default `1.2`): higher = bouncier flight, lower = subtler. (Also governs how far the card visually overshoots the slot.)
- **Glow alpha** in `web/src/ui/tokens.css` `--team-*-glow` (default `16%`): higher = louder lit plane on the felt, lower = more restrained. Keep both teams symmetric.

If either changes, amend or add a commit:
```bash
git add web/src/cards/animation.ts web/src/ui/tokens.css
git commit -m "chore(web): tune flight overshoot and turn-plane wash"
```

- [ ] **Step 5: Optional e2e**

Run: `make e2e`
Expected: green. The ai-game origin-probe test runs under `reducedMotion: 'reduce'` (flight/pop/pulse skipped); the dedicated animation e2e opts back into motion and still exercises the flight path.

- [ ] **Step 6: Final pre-push gate**

Run: `make check`
Expected: fmt-check + clippy + all tests pass. Cargo crates are untouched, so the per-crate coverage baseline is unaffected.

---

## Self-Review

**Spec coverage:**
- §1 tokens (`--team-*-glow`) → Task 5 Step 1. ✓
- §2 active plane / chip fill / keel removal / pulse re-expression → Task 5 Steps 2-4. ✓
- §3 `backOut` ease + flight use → Task 1, Task 2. ✓
- §4 landing impact → Task 3. ✓
- §5 hover spring → Task 4. ✓
- §6 trick-collect deferred → not built (correctly absent). ✓
- Testing section (gate, orchestrator green, e2e reduced-motion, manual both-theme) → Task 6. ✓
- Boundaries (scoreboard keel / sound / deal-in untouched; invariants preserved) → Global Constraints + Task 5 Step 5 grep. ✓

**Placeholder scan:** No TBD/TODO; every code step shows full code; commands have expected output. The two "tuning" values are real defaults with an owner checkpoint, not placeholders. ✓

**Type consistency:** `EASE` key union widened to include `'backOut'` (Task 1) and consumed as `ease: 'backOut'` (Task 2) — matches. CSS class `card-land` defined (Task 3 Step 1) and toggled by the exact same string (Task 3 Step 2). Token names `--team-1-glow`/`--team-2-glow` defined (Task 5 Step 1) and consumed verbatim (Task 5 Step 4). ✓
