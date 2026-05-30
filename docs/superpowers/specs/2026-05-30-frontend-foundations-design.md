# Frontend Redesign — Phase 0: Foundations (Design Spec)

**Date:** 2026-05-30
**Status:** Approved (visual direction validated via `web/foundations-preview.html`)
**Phase:** 0 of a sequenced, UX-first redesign. Later phases build on this layer:
**Table → Home → Auth**, each with its own spec → plan.

---

## 1. Context & goals

The rust-spades web client (`web/`, TypeScript + Vite + lit-html + `@preact/signals-core`
+ navaid) is clean and accessible but visually understated: `system-ui` everywhere,
flat surfaces, hairline borders, CSS-drawn rank-in-corner cards, only minimal phone
breakpoints, and many hardcoded colors that make theming impossible.

This phase establishes the **shared design system** every later surface depends on:

- A tokenized, semantic color system with **full light + dark themes**.
- A **fluid, mobile-first** type / space / elevation scale.
- A distinctive **font pairing** (replacing `system-ui`).
- A **license-clean icon pipeline** (Remix Icon, Apache-2.0, vendored).
- The **card-asset pipeline** (me.uk pip/Ace SVGs, CC0 + clean court treatment),
  pulled forward for early visible payoff.
- **Refreshed primitives**: button, form field, header (with theme toggle), footer,
  toast, seat chip — all token-driven.
- **Motion & elevation** primitives, gated by reduced-motion.

**Ambition:** UX-first. We keep and *elevate* the existing warm cream/ink/teal identity
and the existing structure; this is not a from-scratch art direction.

### Non-goals (deferred to later phases)
- Full layout redesigns of the table, home, or auth surfaces.
- Final court-card artwork decisions (the *treatment* is set here; exact court look is
  finalized in the Table phase against these tokens).
- Any gameplay/protocol/server changes.

---

## 2. Constraints & guardrails

- **Stack stays.** lit-html + signals + navaid + Vite. CSS-only — no CSS framework.
  **No new runtime dependencies.**
- **Dev/build-time deps are fine.** Fontsource packages (fonts) and build tooling are OK;
  icons and card art are **vendored as static assets**, not pulled at runtime.
- **Accessibility is preserved and extended:** `:focus-visible` rings, `prefers-reduced-motion`,
  `prefers-color-scheme`, keyboard play, screen-reader announcements, `pointer: coarse`
  touch targets (≥44px), tabular numerals for clocks/scores.
