# Frontend Foundations (Phase 0) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Establish the shared design-system layer (tokens, light/dark theming, fonts, icon pipeline, refreshed primitives) that later surface phases build on.

**Architecture:** A new `tokens.css` holds all semantic color tokens (light `:root` + `[data-theme="dark"]` overrides) plus type/space/radius/shadow scales; `theme.ts` is a signals-based controller that persists the choice and follows the OS; icons are vendored Apache-2.0 Remix SVGs inlined via a lit-html `icon()` helper; `design.css` is migrated to consume tokens and restyled to the validated preview. No new JS runtime deps.

**Tech Stack:** TypeScript, Vite, lit-html, `@preact/signals-core`, vitest (unit=node, component=happy-dom), Fontsource (build-time font assets).

**Reference:** Spec at `docs/superpowers/specs/2026-05-30-frontend-foundations-design.md`. Visual source of truth: `web/foundations-preview.html`.

**TDD note:** Behavioral TypeScript (storage, `theme.ts`, `icon.ts`, header toggle) is built test-first. Pure-stylesheet tasks (tokens, fonts, `design.css` migration) cannot be meaningfully unit-tested; they are verified by `pnpm -C web build`, the existing test suite staying green, and a visual diff against the preview. These are explicitly marked **[VERIFY]** rather than **[TDD]**.

**Run all commands from the repo root** unless noted. Web scripts use `pnpm -C web <script>`.

---

### Task 1: Token layer — `tokens.css` **[VERIFY]**

**Files:**
- Create: `web/src/ui/tokens.css`
- Modify: `web/src/main.ts:1` (import tokens before design.css)

- [ ] **Step 1: Create `web/src/ui/tokens.css`**

```css
/* Canonical design tokens. Light = :root; dark overrides under [data-theme="dark"]. */
:root {
  /* surfaces & ink */
  --bg: #f1e6d4;
  --surface: #faf4ea;
  --surface-raised: #fffdf9;
  --fg: #1b2330;
  --fg-muted: #5e6a70;
  --fg-subtle: #948b7c;
  --border: rgba(27, 35, 48, 0.12);
  --border-strong: rgba(27, 35, 48, 0.2);

  /* accents */
  --accent: #1f8f80;
  --accent-hover: #1a796c;
  --accent-2: #e0623f;
  --accent-3: #c08a1e;

  /* semantic */
  --success: #1c8a51;
  --success-tint: #e6f4ea;
  --danger: #c33b2b;
  --danger-tint: #fbe9e6;
  --warning: #a9710a;
  --warning-tint: #faf0d8;

  /* cards & table */
  --card-face: #fffdf8;
  --card-red: #c33b2b;
  --card-ink: #1b2330;
  --card-edge: rgba(27, 35, 48, 0.16);
  --felt: #2f6f62;
  --felt-ink: #eaf3ee;

  /* effects */
  --focus: #1f8f80;
  --shadow-color: 27 35 48;

  color-scheme: light;

  /* fonts (faces wired in Task 6) */
  --font-display: 'Fraunces', Georgia, 'Times New Roman', serif;
  --font-text: 'Hanken Grotesk', system-ui, -apple-system, 'Segoe UI', sans-serif;
  --font-mono: 'IBM Plex Mono', ui-monospace, 'SF Mono', monospace;

  /* fluid type scale */
  --text-xs: 0.78rem;
  --text-sm: 0.875rem;
  --text-base: clamp(1rem, 0.96rem + 0.18vw, 1.0625rem);
  --text-lg: clamp(1.125rem, 1.06rem + 0.3vw, 1.25rem);
  --text-xl: clamp(1.4rem, 1.24rem + 0.7vw, 1.75rem);
  --text-2xl: clamp(1.85rem, 1.5rem + 1.55vw, 2.6rem);
  --text-3xl: clamp(2.4rem, 1.8rem + 2.7vw, 3.6rem);

  /* space scale (7/9/11 intentionally unused) */
  --space-1: 0.25rem;
  --space-2: 0.5rem;
  --space-3: 0.75rem;
  --space-4: 1rem;
  --space-5: 1.25rem;
  --space-6: 1.5rem;
  --space-8: 2rem;
  --space-10: 3rem;
  --space-12: 4rem;
  --gutter: clamp(1rem, 4vw, 2.5rem);

  /* radius (tight) */
  --radius-sm: 2px;
  --radius-md: 4px;
  --radius-lg: 8px;
  --radius-card: 4px;
  --radius-pill: 999px;

  /* elevation (warm-tinted via --shadow-color) */
  --shadow-1: 0 1px 2px rgb(var(--shadow-color) / 0.06), 0 1px 1px rgb(var(--shadow-color) / 0.04);
  --shadow-2: 0 2px 6px rgb(var(--shadow-color) / 0.08), 0 6px 16px rgb(var(--shadow-color) / 0.06);
  --shadow-3: 0 10px 30px rgb(var(--shadow-color) / 0.14), 0 3px 8px rgb(var(--shadow-color) / 0.08);
  --shadow-card: 0 1px 1px rgb(var(--shadow-color) / 0.1), 0 6px 14px rgb(var(--shadow-color) / 0.18);

  /* motion & layout */
  --ease: cubic-bezier(0.2, 0.7, 0.3, 1);
  --dur: 180ms;
  --content-max: 60rem;
}

[data-theme='dark'] {
  --bg: #16140f;
  --surface: #201d17;
  --surface-raised: #2a261e;
  --fg: #f2ecdd;
  --fg-muted: #b0a794;
  --fg-subtle: #7e7567;
  --border: rgba(242, 236, 221, 0.13);
  --border-strong: rgba(242, 236, 221, 0.24);

  --accent: #3cc3b0;
  --accent-hover: #54d0be;
  --accent-2: #f0835f;
  --accent-3: #e0aa3e;

  --success: #41c079;
  --success-tint: #15271c;
  --danger: #ec6a55;
  --danger-tint: #2a1613;
  --warning: #e0aa3e;
  --warning-tint: #2a2210;

  --card-face: #f6f0e4;
  --card-red: #c8402f;
  --card-ink: #1b2330;
  --card-edge: rgba(0, 0, 0, 0.4);
  --felt: #143029;
  --felt-ink: #d9e8e0;

  --focus: #3cc3b0;
  --shadow-color: 0 0 0;

  color-scheme: dark;
}
```

