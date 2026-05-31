# Frontend Redesign — Phase 1: In-Game Table (Design Spec)

**Date:** 2026-05-30
**Status:** Approved (visual direction validated via `web/table-preview.html` with real me.uk cards)
**Phase:** 1 of the redesign. Builds directly on **Phase 0 (Foundations)** — tokens, light/dark theming, fonts, icons — which is merged to master. Remaining after this: Home, Auth.

---

## 1. Context & goals

The in-game table is the product's core surface and the most visually dated: cards are CSS-drawn rank-in-corner rectangles, there is no table surface (cards float on the page background), per-player **clocks arrive in the data but are never rendered**, the bid UI is a plain 0–13 button grid, and layout is desktop-centric. This phase makes the table feel like a real card table.

**Architecture reality (must respect):** the table is a **hybrid render**. `routes/game-view.ts` renders chrome (scores, seats, bid buttons) with lit-html inside one `effect`; a **separate `effect`** drives the **imperative `CardOrchestrator`** (`cards/orchestrator.ts` + `card-el.ts`), which owns the card DOM and all animations (deal, fly-to-center, trick-collect, drag, keyboard). **Card *faces* are swappable without touching the animation/positioning code** — the orchestrator manipulates the `.card` element, not its contents.

### Goals
- Real **me.uk Goodall card faces** (CC0) for all 52 cards; a themed card back.
- A **felt table surface**, in light and dark (cards stay paper in both).
- **Mobile-first responsive** table layout.
- **Display the clocks**: per-seat clock with an active-player ticking countdown, progress bar, and low-time warning.
- A **token-styled bid bar** (Nil + 1–13) and a **cleaner hand fan** (index-corner overlap).
- Preserve all existing orchestrator animations + a11y (keyboard play, SR announcements, focus, reduced-motion).

### Non-goals (later phases / out of scope)
- Home, Auth, lobby surfaces.
- Server/protocol/gameplay-rule changes.
- New game features (chat, emotes, etc.).

---

## 2. Constraints & guardrails
- Build on Phase 0: consume existing tokens; **stay** on lit-html + `@preact/signals-core` + Vite, CSS-only, **no new runtime JS deps**.
- Preserve a11y: `:focus-visible`, `prefers-reduced-motion`, keyboard play (`cards/keyboard.ts`), SR announcements (`ui/announce.ts`), tabular numerals on clocks, ≥44px touch targets.
- **License:** me.uk cards are **CC0** (no attribution required). `svgo` is a dev/build-time tool only.
- **No runtime/CI fetch of me.uk:** the deck is downloaded once and **vendored** into the repo.

---

## 3. Locked decisions (validated in the preview)

| Area | Decision |
| --- | --- |
| Card faces | Real **me.uk CC0** deck (ornate Goodall courts + pip cards), rendered as `<img>` into the existing card element. |
| Card back | **Foundations teal-hatch** (`.card-back`), themed — NOT me.uk's plain `1B` back. |
| Acquisition | `curl -F "zip=Download zip file of SVG for web use" https://www.me.uk/cards/makeadeck.cgi` → 52 `<rank><suit>.svg` (+ backs/jokers, unused). `T`=10; suits `C/D/H/S`. Vendored once; `svgo`-optimized. |
| Felt | Radial-green surface (`--felt`/`--felt-edge`/`--felt-ink`), cards stay paper in both themes. |
| Clocks | Big mono seat clock; active player ticks down client-side with a progress bar + low-time color. |
| Bid bar | Nil + 1–13, token-styled grid. |
| Responsive | Mobile-first; table flips to portrait on narrow widths; cards + hand fan scale down. |

---

## 4. Architecture