- **License-clean:**
  - Remix Icon — **Apache-2.0**, vendored from the
    [`cyberalien/RemixIcon`](https://github.com/cyberalien/RemixIcon) backup (the last
    Apache-2.0 snapshot before upstream relicensed). Ship `LICENSE`/`NOTICE`.
  - me.uk playing cards — **CC0 / public domain**
    ([me.uk/cards](https://www.me.uk/cards/), source
    [`revk/SVG-playing-cards`](https://github.com/revk/SVG-playing-cards)).

---

## 3. Locked decisions (validated in the preview)

| Area | Decision |
| --- | --- |
| Identity | Warm cream/ink/teal, elevated and tokenized. |
| Themes | Light (warm paper) **and** dark (warm charcoal). Cards stay **paper-white in both**. |
| Type | **Fraunces** (display) · **Hanken Grotesk** (text) · **IBM Plex Mono** (numerals). Self-hosted. |
| Shape | Tight radii **2 / 4 / 8** (card 4). Compact component paddings. |
| Cards | me.uk **pip + Ace** SVGs (CC0) + **clean minimalist court** treatment (legible at small sizes). |
| Icons | **Remix Icon** (Apache-2.0), curated subset vendored + inlined at build, no runtime dep. |
| Dark theme | Persisted toggle in header; default = stored preference, else `prefers-color-scheme`; follows OS changes when no explicit choice. |
| Responsive | Fluid mobile-first scale via `clamp()`; container-query-ready. |
| Timer | Large, mono, tabular figures; name + status dot stacked above it. |

---

## 4. Architecture

New/changed files (names indicative; follow existing `src/ui` conventions):

```
web/src/ui/
  tokens.css        # :root light + [data-theme="dark"] overrides; scales; color-scheme
  fonts.css         # @fontsource imports + @font-face wiring, font-display: swap
  icons/            # vendored Apache-2.0 Remix SVGs (curated subset) + LICENSE
  icon.ts           # icon(name): TemplateResult — inline <svg>, currentColor, aria-hidden
  design.css        # existing sheet, migrated to consume tokens (no hardcoded colors)
web/src/state/
  theme.ts          # theme signal + persistence (via lib/storage.ts) + OS listener
web/src/cards/
  assets/           # vendored me.uk CC0 pip/Ace SVGs (curated) + court treatment
  card-face.ts      # renderer: rank+suit -> face (pip art or clean court), size tokens
```

### 4.1 Color tokens & theming
- All color lives in `tokens.css` as **semantic** custom properties (see §5). `:root`
  defines light; `[data-theme="dark"]` overrides. Set `color-scheme` per theme so native
  controls/scrollbars match.
- **Migration:** every hardcoded color in `design.css` (`white`, `#effaf3`, `#fff1f1`,
  `rgba(0,0,0,…)` borders/shadows, the existing `--color-*`) is replaced by a token.
- **Felt-inheritance rule (regression guard):** any component that can render *inside*
  the felt panel (which sets a light `--felt-ink` text color) **must set its own explicit
  text color** rather than inheriting. Seat chips, toasts, menus on felt all set `color`.

### 4.2 Theme controller (`theme.ts`)
- A `theme` signal holds `'light' | 'dark'`.
- **Initial value:** `storage.get('theme')` if present, else
  `matchMedia('(prefers-color-scheme: dark)')`.
- Applies by setting `data-theme` on `<html>`.
- A header toggle flips it and **persists** the choice via the existing `lib/storage.ts`.
- When the user has made *no* explicit choice, a `matchMedia` `change` listener keeps the
  app in sync with the OS. An explicit choice wins until cleared.
- Toggle button: `aria-label`, reflects current state, icon swaps sun/moon.

### 4.3 Typography (`fonts.css`)
- Self-hosted via Fontsource (build-time, no CDN at runtime):
  `@fontsource-variable/fraunces`, `@fontsource-variable/hanken-grotesk`,
  `@fontsource/ibm-plex-mono`.
- `font-display: swap`; preload the primary text weight; subset to latin.
- Tokens: `--font-display`, `--font-text`, `--font-mono` (see §5). Fraunces uses
  `font-optical-sizing: auto`. Numerals use `font-variant-numeric: tabular-nums`.

### 4.4 Icon pipeline (`icons/` + `icon.ts`)
- **Vendor a curated subset** of the SVGs actually used (start set below) from
  `cyberalien/RemixIcon` into `src/ui/icons/`, preserving Apache-2.0 `LICENSE`/`NOTICE`.
- `icon(name, opts?)` returns an inline `<svg>` `TemplateResult`: `fill="currentColor"`,
  `aria-hidden="true"` (decorative) or labeled when standalone, sized in `em` so it scales
  with `font-size`. Vite inlines the SVG at build → tree-shaken, **zero runtime dep**.
- Document "how to add an icon" (copy the `*-line`/`*-fill` SVG, run the formatter).
- **Rejected alternative:** `unplugin-icons` + `@iconify-json/ri`. Convenient, but the
  packaged provenance can track the newer non-Apache license; vendoring the Apache backup
  is unambiguous.
- **Starter icon set:** `play-fill`, `group-(line|fill)`, `robot-2-(line|fill)`,
  `flashlight-fill`, `timer-flash-(line|fill)`, `hourglass-fill`, `trophy-line`,
  `share-forward-line`, `user-line`, `settings-3-line`, `notification-3-line`,
  `close-line`, `sun-line`, `moon-line`, `arrow-right-s-line`, `checkbox-circle-fill`,
  `error-warning-fill`, `logout-box-r-line`. (Extend per surface in later phases.)

### 4.5 Card-asset pipeline (`cards/assets/` + `card-face.ts`)
- Vendor the **pip (2–10) and Ace** faces from the me.uk CC0 set; curate to the 52 needed.
- **Courts (J/Q/K):** clean minimalist treatment (large display-serif rank + suit inside a
  subtle inner frame, suit-colored) — chosen because ornate Victorian figures read muddy at
  ~76px and on mobile, which fights the UX-first goal.
- `cardFace(rank, suit)` renders the correct face; **card faces are paper-white in both
  themes** (`--card-face`) — cards are physical objects on the table, they don't invert.
- Sizing tokens: `--card-w` with a fixed aspect ratio (≈1.4) driving height; corner index
  + (for pips) center pip layout. Replaces the current rank-in-corner `.card`.
- **Acquisition note:** me.uk cards are CGI-generated; producing the exact static SVG set is
  an implementation task. Fallback if blocked: a clean custom pip set matching the index
  treatment already shown in the preview.

### 4.6 Primitives, motion & elevation
- Restyle (token-driven, keep class names where practical to limit churn): `.btn`
  (primary/secondary/ghost/danger; hover/active/disabled/focus; icon support), form field +
  input (focus ring, invalid state), header (wordmark + nav + theme toggle + avatar), footer,
  toast (success/error/info with leading icon), seat chip (stacked name+dot / big timer).
- Elevation: warm-tinted `--shadow-1/2/3` + `--shadow-card` driven by `--shadow-color`.
- Motion: `--ease`, `--dur`; lifts/translates on hover; **all transitions disabled under
  `prefers-reduced-motion: reduce`.**

---

## 5. Token reference (canonical values)

### Color — Light (`:root`)
```
--bg:#f1e6d4  --surface:#faf4ea  --surface-raised:#fffdf9
--fg:#1b2330  --fg-muted:#5e6a70  --fg-subtle:#948b7c
--border:rgba(27,35,48,.12)  --border-strong:rgba(27,35,48,.20)
--accent:#1f8f80  --accent-hover:#1a796c  --accent-2:#e0623f  --accent-3:#c08a1e
--success:#1c8a51  --success-tint:#e6f4ea
--danger:#c33b2b   --danger-tint:#fbe9e6
--warning:#a9710a  --warning-tint:#faf0d8
--card-face:#fffdf8  --card-red:#c33b2b  --card-ink:#1b2330  --card-edge:rgba(27,35,48,.16)
--felt:#2f6f62  --felt-ink:#eaf3ee
--focus:#1f8f80  --shadow-color:27 35 48
```

### Color — Dark (`[data-theme="dark"]`)
```
--bg:#16140f  --surface:#201d17  --surface-raised:#2a261e
--fg:#f2ecdd  --fg-muted:#b0a794  --fg-subtle:#7e7567
--border:rgba(242,236,221,.13)  --border-strong:rgba(242,236,221,.24)
--accent:#3cc3b0  --accent-hover:#54d0be  --accent-2:#f0835f  --accent-3:#e0aa3e
--success:#41c079  --success-tint:#15271c
--danger:#ec6a55   --danger-tint:#2a1613
--warning:#e0aa3e  --warning-tint:#2a2210
--card-face:#f6f0e4  --card-red:#c8402f  --card-ink:#1b2330  --card-edge:rgba(0,0,0,.4)
--felt:#143029  --felt-ink:#d9e8e0
--focus:#3cc3b0  --shadow-color:0 0 0
```

### Scales (theme-independent)
```
Fonts   --font-display:'Fraunces',Georgia,serif
        --font-text:'Hanken Grotesk',system-ui,sans-serif
        --font-mono:'IBM Plex Mono',ui-monospace,monospace
Type    --text-xs:.78rem  --text-sm:.875rem
        --text-base:clamp(1rem,0.96rem+0.18vw,1.0625rem)
        --text-lg:clamp(1.125rem,1.06rem+0.3vw,1.25rem)
        --text-xl:clamp(1.4rem,1.24rem+0.7vw,1.75rem)
        --text-2xl:clamp(1.85rem,1.5rem+1.55vw,2.6rem)
        --text-3xl:clamp(2.4rem,1.8rem+2.7vw,3.6rem)
Space   --space-1:.25 -2:.5 -3:.75 -4:1 -5:1.25 -6:1.5 -8:2 -10:3 -12:4 (rem; 7/9/11 unused)
        --gutter:clamp(1rem,4vw,2.5rem)
Radius  --radius-sm:2px --radius-md:4px --radius-lg:8px --radius-card:4px --radius-pill:999px
Shadow  --shadow-1/2/3 + --shadow-card  (rgb(var(--shadow-color)/α), warm-tinted)
Motion  --ease:cubic-bezier(.2,.7,.3,1)  --dur:180ms
Layout  --content-max:60rem
```

---

## 6. Responsive strategy
- **Mobile-first**: base styles target narrow; enhance upward.
- Fluid type + `--gutter` via `clamp()`; `--content-max` caps line length on desktop.
- Layout breakpoints are retained for structural shifts (e.g. the table grid) and refined
  per surface in later phases; foundations make those phases container-query-ready.
- `@media (pointer: coarse)` keeps ≥44px touch targets.

---

## 7. Accessibility
- `color-scheme` set per theme; toggle is keyboard-operable, labeled, reflects state.
- **Contrast:** token pairs chosen for WCAG AA on text. Verify during build:
  `fg`/`fg-muted` on `bg`/`surface`/`surface-raised`, `accent` text/`#fff` on `accent`,
  semantic text on tints, and seat/clock text on `--surface-raised` — in **both** themes.
- `:focus-visible` uses `--focus`; `prefers-reduced-motion` disables transitions; numerals
  tabular; decorative icons `aria-hidden`, standalone icons labeled; SR announcements kept.

---

## 8. Migration & testing
- Introduce `tokens.css` + `fonts.css` + `theme.ts`; wire the toggle into the header.
- Tokenize `design.css` (remove hardcoded colors), keeping class names to limit churn.
- Build the icon helper + vendored subset; build card sizing tokens + `card-face.ts`.
- **Tests/gates:** existing `vitest` (unit + component) and Playwright e2e stay green; the
  repo's lint/format/typecheck/audit gates pass. Add: a theme-persistence test; consider a
  guard that flags hardcoded hex in component CSS.
- **Visual reference:** `web/foundations-preview.html` (throwaway) is the source of truth
  for the look; verify light/dark parity, contrast, and mobile widths against it.

---

## 9. Risks & open questions
- **Font perf:** self-hosted variable fonts must be subset + preloaded to avoid layout shift.
- **me.uk card acquisition:** CGI-generated; curating the static SVG set is real work —
  fallback to a clean custom pip set if blocked.
- **Bundle size:** vendor only the icons/cards actually used; revisit per phase.
- **Court look:** "clean minimalist" finalized in the Table phase against these tokens.

## 10. Phase 0 deliverables
`tokens.css` · `fonts.css` · `theme.ts` + header toggle · `icon.ts` + vendored icon subset
(+LICENSE) · card sizing tokens + `card-face.ts` + vendored pip/Ace set · refreshed
primitives · tokenized `design.css` · tests green.
