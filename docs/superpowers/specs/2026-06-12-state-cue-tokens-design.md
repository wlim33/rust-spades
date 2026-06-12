# State-Cue Tokens & Lobby Team Buttons — Design

**Date:** 2026-06-12
**Scope:** Token foundation + lobby as proving ground (other surfaces migrate later).

## Principle

State lives in the **background plane**, identity in **icons**, urgency in **sound**;
borders are structure only. Border color never carries meaning. This replaces the
current border-accent language (team keels on cards/chips/scoreboard, border-color
hover cues, dot indicators).

## 1. Tokens (`web/src/ui/tokens.css`)

New `/* state cues */` block in `:root`:

```css
/* Team colors as area fills: text must stay readable on top, so these are
   tints of the accents over the raised surface, not the accents themselves. */
--team-1-fill: color-mix(in oklab, var(--accent) 22%, var(--surface-raised));
--team-2-fill: color-mix(in oklab, var(--accent-2) 22%, var(--surface-raised));

/* Gauge motion: slower than --dur so a fill change reads as "liquid rising,"
   not a UI flicker. */
--dur-cue: 400ms;
```

- Dark theme defines its own `--team-*-fill` with the same recipe; mix percentage
  retuned by eye against dark surfaces.
- The 22% mix must keep `--fg` text readable *inside* the filled region (names sit
  directly on the gauge, no plate/text-shadow). Tune by eye if contrast fails.
- Existing `--team-1`/`--team-2` keel tokens **stay** (seat chips and scoreboard
  still consume them) but are marked deprecated in a comment naming
  `--team-*-fill` as the replacement. They are removed when those surfaces migrate.

## 2. `teamButton` component (`web/src/ui/components/team-button.ts`)

Follows the `button.ts` idiom: template function returning a `TemplateResult`,
classes in `design.css`, no shadow DOM.

```ts
type TeamButtonOpts = {
  teamNo: '1' | '2';                          // drives the fill color
  label: string;                              // "Team A"
  members: { name: string; mine: boolean }[]; // 0..capacity
  capacity: number;                           // 2 today, not hardcoded
  joinable: boolean;
  onJoin: () => void;
};
```

Markup (spans only — `<button>` content model is phrasing content, no `<ul>`):

```html
<button type="button" class="team-btn" data-team="1" data-fill="1"
        ?disabled=${!joinable} aria-label="Join Team A, 1 of 2 seats filled">
  <span class="team-btn__label">Team A</span>
  <span class="team-btn__slot">[user-fill] Alice</span>
  <span class="team-btn__slot team-btn__slot--open">[user-line] Open</span>
</button>
```

**Gauge:** `::before` layer pinned to the bottom edge, `background` from the
team's `--team-*-fill`, height `0% / 50% / 100%` keyed off `[data-fill]`
(`data-fill` = `members.length`). The CSS enumerates heights for capacity 2 —
the only capacity that exists; `capacity` in the TS API feeds the aria-label,
not the CSS. Height transitions over `--dur-cue`;
`prefers-reduced-motion` snaps with no transition. Content sits above on its
own z-layer.

**States:**

- *Joinable* — normal button affordance (cursor, hover lift, focus ring).
- *Disabled* (viewer already seated, or team full) — keeps the raised button
  styling with native `disabled`; no hover/cursor; gauge keeps animating as
  others join. `aria-label` drops the verb: "Team A, 2 of 2 seats filled".
- *Own row* — bold name (existing `.mine` convention). No accent color on
  non-interactive internals (color-restraint rule).

**Icons:** `user-line` exists in `web/src/ui/icons/`; add `user-fill` from the
same Remix Icon set (already licensed in that directory).

**Screen-reader parity:** seat changes announce via the existing
`ui/announce.ts` live region ("Alice joined Team A") — the accessible twin of
the audio tick.

## 3. Sounds (`web/src/lib/sound.ts`)

Same discipline as `chime()`: gated by the sound pref, skipped while the
AudioContext isn't running, all failures swallowed.

- **`seatTick(filledSeats: 1 | 2 | 3 | 4)`** — one short sine note per join,
  pitch rising up an A-major arpeggio: A4 → C♯5 → E5 → A5. The 4th seat lands
  on the octave (lobby "resolves" as the game becomes startable). Same
  gain/envelope family as the chime (~0.08 peak, fast attack, ~0.3 s decay).
- **`gameStart()`** — three-note rising flourish (E5 → A5 → C♯6), more
  emphatic: the "come back to the tab" moment.
- Extract the note-synthesis block from `chime()` into a shared
  `playNote(ac, freq, at)` helper used by all three.
- Frequencies/envelopes are tuning placeholders; dial in by ear.

## 4. Lobby integration (`web/src/routes/lobby.ts`)

Logic (TEAMS pairing, join-time seat resolution, SSE handling) unchanged;
presentation swaps:

- `team-grid` of `.team-card`s → two `teamButton(...)` calls. `members` from
  `teamOccupants()`; `joinable = !myPlayerId && open.length > 0`; `onJoin`
  opens the existing name modal (flow unchanged).
- `defaultTeam` primary/secondary variant logic removed — fill level says
  which team has room.
- `.team-card*` CSS deleted (not left behind).
- Keep a `lastFilled` count seeded from `initialStatus` (no tick on load).
  On `seat_update`: if filled count increased, `seatTick(newCount)` and
  announce joiners by diffing old vs new seats; decreases stay silent.
  On `game_start`: `gameStart()` before navigating.

## 5. Testing

- **Component tests:** `teamButton` renders the right `data-fill`, `disabled`,
  and `aria-label` per (members, joinable) combination — state matrix as a
  table-style test.
- **Lobby behavior:** tick/announce fire only on seat-count increases (stub
  `sound.ts` exports).
- **e2e:** update Playwright lobby selectors (`team-card` → `team-btn`); the
  join path is already covered.
- **Manual:** `make dev`, two browser profiles; verify gauge rise + pitch
  ladder, dark theme, `prefers-reduced-motion`.
- Rust crates untouched (coverage baseline unaffected); web changes go through
  `make check`.

## Out of scope (future migrations under the same principle)

- Seat chips & scoreboard keels → background-plane cues.
- Bet-button hover (border-color) → background cue.
- Create-form team picker could adopt `teamButton` at fill 0.