### 4.1 Card asset pipeline
- **Acquire** (one-time, documented; not run in CI): the `curl -F "zip=…"` call above yields a 251 KB ZIP of per-card SVGs.
- **Optimize**: run `svgo` over the 52 needed faces (courts are ~50 KB colorful SVGs; svgo meaningfully shrinks them).
- **Vendor**: place optimized faces in **`web/public/cards/<RANK><SUIT>.svg`** (served as static URLs, *not* bundled into JS — keeps the JS bundle lean and lets the browser fetch faces on demand). Include a short `web/public/cards/SOURCE.md` noting CC0 + the generator command/options used (so the set is regenerable).
- **Naming map**: game `Card { rank, suit }` → filename. Rank: `Two..Nine`→`2..9`, `Ten`→`T`, `Jack/Queen/King/Ace`→`J/Q/K/A`. Suit: `Spade/Heart/Diamond/Club`→`S/H/D/C`. A pure helper `cardFaceUrl(card): string` in `card-el.ts` (or a small `cards/face.ts`).

### 4.2 Card face rendering (`cards/card-el.ts`)
- `createFront(card)` / `setFront(el, card)`: instead of `textContent` + `.card-front::before`, render an `<img class="card-face" loading="lazy" src=${cardFaceUrl(card)} alt="">` as the child of the `.card` element. The me.uk SVGs carry their own baked colors (red/black pips, colorful courts) and indices, so **no CSS recoloring** — `--card-red`/`--card-ink` no longer apply to faces (they remain for any fallback).
- **Preserve the element contract** the rest of the system depends on: keep the `.card`/`.card-front` classes, the `role="button"`, the `aria-label="${rank} of ${suit}s"` (keyboard + SR), and the `._cm` position field. The orchestrator, `drag.ts`, `animation.ts`, `keyboard.ts` are **unchanged**.
- `createBack()` stays as-is (CSS `.card-back` teal-hatch; no img).
- The `.card` element keeps `border-radius` + `--card-edge` border + `--shadow-card` (the physical-card framing); `.card-face` img is `position:absolute; inset:0; width/height:100%; object-fit:fill` and the card clips it via `overflow:hidden`. Remove the now-unused `.card-front::before` rank/suit rule.
- **Image-load timing:** faces are tiny/local; a face swapping mid-animation must not break layout (img has fixed box from `.card`). Acceptable; note for review.

### 4.3 Felt surface (`design.css` + `game-table.ts`)
- Add a felt to the table container. In `game-table.ts`, the `.spades-table` (or a new `.spades-felt` wrapper) gets the felt background. Tokens: reuse `--felt`/`--felt-ink`; **add `--felt-edge`** to `tokens.css` (light `#25564c`, dark `#0d211c`) for the radial edge.
- Recipe (from preview): `radial-gradient(120% 90% at 50% 38%, color-mix(in oklab, var(--felt) 78%, #fff 8%), var(--felt) 70%, var(--felt-edge))`, with `inset` highlight + shadow. Rounded corners. `color: var(--felt-ink)` (and per the Phase 0 felt-inheritance rule, chips/text on the felt set their own colors).
- Cards stay paper (`--card-face`) in both themes.

### 4.4 Clocks (`state/clocks.ts` + `game-table.ts` + `game-view.ts`)
- The store already provides `timerConfig`, `playerClocksMs` (per-seat snapshot), `activePlayerClockMs`, and `currentPlayerId`; `game-view.ts` currently passes `clockText: null`.
- New module **`state/clocks.ts`** (lives under `state/`, not `cards/`): on each `applyState`, capture `(activePlayerClockMs, capturedAt = performance.now())`; a timer (250 ms) recomputes the active seat's remaining = `snapshot − (now − capturedAt)`, clamped ≥ 0; non-active seats show their static `playerClocksMs`. Exposes a `clockText(seatIdx)` + `isLow(seatIdx)` (low threshold e.g. ≤ 15 s) via a signal the template reads, so the seat chips re-render each tick. Pure formatter `fmtClock(ms): string` (→ `m:ss`) is unit-tested. No timer when `timerConfig` is null (untimed games show no clock).
- `game-view.ts` passes `clockText` + a `low` flag to each `SeatProps`; `game-table.ts` renders the clock (already styled `.spades-clock`) and a thin progress bar (active seat), with `.low` recoloring to `--accent-2`.
- Clean up the timer on route teardown (push to `resources.cleanups`).

