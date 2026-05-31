# Home & Menus Redesign (Phase 2) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Bring the menu-first home onto the design system — swap its hand-rolled inline SVGs for vendored Remix icons, restyle the quick-match tiles and "other ways" rows, and refresh the matchmaking searching state. No landing hero; matchmaking logic unchanged.

**Architecture:** `web/src/routes/home.ts` is a single lit-html route module. We keep its markup class names (so `home.spec.ts` label assertions and the preserved `data-testid`s stay valid) and restyle them in `design.css`, replacing only the icon *content* with `icon()` from the Phase-0 pipeline. The searching state gets a new `.home-searching` block (with a `tier` label threaded through the existing `quickplay` signal — presentational only).

**Tech Stack:** TypeScript, Vite, lit-html, `@preact/signals-core`, vitest (component=happy-dom). Icons already vendored (Phase 0); cards/fonts already in place.

**Reference:** Spec `docs/superpowers/specs/2026-05-31-home-redesign-design.md`. Visual reference: `web/home-preview.html` (throwaway — ignore its hero, which was cut). On branch `home-redesign`.

**TDD note:** The icon swap and searching-state structure have small testable assertions (component tests). The CSS restyle is **[VERIFY]** (build + suite + visual). Run all commands from the repo root.

---

### Task 1: Quick-match tiles & menu rows — Remix icons + restyle **[TDD + VERIFY]**

**Files:**
- Modify: `web/src/routes/home.ts`
- Modify: `web/src/ui/design.css`
- Test: `web/tests/component/home.spec.ts`

- [ ] **Step 1: Confirm the six icons are vendored** (run; expected: all six files listed, no "MISSING"):

```bash
for n in flashlight-fill timer-flash-fill hourglass-fill group-fill robot-2-fill arrow-right-s-line; do
  test -f "web/src/ui/icons/$n.svg" && echo "ok $n" || echo "MISSING $n"
done
```
If any is MISSING, vendor it first via the Phase-0 path (`web/public`? no — icons live in `web/src/ui/icons/`; fetch from `cyberalien/RemixIcon` as the Phase-0 plan documents). All six are expected present.

- [ ] **Step 2: Add the failing test assertion** — in `web/tests/component/home.spec.ts`, in the existing `it('renders the menu with five action buttons', …)` test, after the `expect(labels).toEqual([...])` line (before `cleanup()`), add:

```ts
    // Icons now come from the vendored Remix pipeline (icon() → span.icon > svg), not inline <svg>
    expect(menu!.querySelector('.quickplay-tile .icon svg')).not.toBeNull();
    expect(menu!.querySelector('.menu__row .menu__row-icon .icon svg')).not.toBeNull();
    expect(menu!.querySelector('.menu__row-go .icon svg')).not.toBeNull();
```

- [ ] **Step 3: Run, verify FAIL** — `pnpm -C web test:component -- home`. Expected: the new assertions fail (current icons are bare inline `<svg>`, no `.icon` wrapper).

- [ ] **Step 4: Swap icons in `home.ts`** — make these edits:
  1. Add the import near the top: `import { icon } from '../ui/icon';`
  2. **Delete** the entire `TIER_ICONS` and `ROW_ICONS` `const … : Record<string, TemplateResult> = { … }` declarations (the two large inline-SVG maps).
  3. Add a tier→icon-name map near `QUICKPLAY_TIMERS`:
     ```ts
     const TIER_ICON: Record<string, string> = {
       blitz: 'flashlight-fill',
       rapid: 'timer-flash-fill',
       classic: 'hourglass-fill',
     };
     ```
  4. In the quickplay tile markup, replace `${TIER_ICONS[t.key]}` with `${icon(TIER_ICON[t.key]!)}`.
  5. In the **friends** row, replace `<span class="menu__row-icon">${ROW_ICONS.friends}</span>` with `<span class="menu__row-icon">${icon('group-fill')}</span>` and, as the LAST child of that `<button>` (after the `.menu__row-text` span), add `<span class="menu__row-go">${icon('arrow-right-s-line')}</span>`.
  6. In the **computers** row, the same with `${icon('robot-2-fill')}` and a trailing `<span class="menu__row-go">${icon('arrow-right-s-line')}</span>`.
  7. If `TemplateResult` is now only used in type positions that no longer exist, leave the existing `import type { TemplateResult }` if still referenced (the `template(): TemplateResult` return type still uses it) — do not remove it.

