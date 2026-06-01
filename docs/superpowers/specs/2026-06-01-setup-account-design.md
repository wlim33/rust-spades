# Phase 3b — Setup & Account Redesign

**Date:** 2026-06-01
**Phase:** 3b (final phase of the `web/` UX-first redesign)
**Branch:** `setup-account`

## Goal

Apply the already-validated design language (Phase 0 tokens, the auth-card surface,
segmented controls) to the four remaining un-redesigned routes — **create**, **lobby**,
**profile**, **settings** — completing the redesign. No behavior changes: every SSE call,
signal, `data-testid`, and route contract is preserved. This is a visual/markup pass only.

## Context

Phases 0–2 and Auth-3a are merged. The setup/account routes are the last screens still on
the pre-redesign styling. The visual direction for this phase (segmented controls, a
team-colored lobby seat-grid, a share-link with copy feedback, card-consistent account pages)
was validated in the earlier (now-deleted) `auth-preview.html` mockup.

**Stack constraint (unchanged):** lit-html + @preact/signals + Vite, CSS-only, **no new
runtime deps**, preserve accessibility, dark + light themes via `[data-theme]` tokens only.

**happy-dom gotcha (carried from 3a):** a `${binding}` placed immediately after `</form>`
is silently dropped by happy-dom's HTML parser (renders fine in real browsers). Assert such
bindings via standalone component specs, not route specs. Watch for it in create/lobby.

## Shared building blocks

Two reusable pieces are introduced first; the four routes consume them.

### 1. `.seg` segmented control (new)

Does not exist yet — it lived only in the deleted mockup. A horizontal row of mutually
exclusive options rendered as buttons, with the selected one filled in the accent color.

**Markup contract:**

```html
<div class="seg" role="group" aria-label="<group label>">
  <button type="button" aria-pressed="true">A</button>
  <button type="button" aria-pressed="false">B</button>
  …
</div>
```

**Behavior/semantics:**

- Selection is conveyed by `aria-pressed` (`"true"` on exactly one segment per group).
- The pressed segment gets the accent fill (`--accent` background, `--accent-fg` text — see the
  token note below); the rest are transparent with `--fg-muted` text.
- Segments are equal-width (`flex: 1`) within a bordered, rounded (`--radius-md`) track on
  `--surface-raised`; internal hairline dividers between segments.
- Keyboard: each segment is a real `<button>`, so Tab/Enter/Space work natively. No custom
  roving-tabindex needed (kept simple; matches existing button-group behavior).
- On narrow viewports the track may wrap (`flex-wrap: wrap`) so the 4-seat row never overflows.

This is a **CSS + markup pattern**, not a new TS component — create builds the `<div class="seg">`
inline (it already maps over option arrays). No `button()` component call inside a segment.

### 2. `.panel` card surface (new, factored from `.auth-card`)

The `.auth-card` block already defines the canonical card surface
(`--surface-raised` + `1px --border` + `--radius-lg` + `--shadow-2` + `--space-6` padding).
Factor that surface into a shared `.panel` class so account pages read as siblings of the auth
pages, **without** the auth-only ♠ wordmark.

- Add `.panel` with the surface declarations.
- Give `.panel` and `.auth-card` the **same** surface via a single shared rule
  (`.panel, .auth-card { …surface… }`) so the two can never visually drift; `.auth-card` keeps
  its extra rules (brand wordmark, centered `h2`, `gap`) layered on top.
- No wordmark, no forced text-align on `.panel` — it's a neutral container.

## Routes

### create.ts

**Current:** `.form-page` with `<h2>Create Challenge</h2>`, a plain name `<label><input></label>`,
and three `<fieldset>`s (Seat A/B/C/D · Points 200/300/500 · Timer None/5+3/10+5/15+10), each
rendered as a group of `button()` calls with `variant: selected ? 'primary' : 'secondary'`,
then Create/Back actions and `openSse('/challenges', …)`.

**Change:** replace each fieldset's `button()` group with a `.seg` segmented control. Each
option becomes `<button type="button" aria-pressed=${selected}>`. The `<fieldset>` + `<legend>`
structure (or the existing label text) is preserved for grouping/a11y; only the inner controls
change from standalone buttons to a `.seg` row.

**Preserve:** all signals (`seat`, `points`, timer selection), `TIMER_PRESETS`, the name input,
`openSse('/challenges', …)`, Create/Back buttons, and every existing `data-testid`. Keep
`.form-page` as the outer layout.

**Out of scope:** the name field stays a `formField`-style input (no change to its mechanics);
no new validation.

### lobby.ts

**Current:** `.lobby` with `<h2>Waiting for players</h2>`, a `.seat-grid` of 4 seats
(taken `.seat-taken` / `.seat-taken.mine`, open `.seat-open`, or a join
`<button class="seat-open btn btn--primary">`), `SEAT_TEAMS = {A:'1',B:'2',C:'1',D:'2'}`,
a `.join-modal` (name input + Join/Cancel), a `.share-link` (readonly input + Copy via
`navigator.clipboard`), and a creator-only Cancel Challenge `.btn--danger`.

**Changes:**

- **Team-colored seats:** each seat carries a `--team` custom property used for a colored
  left border (or top accent) — Team 1 (`SEAT_TEAMS` A/C) = `--accent`, Team 2 (B/D) =
  `--accent-2`. Drive it by setting `style="--team: …"` (or a `data-team` + CSS rule) per seat.
