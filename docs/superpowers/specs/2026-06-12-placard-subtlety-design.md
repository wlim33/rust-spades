# Placard Subtlety + Icon Foundation — Design

Date: 2026-06-12
Status: approved

## Problem

The felt shows ~10 colored decorations at rest, violating the project's own
"accent = interactive only" rule:

- 4 seat chips each carry a full-strength `--accent` teal connection dot
  (`.spades-seat-label::before`) and a 2px team-colored bottom keel.
- The scoreboard placard carries 2 more team keels.
- (Legitimate, kept: the active-turn accent outline + pulse — it signals state.)

Separately, the project wants icons for points, bags, and future game glyphs.
Today the scoreboard spells out the word "Bags"; the icon system
(`web/src/ui/icon.ts`, build-time-inlined SVGs in `web/src/ui/icons/`) vendors
Remix Icon (Apache-2.0) only.

## Decisions (brainstorm outcomes)

1. **Team color: mute everywhere.** Keep the keel on all 6 spots (4 chips +
   2 scoreboard blocks) but blend it much closer to neutral. Team identity
   stays learnable, just quieter.
2. **Connection dots: neutral, always shown.** Gray dot when connected
   (presence feel preserved); existing hollow ring when disconnected.
3. **Icon strategy: Remix + Lucide.** Keep Remix for UI chrome; add Lucide
   (ISC, 24px grid, 2px stroke — visually compatible with Remix line weight)
   for game glyphs Remix lacks (notably a literal `spade` suit).
4. **Icon application: scoreboard only.** The word "Bags" becomes a bag glyph.
   Seat-chip text ("Bid 3 · Took 2") stays words — glyphs ×4 seats would add
   visual chatter, against the subtlety goal. Scores stay bare numbers.

## Changes

### 1. Tokens — `web/src/ui/tokens.css`

The de-accent is one knob: drop the team mix from 60% accent to ~35%.

```css
--team-1: color-mix(in oklab, var(--accent) 35%, var(--fg-muted));
--team-2: color-mix(in oklab, var(--accent-2) 35%, var(--fg-muted));
```

Both themes inherit automatically (dark mode only redefines the accent
inputs). The lobby seat grid (`.seat-grid`, the third consumer of these
tokens) quiets down with it — intended. The percentage is a taste knob;
35% is the starting value, tuned visually in-browser (range 30–40%).

### 2. Seat chips — `web/src/ui/design.css`

One property: `.spades-seat-label::before` changes
`background: var(--accent)` → `background: var(--fg-subtle)`.
Disconnected style (hollow `--fg-subtle` ring) already matches.
Active-turn outline/pulse untouched.

### 3. Scoreboard — `web/src/ui/components/scores.ts`

`"${score} · Bags ${bags}"` → `${score} · <bag glyph> ${bags}` using the
existing `icon()` helper with `label: 'Bags'` so the accessible name is
unchanged (`role="img" aria-label="Bags"`). Team labels, "(You)", and
`centerText` unchanged.

### 4. Icon infrastructure — `web/src/ui/icons/`

- Vendor 3 Lucide SVGs: `shopping-bag.svg` (bags), `coins.svg` (points,
  future use), `spade.svg` (suit glyph, future branding/empty states).
- Add `LICENSE-lucide` (ISC text) beside the existing Apache-2.0 LICENSE.
- **Required CSS fix** in `design.css`: Lucide SVGs are stroke-based with
  `fill="none"`; the current `.icon svg { fill: currentColor }` rule would
  override that attribute and render them as solid blobs. Scope it:

```css
.icon svg:not([fill='none']) {
  fill: currentColor;
}
```

Lucide files are vendored as-is (no stroke-width tweaks needed at 1em sizes).

### 5. Testing

- New component test for `scores()`: both team scores render, the bag glyph
  is present with `aria-label="Bags"`, and "(You)" lands on the caller's team.
  (No existing test covers the scoreboard; no test or e2e selector depends on
  the literal "Bags" text — verified by grep.)
- Manual visual pass: light + dark themes, muted keels on felt + lobby.

## Out of scope

Bid bar, trick area, lobby markup (only its inherited token shift), seat-chip
text changes, replacing any existing Remix icon.