- [ ] **Step 2: Import tokens before design.css in `web/src/main.ts`**

Change line 1 from `import './ui/design.css';` to:

```ts
import './ui/tokens.css';
import './ui/design.css';
```

- [ ] **Step 3: Verify build**

Run: `pnpm -C web build`
Expected: completes with no TypeScript or Vite errors (tokens.css is bundled).

- [ ] **Step 4: Commit**

```bash
git add web/src/ui/tokens.css web/src/main.ts
git commit -m "feat(web): add design token layer (light + dark)"
```

---

### Task 2: Theme persistence in storage **[TDD]**

**Files:**
- Modify: `web/src/lib/storage.ts` (append theme helpers)
- Test: `web/tests/unit/storage.spec.ts` (append a describe block)

- [ ] **Step 1: Write the failing test** — append to `web/tests/unit/storage.spec.ts`:

```ts
import { getThemePref, setThemePref, clearThemePref } from '../../src/lib/storage';

describe('theme preference storage', () => {
  beforeEach(() => {
    const store: Record<string, string> = {};
    vi.stubGlobal('localStorage', {
      getItem: (k: string) => (k in store ? store[k]! : null),
      setItem: (k: string, v: string) => {
        store[k] = v;
      },
      removeItem: (k: string) => {
        delete store[k];
      },
    });
  });

  it('returns null when unset', () => {
    expect(getThemePref()).toBe(null);
  });

  it('round-trips a valid theme', () => {
    setThemePref('dark');
    expect(getThemePref()).toBe('dark');
  });

  it('ignores an invalid stored value', () => {
    localStorage.setItem('spades_theme', 'banana');
    expect(getThemePref()).toBe(null);
  });

  it('clears the preference', () => {
    setThemePref('light');
    clearThemePref();
    expect(getThemePref()).toBe(null);
  });
});
```

- [ ] **Step 2: Run test to verify it fails**

Run: `pnpm -C web test:unit -- storage`
Expected: FAIL — `getThemePref` is not exported / not a function.

- [ ] **Step 3: Implement** — append to `web/src/lib/storage.ts`:

```ts
const THEME_KEY = 'spades_theme';

export function getThemePref(): 'light' | 'dark' | null {
  try {
    const v = localStorage.getItem(THEME_KEY);
    return v === 'light' || v === 'dark' ? v : null;
  } catch {
    return null;
  }
}

export function setThemePref(theme: 'light' | 'dark'): void {
  try {
    localStorage.setItem(THEME_KEY, theme);
  } catch {
    // ignore (private mode)
  }
}

export function clearThemePref(): void {
  try {
    localStorage.removeItem(THEME_KEY);
  } catch {
    // ignore
  }
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `pnpm -C web test:unit -- storage`
Expected: PASS (all describe blocks green).

- [ ] **Step 5: Commit**

```bash
git add web/src/lib/storage.ts web/tests/unit/storage.spec.ts
git commit -m "feat(web): persist theme preference in storage"
```

---

### Task 3: Theme controller — `theme.ts` **[TDD]**

**Files:**
- Create: `web/src/state/theme.ts`
- Test: `web/tests/component/theme.spec.ts` (needs `document`, so component/happy-dom)
- Modify: `web/src/main.ts` (apply theme at bootstrap, before first render)

- [ ] **Step 1: Write the failing test** — create `web/tests/component/theme.spec.ts`:

```ts
import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest';
import { themeState, initialTheme } from '../../src/state/theme';
import { clearThemePref, setThemePref } from '../../src/lib/storage';

describe('theme controller', () => {
  beforeEach(() => {
    clearThemePref();
    document.documentElement.removeAttribute('data-theme');
    vi.stubGlobal('matchMedia', (q: string) => ({
      matches: false,
      media: q,
      addEventListener: () => {},
      removeEventListener: () => {},
    }));
  });
  afterEach(() => vi.restoreAllMocks());

  it('initialTheme falls back to system (light) when unset', () => {
    expect(initialTheme()).toBe('light');
  });

  it('initialTheme honors a stored preference over system', () => {
    setThemePref('dark');
    expect(initialTheme()).toBe('dark');
  });

  it('set() reflects on <html> and persists', () => {
    themeState.set('dark');
    expect(document.documentElement.getAttribute('data-theme')).toBe('dark');
    expect(themeState.theme.value).toBe('dark');
  });

  it('toggle() flips the current theme', () => {
    themeState.set('light');
    themeState.toggle();
    expect(themeState.theme.value).toBe('dark');
  });
});
```

- [ ] **Step 2: Run test to verify it fails**

Run: `pnpm -C web test:component -- theme`
Expected: FAIL — cannot import `../../src/state/theme`.

- [ ] **Step 3: Implement** — create `web/src/state/theme.ts`:

```ts
import { signal } from '@preact/signals-core';
import { getThemePref, setThemePref } from '../lib/storage';

export type Theme = 'light' | 'dark';

function systemTheme(): Theme {
  return globalThis.matchMedia?.('(prefers-color-scheme: dark)').matches ? 'dark' : 'light';
}

export function initialTheme(): Theme {
  return getThemePref() ?? systemTheme();
}

const theme = signal<Theme>(initialTheme());

function apply(t: Theme): void {
  document.documentElement.setAttribute('data-theme', t);
}

function set(t: Theme): void {
  theme.value = t;
  setThemePref(t);
  apply(t);
}

function toggle(): void {
  set(theme.value === 'dark' ? 'light' : 'dark');
}

/** Apply current theme and follow the OS while the user hasn't chosen explicitly. */
function initTheme(): void {
  apply(theme.value);
  const mq = globalThis.matchMedia?.('(prefers-color-scheme: dark)');
  mq?.addEventListener?.('change', (e: MediaQueryListEvent) => {
    if (getThemePref() === null) {
      theme.value = e.matches ? 'dark' : 'light';
      apply(theme.value);
    }
  });
}