- [ ] **Step 5: Restyle in `design.css`** — replace the `.menu__row-icon` + `.menu__row-icon svg` rules with an icon chip, add `.menu__row-go`, and replace the tile `svg` sizing with `.icon` sizing + Fraunces time. Specifically:
  - Replace:
    ```css
    .menu__row-icon {
      display: inline-flex;
      flex: none;
      color: var(--row-accent, var(--fg));
    }
    .menu__row-icon svg {
      width: 24px;
      height: 24px;
    }
    ```
    with:
    ```css
    .menu__row-icon {
      display: grid;
      place-items: center;
      width: 42px;
      height: 42px;
      flex: none;
      border-radius: var(--radius-md);
      background: color-mix(in oklab, var(--row-accent, var(--accent)) 14%, transparent);
      color: var(--row-accent, var(--accent));
      font-size: 1.4rem;
    }
    .menu__row-go {
      margin-left: auto;
      color: var(--fg-subtle);
      font-size: 1.3rem;
      display: inline-flex;
    }
    ```
  - Replace the `.quickplay-tile svg { … }` rule with:
    ```css
    .quickplay-tile .icon {
      font-size: 1.6rem;
      color: var(--tier, var(--fg));
    }
    ```
  - Replace the `.quickplay-tile__time` rule body with:
    ```css
    .quickplay-tile__time {
      font-family: var(--font-display);
      font-weight: 600;
      font-size: 1.35rem;
    }
    ```

- [ ] **Step 6: Run, verify PASS** — `pnpm -C web test:component -- home` (the new icon assertions pass; the existing label/5-button assertions still pass because class names + tile text are unchanged).

- [ ] **Step 7: Verify gate** — `pnpm -C web build && pnpm -C web test && pnpm -C web lint && pnpm -C web format:check`. All green. (No unused-symbol lint errors from the removed maps.)

- [ ] **Step 8: Commit**

```bash
git add web/src/routes/home.ts web/src/ui/design.css web/tests/component/home.spec.ts
git commit -m "feat(web): home tiles/rows use vendored Remix icons + icon chips + chevrons"
```

---

### Task 2: Matchmaking "searching" state refresh **[TDD + VERIFY]**

**Files:**
- Modify: `web/src/routes/home.ts`
- Modify: `web/src/ui/design.css`
- Test: `web/tests/component/home.spec.ts`

- [ ] **Step 1: Update the test for the new searching markup** — in `web/tests/component/home.spec.ts`, in the `it('clicking a quickplay button shows the waiting view', …)` test, change the cancel-button selector from `.quickplay-wait button` to `.home-searching button` (two occurrences if present — the `querySelector` line). Keep the `expect(document.body.textContent).toContain('Finding players')` assertion unchanged. Also add, right after that `toContain('Finding players')` line:

```ts
    expect(document.body.textContent).toContain('of 4 seated');
```

- [ ] **Step 2: Run, verify FAIL** — `pnpm -C web test:component -- home`. Expected: fails (no `.home-searching` element / no "of 4 seated" text yet).

- [ ] **Step 3: Thread `tier` + render the new state in `home.ts`**:
  1. Change the `QuickplayState` type to include the tier label:
     ```ts
     type QuickplayState = { waiting: number; cancel: () => void; tier: string } | null;
     ```
  2. Change `onSeek` to accept the tier label and store it. Update the signature to `function onSeek(timer: TimerCfg, tier: string): void`, set `quickplay.value = { waiting: parsed.waiting as number, cancel, tier };` in the `queue_status` branch, and the initial `quickplay.value = { waiting: 0, cancel, tier };` at the end of `onSeek`.
  3. Update the tile click handler to pass the tier: `@click=${() => onSeek(t.value, t.tier)}`.
  4. Replace the `quickplay.value` branch of `template()` (the `.quickplay-wait` block) with:
     ```ts
     if (quickplay.value) {
       const q = quickplay.value;
       return appShell(html`
         <div class="home-searching" data-testid="home-searching">
           <div class="home-searching__dots" aria-hidden="true"><i></i><i></i><i></i><i></i></div>
           <p class="home-searching__msg">Finding players…</p>
           <p class="home-searching__sub">${q.waiting} of 4 seated · ${q.tier}</p>
           ${button({ label: 'Cancel', onClick: q.cancel, variant: 'secondary' })}
         </div>
       `);
     }
     ```

- [ ] **Step 4: Run, verify PASS** — `pnpm -C web test:component -- home`.

- [ ] **Step 5: Add `.home-searching` CSS** — append to `web/src/ui/design.css`:

```css
.home-searching {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: var(--space-3);
  padding: var(--space-8);
  width: 100%;
  max-width: 360px;
  background: var(--surface);
  border: 1px solid var(--border);
  border-radius: var(--radius-lg);
}
.home-searching__dots {
  display: flex;
  gap: var(--space-2);
}
.home-searching__dots i {
  width: 9px;
  height: 9px;
  border-radius: 50%;
  background: var(--accent);
  opacity: 0.3;
}
.home-searching__msg {
  font-family: var(--font-display);
  font-size: var(--text-lg);
  margin: 0;
}
.home-searching__sub {
  color: var(--fg-muted);
  font-size: var(--text-sm);
  margin: 0;
}
@media (prefers-reduced-motion: no-preference) {
  .home-searching__dots i {
    animation: home-dot 1.2s infinite;
  }
  .home-searching__dots i:nth-child(2) {
    animation-delay: 0.2s;
  }
  .home-searching__dots i:nth-child(3) {
    animation-delay: 0.4s;
  }
  .home-searching__dots i:nth-child(4) {
    animation-delay: 0.6s;
  }
  @keyframes home-dot {
    0%,
    100% {
      opacity: 0.25;
      transform: scale(0.85);
    }
    50% {
      opacity: 1;
      transform: scale(1);
    }
  }
}
```

- [ ] **Step 6: Verify gate** — `pnpm -C web build && pnpm -C web test && pnpm -C web lint && pnpm -C web format:check`. All green.

- [ ] **Step 7: Commit**

```bash
git add web/src/routes/home.ts web/src/ui/design.css web/tests/component/home.spec.ts
git commit -m "feat(web): refreshed matchmaking searching state (animated dots + tier)"
```

---

### Task 3: Final verify & cleanup **[VERIFY]**

**Files:**
- Remove: `web/home-preview.html` (throwaway)

- [ ] **Step 1: Full gate**

```bash
pnpm -C web build && pnpm -C web test && pnpm -C web lint && pnpm -C web format:check
```
Expected: all green (66 unit + 62 component, give or take the added assertions).

- [ ] **Step 2: Confirm no dead inline-SVG icon code or stray selectors remain**:

```bash
grep -n 'TIER_ICONS\|ROW_ICONS' web/src/routes/home.ts || echo "inline icon maps gone (good)"
grep -n 'quickplay-wait\|quickplay-tile svg\|menu__row-icon svg' web/src/ui/design.css || echo "old icon/wait rules gone (good)"
```
Expected: both report the "gone" message (the inline maps removed in Task 1, the `.quickplay-wait` class replaced by `.home-searching` and the raw-svg sizing rules replaced in Tasks 1–2).

- [ ] **Step 3: e2e selector check** — confirm the searching phrase the smoke test relies on is intact:

```bash
grep -rn "Finding players" web/src/routes/home.ts web/tests/e2e
```
Expected: the phrase is present in `home.ts` (the `.home-searching__msg`) and still matched by `web/tests/e2e/smoke.spec.ts`. Run `pnpm -C web test:e2e` with the Rust backend available (CI), or note it for CI.

- [ ] **Step 4: Remove the throwaway preview**

```bash
rm -f web/home-preview.html
```

- [ ] **Step 5: Commit**

```bash
git add -A web/
git commit -m "chore(web): remove throwaway home preview"
```

---

## Self-review

**Spec coverage:**
- §3/§4.1 Remix icon swap on tiles + rows (flashlight/timer/hourglass, group/robot, arrow chevron) → Task 1.
- §3 tiles restyle (tier border already present from Phase 0; Fraunces time + bigger icon) + rows restyle (icon chip + chevron) → Task 1.
- §4.2 icons confirmed vendored → Task 1 Step 1.
- §4.3 searching state (dots + msg + "N of 4 seated · tier", `tier` threaded through `quickplay`/`onSeek`, reduced-motion-gated) → Task 2.
- §4.4 CSS on tokens; dead-rule removal → Tasks 1–2 (the only rules that go dead — `.quickplay-tile svg`, `.menu__row-icon svg`, `.quickplay-wait` — are replaced in place); Task 3 Step 2 verifies none linger.
- §6 tests (preserve `data-testid`s + labels; assert `.icon svg`; searching path) → Tasks 1–2; e2e "Finding players" → Task 3.
- §1 no hero → nothing adds one (verified: no hero markup in any task).

**Placeholder scan:** No TBD/"handle errors"/"similar to". Every code/CSS step is complete; the one conditional ("vendor it if MISSING") is guarded by an explicit check and points at the documented Phase-0 path.

**Type consistency:** `QuickplayState` gains `tier: string` (Task 2) and every writer (`onSeek` initial + `queue_status`) supplies it; `onSeek(timer, tier)` updated at its one call site. `TIER_ICON` keys (`blitz`/`rapid`/`classic`) match `QUICKPLAY_TIMERS[].key`. `icon(name)` calls use names verified present in Task 1 Step 1. The searching class `.home-searching` is consistent across `home.ts`, the CSS, and the updated `home.spec.ts` selector.

**Note for executor:** keep the markup class names (`.menu`, `.menu__row`, `.menu__row-title`, `.quickplay-tile`, `.quickplay-tile__time`, etc.) and the `data-testid`s exactly — `home.spec` test 1 asserts the five labels via `.menu__row-title`/textContent, and the restyle must not rename them.
