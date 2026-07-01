# Panel border geometry — one structural width & radius

**Date:** 2026-07-01 · **Status:** implemented

## Problem

Panel-like surfaces disagree on border geometry. The button (`.btn`) — the
reference control — draws `1px` borders with `var(--radius-md)` (2px) corners,
but several surfaces deviate:

| Surface | Deviation |
| --- | --- |
| `.spades-table` (felt) | `border: 2px solid var(--felt-edge)` |
| `.spades-scoreboard` | `border-bottom-width: 2px` |
| `.spades-seat-chip` | `border-bottom-width: 2px` |
| `.quickplay-tile` | `border-top: 2px solid var(--border-strong)` |
| `.menu__row` | `border-left: 2px solid var(--border-strong)` |
| `.spades-seat-label::before` (disconnected ring) | `border: 1.5px` (fractional px renders unevenly across DPRs) |
| `.spades-clock-bar` | raw `border-radius: 2px` (untokenized) |

Radius tokens (`--radius-sm/md/lg/card`) were all collapsed to 2px by
`0540f03` ("square the radius scale"), but each rule references a different
token name, so the four knobs can silently drift apart again.

## Reference (canonical, from `.btn`)

- **Width:** `1px` → new token `--border-w`
- **Radius:** `2px` → new base token `--radius-base`

## Design

1. **`--border-w: 1px`** — every structural border (panels, chips, cards,
   inputs, fieldsets, toasts, banners, dividers) uses it. Hairline dividers
   built as `height: 1px` backgrounds adopt it too, so all hairlines move as
   one.
2. **`--radius-base: 2px`** — `--radius-sm/md/lg/card` become aliases of it.
   Existing names stay (zero-churn call sites, readable intent); drift becomes
   impossible. `--radius-pill`/`50%` remain for genuinely circular elements.
3. **Normalize deviants** listed above to `--border-w`; drop the 2px keel
   edges entirely (keels are already deprecated in favor of background-plane
   state cues); tokenize the clock-bar radius.
4. **Two-tier rule made explicit in tokens.css:** structure = `--border-w`
   borders; state/focus cues = 2px `outline`/`box-shadow` rings
   (`:focus-visible`, `.card-will-play`, playable-card inset, replay winner).
   Cues are not borders and intentionally stay 2px for visibility (a11y).
5. **Scale invariant:** border width and radius are fixed device-px. Element
   size and type scale (`--card-w` 36→64px, `clamp()` type), border geometry
   does not — a 36px and a 64px card share identical 1px/2px edges. No
   resting-state `transform: scale` exists (only transient keyframe pops), so
   no compensation is needed.

## Exception (deliberate)

`.spades-scoreboard__team`'s 2px team keel stays: it is a *state cue* carrying
team identity, with a documented pending migration to `--team-*-fill`
background cues (2026-06-12 state-cue spec). Removing it now, without the
replacement cue, would regress team identification. The seat chip's neutral
2px bottom has no cue value and is removed; the tokens.css deprecation note
now names only the scoreboard keel.

## Approaches considered

- **A (chosen): alias tokens + normalize deviants.** Minimal churn,
  drift-proof, honors the existing collapse decision.
- **B: collapse all call sites to a single `--radius` token.** ~45-line churn,
  no guarantee beyond A; loses readable size-intent at call sites.
- **C: composite `--panel-border` shorthand token.** Rejected — couples color
  to width; border colors legitimately vary (`--border`, `--card-edge`,
  `--felt-edge`, semantic mixes).

## Files

`web/src/ui/tokens.css`, `web/src/ui/design.css`, `web/src/replay/replay.css`,
`web/src/routes/profile.css`.

## Verification

`pnpm -C web lint` + `format:check` + `test`; live computed-style audit over
rendered routes via Chrome DevTools MCP (assert every non-zero border-width
= 1px and every radius ∈ {2px, 50%, 999px…}, outlines exempt); Playwright e2e.