- `.seat-taken.mine` keeps its existing `--accent` border + `--success-tint` background.
- **Share-link copy feedback:** the Copy button shows transient "Copied!" text for ~1.5s after a
  successful `navigator.clipboard.writeText`, then reverts. Implement with the existing signal
  pattern + a `setTimeout` — **no new dep**. If clipboard write rejects, leave the label
  unchanged (no error toast needed).
- **Join modal polish:** `.join-modal` sits on a panel-consistent surface; Join/Cancel use
  `.btn` variants. No behavior change to the join `openSse` flow.

**Preserve:** `renderLobby({root, resources, shortId, challengeId, initialStatus})` signature,
`SEAT_TEAMS`, the join SSE flow, the creator-only Cancel, and all `data-testid`s/class hooks
the e2e/component tests rely on.

### profile.ts

**Current:** `.profile-page` with loading / not-found / error / empty / list states (the states
are read eagerly into locals before building the template — a deliberate workaround for a
happy-dom + lit-html nested-ternary re-render bug; **keep this exactly**). The list is
`.profile-games` of `<li>`s showing `Game <code>{id.slice(0,8)}</code> — seat {n}`.

**Changes:**

- Wrap the `.profile-page` content in a `.panel` for surface consistency.
- Polish `.profile-games` rows: the game-id stays in a mono `<code>` chip; seat shown as a
  small label. Keep the `<ul>/<li>` structure and the `.slice(0, 8)` id.
- `.empty-state` (dashed) is unchanged.

**Preserve:** the eager-signal-read pattern and its explanatory comment, the
`request<PublicProfile>` / `request<ProfileGames>` calls, the 404→not-found branch, and the
`username` param handling.

### settings.ts

**Current:** `.form-page` with `<h2>Settings</h2>`, a "Signed in as …" line, error/saved
messages, three `formField`s (email / current password / new password), and a `.form-actions`
row with Save + Sign out. The "Saved." confirmation uses an inline
`style="color: var(--color-accent)"` — a **non-token leftover**.

**Changes:**

- Wrap the form content in a `.panel`.
- Replace the inline `var(--color-accent)` on the "Saved." line with the semantic `--accent`
  token (via a small class, e.g. `.field-success`, or an existing success token) — no raw
  inline color.
- Keep the three `formField`s, the Save/Sign-out `.form-actions`, and the `tagSave()`
  `data-testid` wiring.

**Preserve:** the auth gate (`navigateTo('/login?next=/me')` when signed out), `onSave` logic
(`updateEmail` / `updatePassword`, the "no changes" / "current password required" guards), and
all `data-testid`s.

## CSS touch list (`web/src/ui/design.css`)

- **Add** `.seg` + `.seg button` + `.seg button[aria-pressed='true']` rules.
- **Add** `.panel` surface (shared with `.auth-card`'s surface values).
- **Modify** `.seat-taken` / `.seat-open` to consume `--team` for the team-colored border;
  keep `.seat-taken.mine` as-is.
- **Modify** `.join-modal` for panel-consistent surface.
- **Modify** `.profile-page` / `.profile-games` row treatment.
- **Add** a success/confirmation text class for settings' "Saved." line.
- **Modify** `.btn--primary` to consume the new `--accent-fg` token (drop the inline `#fff` and
  the `[data-theme='dark'] .btn--primary` color override).
- **Keep** `.form-page`, `.form-actions`, `.form-field` (+`.invalid`), `.empty-state`,
  `.share-link` (extend, don't replace).

**One token addition:** `--accent-fg` (text on an accent fill). It does not exist today —
text-on-accent is currently a per-theme hardcode duplicated in `.btn--primary`
(`color: #fff` in light, `color: #06231f` under `[data-theme='dark']`). Add `--accent-fg` to
both theme blocks in `tokens.css` (light `#fff`, dark `#06231f`), refactor `.btn--primary` to
consume it (removing the inline hardcode + the `[data-theme='dark'] .btn--primary` override),
and use it for the pressed segment's text. This is a small in-area cleanup, not new scope: the
pressed segment needs an on-accent text color, and a second copy of the hardcode is exactly
when the value should become a token. No other token additions are expected.

## Verification

Each task runs the standard gate before commit:

```
pnpm -C web build && pnpm -C web test && pnpm -C web lint && pnpm -C web format:check
```

Visual checks use a throwaway standalone HTML that imports the real CSS and calls the route's
`render()` directly (bypassing the no-backend boot abort), served by `pnpm -C web dev`, viewed
in a real browser. Throwaway files are deleted before the phase merges.

## Out of scope

- No router, SSE, session, or API changes.
- No new runtime dependencies.
- No changes to the imperative card layer (Table phase owns it).
- No new routes or features; the `name` field, matchmaking, and OAuth are untouched here.

## Success criteria

- create's three pickers render as accent-filled segmented controls; selection still drives the
  same signals and `openSse('/challenges', …)` payload.
- lobby seats show team color (A/C vs B/D), `.mine` stays success-tinted, Copy shows "Copied!"
  feedback, join modal is panel-consistent.
- profile + settings sit on a `.panel`; settings' "Saved." line uses a token, not inline color.
- All existing unit/component/e2e tests pass; `data-testid`s and markup hooks preserved.
- `build && test && lint && format:check` all green.
