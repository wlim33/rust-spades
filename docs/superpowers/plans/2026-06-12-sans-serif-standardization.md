# Sans-Serif Font Standardization Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make Hanken Grotesk the single text/display family (removing the Fraunces serif), keeping IBM Plex Mono for clocks/IDs.

**Architecture:** Typography is centralized in `web/src/ui/tokens.css` (tokens) and `web/src/ui/fonts.css` (@fontsource imports). We repoint the `--font-display` token at the existing sans stack, then delete the Fraunces package, then tune the few display rules in `design.css` that were calibrated for the serif. Spec: `docs/superpowers/specs/2026-06-12-sans-serif-standardization-design.md`.

**Tech Stack:** Plain CSS custom properties, @fontsource self-hosted fonts, pnpm, Vite.

**Testing note:** This is a CSS-only change with no testable logic — TDD does not apply. Verification is build + existing test suite + a manual browser pass (no visual-regression suite exists in this repo).

**Conventions that apply (from CLAUDE.md / project memory):**
- Styling uses only tokens from `web/src/ui/tokens.css` — no raw font stacks in rules.
- Commit with explicit pathspecs (`git commit -- <files>`); the repo often has unrelated WIP staged.
- `cargo` is not on the default PATH: run `export PATH="$HOME/.cargo/bin:$PATH"` before any `make` target that invokes cargo.

---

### Task 1: Repoint the display token at the sans stack

**Files:**
- Modify: `web/src/ui/tokens.css:46`

- [ ] **Step 1: Edit the token**

Change line 46 from:

```css
  --font-display: 'Fraunces Variable', Georgia, 'Times New Roman', serif;
```

to:

```css
  --font-display: var(--font-text);
```

(Custom properties may reference siblings declared in the same `:root` block; declaration order does not matter. Keeping the token — rather than replacing its usages — preserves the display/text semantic distinction and makes a future display-font swap a one-line change.)

- [ ] **Step 2: Verify the app still builds**

Run: `pnpm -C web build`
Expected: exits 0, no "Fraunces" warnings (the font files are still installed at this point).

- [ ] **Step 3: Commit**

```bash
git add web/src/ui/tokens.css
git commit -m "style(web): point --font-display at the sans text stack" -- web/src/ui/tokens.css
```

---

### Task 2: Remove the Fraunces font package

**Files:**
- Modify: `web/src/ui/fonts.css:1`
- Modify: `web/package.json:23`
- Modify: `web/pnpm-lock.yaml` (regenerated, not hand-edited)

- [ ] **Step 1: Delete the Fraunces import**

`web/src/ui/fonts.css` currently reads:

```css
@import '@fontsource-variable/fraunces';
@import '@fontsource-variable/hanken-grotesk';
@import '@fontsource/ibm-plex-mono/400.css';
@import '@fontsource/ibm-plex-mono/500.css';
```

Delete line 1 so it reads:

```css
@import '@fontsource-variable/hanken-grotesk';
@import '@fontsource/ibm-plex-mono/400.css';
@import '@fontsource/ibm-plex-mono/500.css';
```

- [ ] **Step 2: Remove the dependency**

In `web/package.json`, delete this line from `dependencies`:

```json
    "@fontsource-variable/fraunces": "^5.2.9",
```

- [ ] **Step 3: Regenerate the lockfile**

Run: `pnpm -C web install`
Expected: lockfile updated, `@fontsource-variable/fraunces` removed.

- [ ] **Step 4: Verify the build no longer ships Fraunces**

Run: `pnpm -C web build && ls web/dist/assets | grep -ci fraunces`
Expected: build exits 0; the grep prints `0` (grep itself exits 1 on zero matches — that is the expected outcome, not a failure).

- [ ] **Step 5: Commit**

```bash
git add web/src/ui/fonts.css web/package.json web/pnpm-lock.yaml
git commit -m "style(web): drop Fraunces font package" -- web/src/ui/fonts.css web/package.json web/pnpm-lock.yaml
```