export const themeState = { theme, set, toggle, initTheme };
```

- [ ] **Step 4: Run test to verify it passes**

Run: `pnpm -C web test:component -- theme`
Expected: PASS.

- [ ] **Step 5: Apply theme at bootstrap** — in `web/src/main.ts`, add the import and call `initTheme()` as the first statement inside the async IIFE (before `session.refresh()`):

```ts
import { themeState } from './state/theme';
```

Then immediately inside `void (async () => {`:

```ts
  themeState.initTheme();
```

- [ ] **Step 6: Prevent theme flash** — in `web/index.html`, add this inline script in `<head>` immediately after the `<title>` (runs before CSS/JS so the first paint is correct):

```html
<script>
  try {
    var t = localStorage.getItem('spades_theme');
    if (t !== 'light' && t !== 'dark')
      t = matchMedia('(prefers-color-scheme: dark)').matches ? 'dark' : 'light';
    document.documentElement.setAttribute('data-theme', t);
  } catch (e) {}
</script>
```

- [ ] **Step 7: Verify build + full suite**

Run: `pnpm -C web build && pnpm -C web test`
Expected: build succeeds; all unit + component tests pass.

- [ ] **Step 8: Commit**

```bash
git add web/src/state/theme.ts web/tests/component/theme.spec.ts web/src/main.ts web/index.html
git commit -m "feat(web): theme controller with persistence, OS-sync, no-flash boot"
```

---

### Task 4: Icon pipeline — vendored Remix SVGs + `icon()` helper **[TDD]**

**Files:**
- Create: `web/src/ui/icons/*.svg` (vendored), `web/src/ui/icons/LICENSE`
- Create: `web/src/ui/icon.ts`
- Test: `web/tests/component/icon.spec.ts`

- [ ] **Step 1: Vendor the Apache-2.0 SVGs** — run from repo root:

```bash
NAMES="play-fill group-line group-fill robot-2-line robot-2-fill flashlight-fill \
timer-flash-line timer-flash-fill hourglass-fill trophy-line share-forward-line \
user-line settings-3-line notification-3-line close-line sun-line moon-line \
arrow-right-s-line checkbox-circle-fill error-warning-fill logout-box-r-line"
mkdir -p web/src/ui/icons
TREE=$(gh api 'repos/cyberalien/RemixIcon/git/trees/master?recursive=1' --jq '.tree[].path')
for n in $NAMES; do
  p=$(printf '%s\n' "$TREE" | grep -iE "/${n}\.svg$" | head -1)
  [ -n "$p" ] && curl -fsSL "https://raw.githubusercontent.com/cyberalien/RemixIcon/master/$p" -o "web/src/ui/icons/$n.svg" || echo "MISSING: $n"
done
gh api repos/cyberalien/RemixIcon/contents/License --jq '.content' | base64 -d > web/src/ui/icons/LICENSE
ls web/src/ui/icons
```

Expected: 21 `.svg` files + `LICENSE`, no `MISSING:` lines.

- [ ] **Step 2: Write the failing test** — create `web/tests/component/icon.spec.ts`:

```ts
import { describe, it, expect, beforeEach } from 'vitest';
import { render } from 'lit-html';
import { icon } from '../../src/ui/icon';

describe('icon', () => {
  beforeEach(() => {
    document.body.innerHTML = '<main id="root"></main>';
  });

  it('renders an inline svg for a known icon', () => {
    render(icon('sun-line'), document.getElementById('root')!);
    expect(document.querySelector('.icon svg')).not.toBeNull();
  });

  it('a labeled icon exposes role=img + aria-label', () => {
    render(icon('group-line', { label: 'Friends' }), document.getElementById('root')!);
    const el = document.querySelector('.icon')!;
    expect(el.getAttribute('role')).toBe('img');
    expect(el.getAttribute('aria-label')).toBe('Friends');
  });

  it('an unlabeled icon is aria-hidden', () => {
    render(icon('moon-line'), document.getElementById('root')!);
    expect(document.querySelector('.icon')!.getAttribute('aria-hidden')).toBe('true');
  });

  it('returns empty for an unknown icon name', () => {
    render(icon('does-not-exist'), document.getElementById('root')!);
    expect(document.querySelector('.icon')).toBeNull();
  });
});
```

- [ ] **Step 3: Run test to verify it fails**

Run: `pnpm -C web test:component -- icon`
Expected: FAIL — cannot import `../../src/ui/icon`.

- [ ] **Step 4: Implement** — create `web/src/ui/icon.ts`:

```ts
import { html, nothing, type TemplateResult } from 'lit-html';
import { unsafeHTML } from 'lit-html/directives/unsafe-html.js';

// Vite inlines each vendored SVG's source at build time (no runtime fetch).
const raws = import.meta.glob('./icons/*.svg', {
  query: '?raw',
  eager: true,
  import: 'default',
}) as Record<string, string>;

const byName: Record<string, string> = {};
for (const [path, raw] of Object.entries(raws)) {
  const name = path.split('/').pop()!.replace('.svg', '');
  byName[name] = raw;
}

export function icon(name: string, opts: { label?: string; class?: string } = {}): TemplateResult | typeof nothing {
  const raw = byName[name];
  if (!raw) return nothing;
  const cls = opts.class ? `icon ${opts.class}` : 'icon';
  return html`<span
    class=${cls}
    role=${opts.label ? 'img' : nothing}
    aria-label=${opts.label ?? nothing}
    aria-hidden=${opts.label ? nothing : 'true'}
    >${unsafeHTML(raw)}</span
  >`;
}
```

- [ ] **Step 5: Run test to verify it passes**

Run: `pnpm -C web test:component -- icon`
Expected: PASS.

- [ ] **Step 6: Add icon sizing CSS** — append to `web/src/ui/design.css`:

```css
.icon {
  display: inline-flex;
  line-height: 0;
}
.icon svg {
  width: 1em;
  height: 1em;
  fill: currentColor;
}
```

- [ ] **Step 7: Verify build + suite**

Run: `pnpm -C web build && pnpm -C web test`
Expected: build succeeds (glob resolves the vendored SVGs); tests pass.

- [ ] **Step 8: Commit**

```bash
git add web/src/ui/icons web/src/ui/icon.ts web/src/ui/design.css web/tests/component/icon.spec.ts
git commit -m "feat(web): vendored Remix icon pipeline (Apache-2.0, build-time inline)"
```

---

### Task 5: Header theme toggle **[TDD]**

**Files:**
- Modify: `web/src/ui/components/header.ts`
- Test: `web/tests/component/header.spec.ts` (append cases)

- [ ] **Step 1: Write the failing test** — append to `web/tests/component/header.spec.ts`:

```ts
import { icon } from '../../src/ui/icon'; // ensure icon module resolves in this suite
import { themeState } from '../../src/state/theme';

describe('header theme toggle', () => {
  beforeEach(() => {
    document.body.innerHTML = '<main id="root"></main>';
    themeState.set('light');
  });

  it('renders a theme toggle button', () => {
    render(header(), document.getElementById('root')!);
    expect(document.querySelector('[data-testid=theme-toggle]')).not.toBeNull();
  });

  it('clicking the toggle flips the theme on <html>', () => {
    render(header(), document.getElementById('root')!);
    (document.querySelector('[data-testid=theme-toggle]') as HTMLButtonElement).click();
    expect(document.documentElement.getAttribute('data-theme')).toBe('dark');
  });
});
```

(`icon` is imported only to confirm the module graph resolves; it is also used by the implementation.)

- [ ] **Step 2: Run test to verify it fails**

Run: `pnpm -C web test:component -- header`
Expected: FAIL — no `[data-testid=theme-toggle]` element.

- [ ] **Step 3: Implement** — replace the body of `web/src/ui/components/header.ts` with:

```ts
import { html, type TemplateResult } from 'lit-html';
import { session } from '../../state/session';
import { themeState } from '../../state/theme';
import { avatarMenu } from './avatar-menu';
import { icon } from '../icon';

function themeToggle(): TemplateResult {
  const dark = themeState.theme.value === 'dark';
  return html`<button
    class="theme-toggle"
    type="button"
    data-testid="theme-toggle"
    aria-label=${dark ? 'Switch to light theme' : 'Switch to dark theme'}
    @click=${() => themeState.toggle()}
  >
    ${icon(dark ? 'sun-line' : 'moon-line')}
  </button>`;
}

export function header(): TemplateResult {
  const user = session.currentUser.value;
  return html`<header class="site-header">
    <a class="site-title" href="/" data-link>Spades</a>
    <nav class="site-nav">
      ${themeToggle()}
      ${user
        ? avatarMenu(user)
        : html`<a class="site-nav__link" href="/login" data-link data-testid="sign-in">Sign in</a>`}
    </nav>
  </header>`;
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `pnpm -C web test:component -- header`
Expected: PASS (existing sign-in/avatar cases still green).

- [ ] **Step 5: Add toggle CSS** — append to `web/src/ui/design.css`:

```css
.theme-toggle {
  appearance: none;
  cursor: pointer;
  display: inline-grid;
  place-items: center;
  width: 36px;
  height: 36px;
  border-radius: var(--radius-md);
  border: 1px solid var(--border-strong);
  background: var(--surface-raised);
  color: var(--fg-muted);
  font-size: 1.2rem;
}
.theme-toggle:hover {
  color: var(--accent);
  border-color: var(--accent);
}
```

- [ ] **Step 6: Commit**

```bash
git add web/src/ui/components/header.ts web/src/ui/design.css web/tests/component/header.spec.ts
git commit -m "feat(web): theme toggle in site header"
```

---

### Task 6: Self-hosted fonts **[VERIFY]**

**Files:**
- Modify: `web/package.json` (add Fontsource font packages)
- Create: `web/src/ui/fonts.css`
- Modify: `web/src/main.ts` (import fonts.css)

- [ ] **Step 1: Add font packages** — run:

```bash
pnpm -C web add @fontsource-variable/fraunces @fontsource-variable/hanken-grotesk @fontsource/ibm-plex-mono
```

Expected: three packages added to `dependencies` (font assets, not JS runtime libs).

- [ ] **Step 2: Create `web/src/ui/fonts.css`**

```css
@import '@fontsource-variable/fraunces';
@import '@fontsource-variable/hanken-grotesk';
@import '@fontsource/ibm-plex-mono/400.css';
@import '@fontsource/ibm-plex-mono/500.css';
```

- [ ] **Step 3: Import fonts before tokens** — in `web/src/main.ts`, add as the new first line:

```ts
import './ui/fonts.css';
```

(Order: `fonts.css`, then `tokens.css`, then `design.css`.)

- [ ] **Step 4: Apply fonts in base CSS** — in `web/src/ui/design.css`, set `body` to use `var(--font-text)` and add heading defaults. Replace the existing `body { font-family: ... }` declaration with `font-family: var(--font-text);` and append:

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

- [ ] **Step 5: Verify build + suite**

Run: `pnpm -C web build && pnpm -C web test`
Expected: build resolves the `@fontsource` imports and bundles woff2; tests pass.

- [ ] **Step 6: Visual check**

Run: `pnpm -C web dev`, open `http://localhost:5173`, confirm headings render in Fraunces and body in Hanken Grotesk. Stop the dev server.

- [ ] **Step 7: Commit**

```bash
git add web/package.json web/pnpm-lock.yaml web/src/ui/fonts.css web/src/main.ts web/src/ui/design.css
git commit -m "feat(web): self-hosted Fraunces + Hanken Grotesk + IBM Plex Mono"
```

---

### Task 7: Migrate & restyle `design.css` to tokens **[VERIFY]**

This is the largest task: replace every hardcoded color with a token (so dark theme works), tighten radii, restyle primitives to the preview, and make components on the felt set explicit text colors. Work through the steps in order; verify once at the end.

**Files:**
- Modify: `web/src/ui/design.css`

- [ ] **Step 1: Delete the old `:root` block** at the top of `design.css` (the `--color-*`, old `--space-*`, old `--radius-*`, `--font-*` definitions, lines ~1–29). All tokens now come from `tokens.css`. Keep the `* { box-sizing }`, `html, body`, and layout rules below it.

- [ ] **Step 2: Replace color references** throughout `design.css` using this exact mapping (replace every occurrence):

| Old value | New token |
| --- | --- |
| `var(--color-bg)` | `var(--bg)` |
| `var(--color-fg)` | `var(--fg)` |
| `var(--color-muted)` | `var(--fg-muted)` |
| `var(--color-accent)` | `var(--accent)` |
| `var(--color-accent-alt)` | `var(--accent-2)` |
| `var(--color-accent-warm)` | `var(--accent-3)` |
| `var(--color-danger)` | `var(--danger)` |
| `var(--color-card-red)` | `var(--card-red)` |
| `var(--color-card-black)` | `var(--card-ink)` |
| `var(--font-family-sans)` | `var(--font-text)` |
| `var(--font-size-base)` | `var(--text-base)` |
| `var(--font-size-sm)` | `var(--text-sm)` |
| `var(--font-size-lg)` | `var(--text-lg)` |
| `white` (as a surface/bg/text-on-accent) | `var(--surface-raised)` for surfaces; keep `#fff` only as text color on `--accent`/`--danger` fills |
| `#fbf6ee` (card-back base, active seat bg) | `var(--surface-raised)` |
| `#effaf3` (mine/seat success bg) | `var(--success-tint)` |
| `#fff1f1` (toast error bg) | `var(--danger-tint)` |
| `#911` (toast error text) | `var(--danger)` |
| `#f3c8c8` (toast error border) | `color-mix(in oklab, var(--danger) 40%, var(--border))` |
| `#effaf3` / `#1a6b3c` / `#c8edd5` (toast success) | `var(--success-tint)` / `var(--success)` / `color-mix(in oklab, var(--success) 40%, var(--border))` |
| `#fff7e6` / `#f6cf7a` (banner) | `var(--warning-tint)` / `color-mix(in oklab, var(--warning) 45%, var(--border))` |
| `rgba(0, 0, 0, 0.06)` / `0.08` / `0.1` / `0.12` / `0.15` (borders) | `var(--border)` (use `var(--border-strong)` where it was `0.15`+) |
| `rgba(0,0,0,0.12)` shadows in `.toast` | `var(--shadow-3)` |

- [ ] **Step 3: Tighten radii & gutters** — these already map to `--radius-*` from tokens (now 2/4/8), so no change beyond Step 2. Replace the page wrapper `max-width: 720px` / `max-width: 45rem` usages on `.page`/`.spades-table`/`.spades-scores` with `var(--content-max)` where they cap the main column (the 480px form/menu caps stay).

- [ ] **Step 4: Restyle buttons** — replace the `.btn`, `.btn--primary`, `.btn--secondary`, `.btn--danger`, `.btn[disabled]` rules with (from the preview):

```css
.btn {
  appearance: none;
  font: inherit;
  font-weight: 600;
  font-size: var(--text-sm);
  display: inline-flex;
  align-items: center;
  gap: var(--space-2);
  padding: 0.45rem 0.9rem;
  border-radius: var(--radius-md);
  border: 1px solid transparent;
  cursor: pointer;
  transition:
    transform var(--dur) var(--ease),
    background var(--dur) var(--ease),
    box-shadow var(--dur) var(--ease),
    border-color var(--dur) var(--ease);
}
.btn:active {
  transform: translateY(1px);
}
.btn--primary {
  background: var(--accent);
  color: #fff;
  box-shadow: var(--shadow-1);
}
.btn--primary:hover {
  background: var(--accent-hover);
  box-shadow: var(--shadow-2);
}
[data-theme='dark'] .btn--primary {
  color: #06231f;
}
.btn--secondary {
  background: var(--surface-raised);
  color: var(--fg);
  border-color: var(--border-strong);
}
.btn--secondary:hover {
  border-color: var(--accent);
  color: var(--accent);
}
.btn--danger {
  background: var(--danger);
  color: #fff;
}
.btn[disabled] {
  opacity: 0.45;
  cursor: not-allowed;
}
```

- [ ] **Step 5: Restyle the seat chip** (stacked name + big timer + explicit color so it never inherits `--felt-ink`) — replace `.spades-seat-chip` and related rules. The seat label/name line and clock:

```css
.spades-seat-chip {
  display: inline-flex;
  flex-direction: column;
  align-items: flex-start;
  gap: 2px;
  padding: var(--space-2) var(--space-3);
  background: var(--surface-raised);
  color: var(--fg); /* explicit: never inherit --felt-ink */
  border: 1px solid var(--border);
  border-radius: var(--radius-md);
  box-shadow: var(--shadow-1);
}
.spades-seat-label {
  display: inline-flex;
  align-items: center;
  gap: var(--space-2);
  font-size: var(--text-sm);
  font-weight: 500;
}
.spades-clock {
  font-family: var(--font-mono);
  font-size: var(--text-xl);
  font-weight: 500;
  color: var(--fg);
  font-variant-numeric: tabular-nums;
  letter-spacing: -0.02em;
}
```

(Note: the seat markup in `game-table.ts`/`scores.ts` may need the name wrapped in `.spades-seat-label` and the clock in `.spades-clock`; adjust those templates so the dot + name sit on one line and the clock below. Verify against component tests in Step 8.)

- [ ] **Step 6: Card faces — tokenize + paper-white in both themes** — replace the `.card`, `.card-red`, `.card-black`, `.card-back` rules:

```css
.card {
  width: var(--card-w, 46px);
  aspect-ratio: 5 / 7;
  height: auto;
  border-radius: var(--radius-card);
  background: var(--card-face);
  border: 1px solid var(--card-edge);
  color: var(--card-ink);
  box-shadow: var(--shadow-card);
  display: flex;
  align-items: center;
  justify-content: center;
  font-weight: 600;
  font-size: 14px;
  user-select: none;
  position: relative;
}
.card-red {
  color: var(--card-red);
}
.card-black {
  color: var(--card-ink);
}
.card-back {
  background:
    repeating-linear-gradient(45deg, transparent 0 6px, color-mix(in oklab, var(--accent) 45%, transparent) 6px 7px),
    repeating-linear-gradient(-45deg, transparent 0 6px, color-mix(in oklab, var(--accent) 45%, transparent) 6px 7px),
    color-mix(in oklab, var(--card-face) 82%, var(--accent) 6%);
}
```

(Cards now read as paper objects in dark theme. The richer me.uk pip art + clean court component is the **Table phase**, not Phase 0.)

- [ ] **Step 7: Felt panel** — wherever the trick/table center sets a green felt (`game-table.ts` center / `.spades-table-center`), drive it from tokens and set `--felt-ink` text. Add:

```css
.spades-table-center {
  color: var(--felt-ink);
}
```

- [ ] **Step 8: Verify build, full suite, lint, format, visual**

```bash
pnpm -C web build
pnpm -C web test
pnpm -C web lint
pnpm -C web format:check
```

Expected: build + all unit/component tests pass; lint and format clean. Then `pnpm -C web dev` → at `http://localhost:5173` confirm against `web/foundations-preview.html`: light/dark both correct, seat name + timer visible on the felt, cards paper-white in dark, tight radii, no invisible text. Run `pnpm -C web test:e2e` and confirm the Playwright suite stays green. Stop the dev server.

- [ ] **Step 9: Commit**

```bash
git add web/src/ui/design.css web/src/ui/components/game-table.ts web/src/ui/components/scores.ts
git commit -m "refactor(web): migrate design.css to tokens; dark theme + restyled primitives"
```

---

### Task 8: Final verification & cleanup **[VERIFY]**

**Files:**
- Remove: `web/foundations-preview.html` (throwaway, once no longer needed)

- [ ] **Step 1: Full gate**

```bash
pnpm -C web build && pnpm -C web test && pnpm -C web lint && pnpm -C web format:check && pnpm -C web test:e2e
```

Expected: everything green.

- [ ] **Step 2: Confirm no hardcoded colors remain in component CSS**

Run: `grep -nE '#[0-9a-fA-F]{3,6}|rgba?\(' web/src/ui/design.css | grep -vE '#fff|#06231f' || echo "clean"`
Expected: only the intentional `#fff` (text-on-accent) and `#06231f` (dark primary-button text) remain; everything else is tokens. Investigate any other hits.

- [ ] **Step 3: Remove the throwaway preview**

```bash
rm web/foundations-preview.html
```

- [ ] **Step 4: Commit**

```bash
git add -A web/
git commit -m "chore(web): remove throwaway foundations preview"
```

---

## Self-review

**Spec coverage:**
- §4.1 color tokens & theming → Task 1, Task 7.
- §4.2 theme controller (storage, OS-sync, no-flash) → Tasks 2, 3.
- §4.3 typography / fonts → Task 6.
- §4.4 icon pipeline (vendored Apache-2.0, build-time, helper) → Task 4.
- §4.5 cards → Task 7 Step 6 establishes tokens + paper-white + dark-correctness; **me.uk pip art + clean-court component explicitly deferred to the Table phase** (acquisition-blocked; consistent with spec §9 fallback). Flagged in handoff.
- §4.6 primitives, motion, elevation → Tasks 1 (shadow/motion tokens), 5 (toggle), 7 (buttons, seat, cards).
- §6 responsive (fluid scale) → Task 1 (clamp scale, gutter, content-max); structural breakpoints refined per later phase.
- §7 a11y (color-scheme, focus, reduced-motion, labeled toggle, tabular numerals) → Task 1 (`color-scheme`), Task 3/5 (labeled toggle), Task 7 (tabular clock). Existing `:focus-visible`/reduced-motion rules in design.css are retained.
- §8 migration & tests → Tasks 7, 8.

**Deviation from spec §10:** "vendored pip/Ace set" is **not** a Phase 0 deliverable — moved to Table phase to avoid a placeholder (me.uk art is CGI-generated; no static SVG set in hand). Card *tokens* and dark-correctness ship in Phase 0.

**Placeholder scan:** No "TBD"/"add error handling"/"similar to". Each code step shows full content. Two steps ("seat markup in game-table.ts/scores.ts may need adjustment") depend on files not yet read — the executor must inspect those templates; flagged explicitly with a verification gate rather than guessed markup.

**Type consistency:** `themeState` API (`theme`, `set`, `toggle`, `initTheme`) and `initialTheme()` are used consistently across Tasks 3 and 5. `getThemePref`/`setThemePref`/`clearThemePref` consistent across Tasks 2 and 3. `icon(name, opts)` signature consistent across Tasks 4 and 5.

**Open follow-up for the executor:** Tasks 7 Steps 5/7 touch `game-table.ts`/`scores.ts` templates not yet inspected; read them first and adjust seat/felt markup to match the new classes before relying on the verification gate.
