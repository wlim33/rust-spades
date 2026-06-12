# Placard Subtlety + Icon Foundation Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** De-accent the felt placards (muted team keels, neutral connection dots) and add Lucide game glyphs to the icon system, replacing the scoreboard's "Bags" word with a bag glyph.

**Architecture:** Token-level mute (`--team-1/2` color-mix percentage) does the whole keel pass; one CSS property neutralizes the dots. Lucide SVGs (stroke-based) join the vendored Remix set (fill-based) behind the existing `icon()` helper — requires scoping the global `fill: currentColor` rule so stroke icons survive. Spec: `docs/superpowers/specs/2026-06-12-placard-subtlety-design.md`.

**Tech Stack:** lit-html templates, vitest + happy-dom component tests, Vite `import.meta.glob` SVG inlining, CSS custom properties with `color-mix(in oklab, …)`.

---

### Task 1: Vendor Lucide icons + scoped fill fix

Lucide files are stroke-based (`fill="none" stroke="currentColor"`), unlike the fill-based Remix set. The existing `.icon svg { fill: currentColor }` rule (a CSS declaration, which beats the SVG's `fill="none"` presentation attribute) would render them as solid blobs — the rule must be scoped first.

**Files:**
- Modify: `web/tests/component/icon.spec.ts` (append one test)
- Create: `web/src/ui/icons/spade.svg`
- Create: `web/src/ui/icons/shopping-bag.svg`
- Create: `web/src/ui/icons/coins.svg`
- Create: `web/src/ui/icons/LICENSE-lucide`
- Modify: `web/src/ui/design.css:1184-1192` (the `.icon` block)

- [ ] **Step 1: Write the failing test**

Append inside the existing `describe('icon', …)` block in `web/tests/component/icon.spec.ts`:

```ts
  it('renders vendored Lucide icons without clobbering their stroke style', () => {
    render(icon('spade'), document.getElementById('root')!);
    const svg = document.querySelector('.icon svg');
    expect(svg).not.toBeNull();
    expect(svg!.getAttribute('fill')).toBe('none');
    expect(svg!.getAttribute('stroke')).toBe('currentColor');
  });
```

- [ ] **Step 2: Run test to verify it fails**

Run: `pnpm -C web test tests/component/icon.spec.ts`
Expected: FAIL — `expected null not to be null` (no `spade.svg` exists, `icon()` returns `nothing`). The 4 pre-existing tests still pass.

- [ ] **Step 3: Create the three SVG files**

Content fetched from `lucide-static@1.18.0` (pin: `https://unpkg.com/lucide-static@1.18.0/icons/<name>.svg`), vendored as-is including the license comment.

`web/src/ui/icons/spade.svg`:

```svg
<!-- @license lucide-static v1.18.0 - ISC -->
<svg
  class="lucide lucide-spade"
  xmlns="http://www.w3.org/2000/svg"
  width="24"
  height="24"
  viewBox="0 0 24 24"
  fill="none"
  stroke="currentColor"
  stroke-width="2"
  stroke-linecap="round"
  stroke-linejoin="round"
>
  <path d="M12 18v4" />
  <path d="M2 14.499a5.5 5.5 0 0 0 9.591 3.675.6.6 0 0 1 .818.001A5.5 5.5 0 0 0 22 14.5c0-2.29-1.5-4-3-5.5l-5.492-5.312a2 2 0 0 0-3-.02L5 8.999c-1.5 1.5-3 3.2-3 5.5" />
</svg>
```

`web/src/ui/icons/shopping-bag.svg`:

```svg
<!-- @license lucide-static v1.18.0 - ISC -->
<svg
  class="lucide lucide-shopping-bag"
  xmlns="http://www.w3.org/2000/svg"
  width="24"
  height="24"
  viewBox="0 0 24 24"
  fill="none"
  stroke="currentColor"
  stroke-width="2"
  stroke-linecap="round"
  stroke-linejoin="round"
>
  <path d="M16 10a4 4 0 0 1-8 0" />
  <path d="M3.103 6.034h17.794" />
  <path d="M3.4 5.467a2 2 0 0 0-.4 1.2V20a2 2 0 0 0 2 2h14a2 2 0 0 0 2-2V6.667a2 2 0 0 0-.4-1.2l-2-2.667A2 2 0 0 0 17 2H7a2 2 0 0 0-1.6.8z" />
</svg>
```

`web/src/ui/icons/coins.svg`:

```svg
<!-- @license lucide-static v1.18.0 - ISC -->
<svg
  class="lucide lucide-coins"
  xmlns="http://www.w3.org/2000/svg"
  width="24"
  height="24"
  viewBox="0 0 24 24"
  fill="none"
  stroke="currentColor"
  stroke-width="2"
  stroke-linecap="round"
  stroke-linejoin="round"
>
  <path d="M13.744 17.736a6 6 0 1 1-7.48-7.48" />
  <path d="M15 6h1v4" />
  <path d="m6.134 14.768.866-.5 2 3.464" />
  <circle cx="16" cy="8" r="6" />
</svg>
```

- [ ] **Step 4: Vendor the Lucide license file**

The full file is required (not just the ISC paragraph): `shopping-bag` is Feather-derived and covered by the MIT section inside Lucide's LICENSE.

Run:

```bash
curl -sL https://unpkg.com/lucide-static@1.18.0/LICENSE -o web/src/ui/icons/LICENSE-lucide
head -1 web/src/ui/icons/LICENSE-lucide
```

Expected output: `ISC License`

- [ ] **Step 5: Scope the fill rule**

In `web/src/ui/design.css`, replace:

```css
.icon svg {
  width: 1em;
  height: 1em;
  fill: currentColor;
}
```

with:

```css
.icon svg {
  width: 1em;
  height: 1em;
}
.icon svg:not([fill='none']) {
  fill: currentColor;
}
```

- [ ] **Step 6: Run test to verify it passes**

Run: `pnpm -C web test tests/component/icon.spec.ts`
Expected: PASS (5 tests).

- [ ] **Step 7: Format, lint, commit**

```bash
pnpm -C web format && pnpm -C web lint
git add web/src/ui/icons/spade.svg web/src/ui/icons/shopping-bag.svg web/src/ui/icons/coins.svg web/src/ui/icons/LICENSE-lucide web/src/ui/design.css web/tests/component/icon.spec.ts
git commit -m "feat(web): vendor Lucide game glyphs alongside Remix icons" -- web/src/ui/icons web/src/ui/design.css web/tests/component/icon.spec.ts
```

---

### Task 2: Mute team keels + neutralize connection dots

Pure CSS; no unit test can observe computed `color-mix` in happy-dom. Existing tests guard against regressions; visual verification happens in Task 4.

**Files:**
- Modify: `web/src/ui/tokens.css:18-19`
- Modify: `web/src/ui/design.css:1080-1087` (`.spades-seat-label::before`)

- [ ] **Step 1: Mute the team tokens**

In `web/src/ui/tokens.css`, replace:

```css
  --team-1: color-mix(in oklab, var(--accent) 60%, var(--fg-muted));
  --team-2: color-mix(in oklab, var(--accent-2) 60%, var(--fg-muted));
```

with:

```css
  --team-1: color-mix(in oklab, var(--accent) 35%, var(--fg-muted));
  --team-2: color-mix(in oklab, var(--accent-2) 35%, var(--fg-muted));
```

(These are defined once at `:root`; dark theme only redefines the `--accent*` inputs, so both themes inherit the mute. The lobby `.seat-grid` borders quiet down too — intended per spec.)

- [ ] **Step 2: Neutralize the connection dot**

In `web/src/ui/design.css`, in the `.spades-seat-label::before` block, replace:

```css
  background: var(--accent);
```

with:

```css
  background: var(--fg-subtle);
```

(The disconnected variant directly below already uses `var(--fg-subtle)` for its hollow ring — solid-gray vs hollow-gray becomes the pair.)

- [ ] **Step 3: Run the web test suite**

Run: `pnpm -C web test`
Expected: PASS — all suites green (CSS-only change; this guards against accidental syntax breakage via any snapshot/visual-adjacent tests).

- [ ] **Step 4: Commit**

```bash
git add web/src/ui/tokens.css web/src/ui/design.css
git commit -m "style(web): mute team keels and neutralize connection dots" -- web/src/ui/tokens.css web/src/ui/design.css
```

---

### Task 3: Scoreboard bag glyph

Replace the literal word "Bags" with the `shopping-bag` glyph carrying `aria-label="Bags"` — the accessible name is unchanged.

**Files:**
- Create: `web/tests/component/scores.spec.ts`
- Modify: `web/src/ui/components/scores.ts`

- [ ] **Step 1: Write the failing test**

Create `web/tests/component/scores.spec.ts`:

```ts
import { describe, it, expect, beforeEach } from 'vitest';
import { render } from 'lit-html';
import { scores } from '../../src/ui/components/scores';

const base = {
  teamAScore: 127,
  teamBScore: 94,
  teamABags: 3,
  teamBBags: 1,
  myTeam: 'A' as const,
  centerText: '',
};

describe('scores placard', () => {
  beforeEach(() => {
    document.body.innerHTML = '<main id="root"></main>';
  });

  it('renders both team blocks with scores and bag counts', () => {
    render(scores(base), document.getElementById('root')!);
    const teams = document.querySelectorAll('.spades-scoreboard__team');
    expect(teams).toHaveLength(2);
    expect(teams[0].textContent).toContain('127');
    expect(teams[0].textContent).toContain('3');
    expect(teams[1].textContent).toContain('94');
    expect(teams[1].textContent).toContain('1');
  });

  it('marks the caller team with (You)', () => {
    render(scores({ ...base, myTeam: 'B' }), document.getElementById('root')!);
    const labels = document.querySelectorAll('.spades-scoreboard__label');
    expect(labels[0].textContent).toBe('Team A');
    expect(labels[1].textContent).toBe('Team B (You)');
  });

  it('replaces the Bags word with a labeled bag glyph per team', () => {
    render(scores(base), document.getElementById('root')!);
    const glyphs = document.querySelectorAll(
      '.spades-scoreboard__nums .icon[aria-label="Bags"]',
    );
    expect(glyphs).toHaveLength(2);
    expect(glyphs[0].getAttribute('role')).toBe('img');
    expect(document.querySelector('.spades-scoreboard')!.textContent).not.toContain('Bags');
  });

  it('renders center text only when provided', () => {
    render(scores(base), document.getElementById('root')!);
    expect(document.querySelector('.spades-scoreboard__center')).toBeNull();
    document.body.innerHTML = '<main id="root"></main>';
    render(scores({ ...base, centerText: 'Trick 7' }), document.getElementById('root')!);
    expect(document.querySelector('.spades-scoreboard__center')!.textContent).toBe('Trick 7');
  });
});
```

- [ ] **Step 2: Run test to verify it fails**

Run: `pnpm -C web test tests/component/scores.spec.ts`
Expected: FAIL — "replaces the Bags word…" fails (`expected 0 to have length 2`; the placard still contains the text "Bags"). The other three tests pass against current markup.

- [ ] **Step 3: Swap the word for the glyph**

Replace the full contents of `web/src/ui/components/scores.ts` with:

```ts
import { html, type TemplateResult } from 'lit-html';
import { icon } from '../icon';

export type ScoresProps = {
  teamAScore: number;
  teamBScore: number;
  teamABags: number;
  teamBBags: number;
  myTeam: 'A' | 'B';
  centerText: string;
};

/** One scoreboard chip in the seat-chip language, overlaid on the felt rail. */
export function scores(p: ScoresProps): TemplateResult {
  const team = (
    label: string,
    you: boolean,
    teamNo: 1 | 2,
    score: number,
    bags: number,
  ): TemplateResult =>
    html`<span class="spades-scoreboard__team" data-team=${teamNo}>
      <span class="spades-scoreboard__label">${label}${you ? ' (You)' : ''}</span>
      <span class="spades-scoreboard__nums"
        >${score} · ${icon('shopping-bag', { label: 'Bags' })} ${bags}</span
      >
    </span>`;
  return html`<section class="spades-scoreboard" aria-label="Scores">
    ${team('Team A', p.myTeam === 'A', 1, p.teamAScore, p.teamABags)}
    ${p.centerText ? html`<span class="spades-scoreboard__center">${p.centerText}</span>` : null}
    ${team('Team B', p.myTeam === 'B', 2, p.teamBScore, p.teamBBags)}
  </section>`;
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `pnpm -C web test tests/component/scores.spec.ts`
Expected: PASS (4 tests).

- [ ] **Step 5: Run the full web suite**

Run: `pnpm -C web test`
Expected: PASS — confirms no other component depended on the "Bags" text (pre-verified by grep: nothing does).

- [ ] **Step 6: Format, lint, commit**

```bash
pnpm -C web format && pnpm -C web lint
git add web/src/ui/components/scores.ts web/tests/component/scores.spec.ts
git commit -m "feat(web): replace scoreboard Bags word with bag glyph" -- web/src/ui/components/scores.ts web/tests/component/scores.spec.ts
```

---

### Task 4: Visual pass + keel tuning (user-in-the-loop)

The 35% mix is a starting value; the maintainer picks the final number by eye. This is a taste decision, not a correctness one.

**Files:**
- Possibly modify: `web/src/ui/tokens.css:18-19` (percentage only)

- [ ] **Step 1: Run the app**

Run: `make dev` (backend :3000 + UI :5173). Open `http://localhost:5173`, start a bot game to reach the felt.

- [ ] **Step 2: Verify, in both themes (header toggle)**

- Seat chips: gray connection dot (not teal); keels read as a faint warm/cool tint, not a colored stripe.
- Scoreboard: bag glyph renders as a stroke outline (not a solid blob) at text size, after each `·`; keels muted; `(You)` on the correct team.
- Active turn: accent outline + pulse still present (unchanged).
- Lobby (`/` → create game screen): `.seat-grid` left borders muted but team-distinguishable.
- Disconnected state if reproducible (kill a bot/tab): hollow ring still distinct from the solid gray dot.

- [ ] **Step 3: Tune the keel percentage if needed**

If 35% reads too loud/too dead, adjust both `--team-1` and `--team-2` percentages in `web/src/ui/tokens.css` (stay within 30–40% per spec) and re-check both themes.

- [ ] **Step 4: Run the pre-push gate**

Run: `make check`
Expected: fmt-check, clippy, and all tests green. (Coverage ratchet: new `scores.spec.ts` only raises web coverage; no baseline change expected.)

- [ ] **Step 5: Commit tuning (only if Step 3 changed values)**

```bash
git add web/src/ui/tokens.css
git commit -m "style(web): tune team keel strength" -- web/src/ui/tokens.css
```