### 4.5 Bid bar (`game-view.ts` + `design.css`)
- Today: `Array.from({length:14})` → buttons 0–13 in `.spades-bets`. Re-style `.spades-bets`/`.spades-bet` to the preview's token-driven grid; render **0 as “Nil”** with the `--accent-2` accent, 1–13 as numerals (tabular). Responsive columns (7 → 5 → 4 at breakpoints, already present).

### 4.6 Hand fan + responsive (`design.css`, `hand-manager.ts` if needed)
- The south hand should overlap so a full 13-card hand fits and shows **clean index corners** (not overlapping centers). Prefer a CSS solution on `.hand-container .card` (negative margin sized so ~the index strip shows; first card full). Verify against `hand-manager.ts` layout; adjust only if the manager hard-positions in a way CSS can't override.
- Responsive: keep/extend the existing `@media (max-width:600px)` table-grid reflow; the felt uses `aspect-ratio` that flips portrait on narrow; card sizes step down (the existing `.card` breakpoints) — confirm the imperative trick offsets (`TRICK_OFFSETS` in orchestrator) still read well at small sizes, retuning constants if needed.

### 4.7 Animations
- `orchestrator.ts` deal/play/collect animations are **retained**. Re-tune `TRICK_OFFSETS` / collect geometry only if the felt center sizing requires it. All animation gated by `prefers-reduced-motion` (verify the orchestrator honors it; if not, add a guard).

---

## 5. Token additions
```
tokens.css :root        --felt-edge: #25564c;
tokens.css [data-theme=dark]  --felt-edge: #0d211c;
```
(`--felt`/`--felt-ink` already exist from Phase 0.)

---

## 6. Testing & verification
- **Unit:** `fmtClock(ms)` formatting + low-threshold logic; `cardFaceUrl(card)` mapping (e.g. `{Ten,Heart}`→`/cards/TH.svg`, `{Ace,Spade}`→`/cards/AS.svg`).
- **Component:** `card-el.spec.ts` **updated** — `createFront` now yields an `<img.card-face>` with the right `src` + the preserved `aria-label`/classes (the old `textContent` assertions are replaced, since faces are images). Bid bar renders Nil + 1–13. Seat chip renders a clock when `clockText` is provided and adds `.low` past the threshold.
- **Gate:** `pnpm -C web build && pnpm -C web test && pnpm -C web lint && pnpm -C web format:check` green.
- **e2e:** check the Playwright suite for any card-selection-by-text-content (cards no longer have text); update page objects to select by `aria-label` if needed. Run with the Rust backend in CI.
- **Visual:** `web/table-preview.html` is the reference; verify light/dark, real cards at game size, ticking clock, mobile portrait.

---

## 7. Risks & open questions
- **Bundle/asset size:** ornate courts are large; mitigate with svgo + serving from `public/` (out of the JS bundle) + `loading="lazy"`. Re-measure after svgo.
- **me.uk regenerability:** record the exact generator options in `public/cards/SOURCE.md`; the live CGI is the only source, so the vendored copy is authoritative.
- **Card `<img>` swap vs animation:** ensure face changes don't reflow mid-flight (fixed `.card` box should prevent it) — verify.
- **Clock accuracy:** client tick is display-only; server state resyncs each `applyState`. Drift between ticks is cosmetic.
- **Scope size:** sizable. The plan may sequence as (a) card assets + face rendering, (b) felt + responsive, (c) clocks, (d) bid bar + hand fan — independently shippable within the phase.

## 8. Deliverables
Vendored + optimized `public/cards/*.svg` (+ `SOURCE.md`) · `cardFaceUrl` + `card-el.ts` img rendering · `--felt-edge` token + felt surface · `state/clocks.ts` ticking clocks wired into seats · token-styled bid bar (Nil + 1–13) · index-corner hand fan + responsive table · updated/added tests green.