---

### Task 3: Tune display rules for the sans + footer mono cleanup

**Files:**
- Modify: `web/src/ui/design.css:783-791` (.quickplay-tile__time)
- Modify: `web/src/ui/design.css:1236-1244` (h1, h2, h3)
- Modify: `web/src/ui/design.css:971-974` (.footer-version)

(Line numbers are pre-edit; later blocks shift up by one after the first edit. Match on content, not line number.)

- [ ] **Step 1: Remove the opsz hint from .quickplay-tile__time**

Hanken Grotesk has no optical-size axis (Fraunces did), so `font-optical-sizing` is dead weight. Change:

```css
.quickplay-tile__time {
  font-family: var(--font-display);
  font-optical-sizing: auto;
  font-weight: 600;
  font-size: 1.55rem;
  line-height: 1;
  letter-spacing: -0.01em;
  font-variant-numeric: tabular-nums;
}
```

to:

```css
.quickplay-tile__time {
  font-family: var(--font-display);
  font-weight: 600;
  font-size: 1.55rem;
  line-height: 1;
  letter-spacing: -0.01em;
  font-variant-numeric: tabular-nums;
}
```

- [ ] **Step 2: Retune the heading rule**

Weight 560 was calibrated for Fraunces' serif contrast; user selected 600 for the sans (matches existing button/brand weight). Change:

```css
h1,
h2,
h3 {
  font-family: var(--font-display);
  font-optical-sizing: auto;
  font-weight: 560;
  line-height: 1.1;
  letter-spacing: -0.01em;
}
```

to:

```css
h1,
h2,
h3 {
  font-family: var(--font-display);
  font-weight: 600;
  line-height: 1.1;
  letter-spacing: -0.01em;
}
```

- [ ] **Step 3: Tokenize the footer-version font**

`.footer-version` violates the tokens-only rule with a raw stack. Change:

```css
.footer-version {
  font-family: ui-monospace, monospace;
  opacity: 0.8;
}
```

to:

```css
.footer-version {
  font-family: var(--font-mono);
  opacity: 0.8;
}
```

- [ ] **Step 4: Verify no display-font or opsz stragglers remain**

Run: `grep -n "font-optical-sizing\|Fraunces\|ui-monospace" web/src/ui/*.css`
Expected: exactly one match — `--font-mono`'s own fallback chain in `tokens.css:48` (`'IBM Plex Mono', ui-monospace, 'SF Mono', monospace`), which is the token definition itself and is allowed.

- [ ] **Step 5: Commit**

```bash
git add web/src/ui/design.css
git commit -m "style(web): retune display rules for sans; tokenize footer mono" -- web/src/ui/design.css
```

---

### Task 4: Full verification

**Files:** none (verification only)

- [ ] **Step 1: Run the full pre-push gate**

```bash
export PATH="$HOME/.cargo/bin:$PATH"
make check
```

Expected: fmt-check, clippy, cargo tests, and web tests all pass. (The Rust side is untouched; this guards against accidental collateral.)

- [ ] **Step 2: Manual browser pass**

Start the app (backend + Vite as two separate background tasks — `make dev` self-kills under the agent runner; use `cargo run -p spades-server -- --insecure-cookies --cors-allow-origin http://localhost:5173` and `pnpm -C web dev`) and check at `http://localhost:5173`:

- Home: h1/h2 headings render in Hanken Grotesk at weight 600 (no serif anywhere).
- Quickplay tiles: the time numerals are sans, tabular, weight 600.
- Auth page (`/login`): the `auth-card__brand` lockup is sans.
- Footer: version string renders in IBM Plex Mono.
- Network tab: no `fraunces*.woff2` requests.

- [ ] **Step 3: Done — report**

No commit; summarize verification results. Deploy happens automatically on push to `master` (do not push without the user's go-ahead).
