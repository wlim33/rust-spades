# Sans-serif font standardization

**Date:** 2026-06-12
**Status:** Approved

## Goal

Standardize the web app on a single sans-serif family. Fraunces (serif) is removed as the
display font; Hanken Grotesk becomes the only text/display family. IBM Plex Mono is kept
for its functional role (tabular clock digits, game IDs).

## Current state

Typography is centralized in two files:

- `web/src/ui/tokens.css` — three tokens:
  - `--font-display`: `'Fraunces Variable', Georgia, 'Times New Roman', serif`
  - `--font-text`: `'Hanken Grotesk Variable', system-ui, -apple-system, 'Segoe UI', sans-serif`
  - `--font-mono`: `'IBM Plex Mono', ui-monospace, 'SF Mono', monospace`
- `web/src/ui/fonts.css` — `@fontsource` imports (self-hosted, no CDN).

`--font-display` is used in `web/src/ui/design.css` for h1–h3, the auth-card brand,
quickplay tile times, and the home "searching" message. The favicon already uses
`system-ui,sans-serif`. No other font references exist in `web/src`, `web/public`,
or `web/index.html`.

## Changes

1. **Token** (`web/src/ui/tokens.css`): point `--font-display` at the same sans stack as
   `--font-text`. The token itself stays — the ~5 `var(--font-display)` usages in
   design.css are untouched, and a future display-font experiment remains a one-line swap.
2. **Font loading** (`web/src/ui/fonts.css`, `web/package.json`): remove the
   `@fontsource-variable/fraunces` import and dependency; `pnpm install` to update the
   lockfile.
3. **Display-style tuning** (`web/src/ui/design.css`): remove `font-optical-sizing: auto`
   from rules where it targeted Fraunces' opsz axis (Hanken Grotesk has none), and adjust
   heading `font-weight` (currently 560, tuned for Fraunces) so headings still read as
   display type in a sans. Exact weight (600–700) is chosen during implementation by
   eyeballing in the browser.
4. **Cleanup**: `.footer-version` uses raw `ui-monospace, monospace`; change to
   `var(--font-mono)` per the tokens-only styling rule.

## Out of scope

- Replacing IBM Plex Mono.
- Any layout, color, or sizing changes beyond the heading-weight adjustment.

## Verification

- `make check` (fmt, clippy, unit/component tests — no visual-regression suite exists).
- Manual browser pass over headings, auth brand, quickplay tiles, footer version.
