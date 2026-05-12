# spades-ts — Plan 1: Scaffold Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Stand up the empty TypeScript SPA shell — Vite + TS + lint + Vitest + Playwright + design tokens + router + a header and a Home route with menu buttons wired to `console.log` — so subsequent plans can fill in real features.

**Architecture:** Single-page app, vanilla TypeScript, `lit-html` for templates, `@preact/signals-core` for reactive state, `navaid` for routing. Layered source tree (`src/{api,state,routes,ui,cards,lib,router,main}.ts`). Built with Vite; same-origin deploy in prod, separate dev server in dev (`VITE_API_URL` env).

**Tech Stack:** Vite 5, TypeScript 5, pnpm, `lit-html`, `@preact/signals-core`, `navaid`, ESLint, Prettier, Vitest, `@testing-library/dom`, happy-dom, Playwright.

**Reference spec:** `/Users/wlim/Projects/spades-ts/docs/superpowers/specs/2026-05-11-spades-ts-design.md`

---

## Files this plan creates

| Path                           | Responsibility                                                    |
| ------------------------------ | ----------------------------------------------------------------- |
| `package.json`                 | Project manifest, scripts, deps                                   |
| `pnpm-lock.yaml`               | Locked dependency tree                                            |
| `tsconfig.json`                | Strict TS for `src/`                                              |
| `tsconfig.node.json`           | TS for Vite config and Node tooling                               |
| `vite.config.ts`               | Vite config, env injection                                        |
| `.eslintrc.cjs`                | Lint config                                                       |
| `.prettierrc.json`             | Format config                                                     |
| `.gitignore`                   | Standard ignores                                                  |
| `.env.development`             | `VITE_API_URL=http://localhost:3000`                              |
| `.env.production`              | `VITE_API_URL=https://spades.wlim.dev`                            |
| `index.html`                   | Vite entry, mounts `<main id="root">`                             |
| `src/main.ts`                  | Bootstraps router                                                 |
| `src/router.ts`                | `navaid` wrapper; `mount(routes)`, `navigate(path)`               |
| `src/lib/util.ts`              | `navigateTo`, env helpers                                         |
| `src/ui/design.css`            | CSS variables (color/space/radius/type), reset, base body         |
| `src/ui/templates.ts`          | `appShell(children)` lit-html template (header + main slot)       |
| `src/ui/components/header.ts`  | Site header (`Spades` title, placeholder sign-in slot)            |
| `src/ui/components/button.ts`  | `button({ variant, onClick }, label)` lit-html template           |
| `src/routes/home.ts`           | `render() → cleanup` — menu shell, buttons wired to `console.log` |
| `src/routes/notfound.ts`       | 404                                                               |
| `tests/unit/router.spec.ts`    | Vitest unit test for the router wrapper                           |
| `tests/component/home.spec.ts` | Component test asserting Home renders the four menu items         |
| `tests/e2e/smoke.spec.ts`      | Playwright: open `/`, assert title and four menu buttons          |
| `playwright.config.ts`         | Playwright config                                                 |
| `vitest.config.ts`             | Vitest config (workspaces: unit + component)                      |
| `happydom.setup.ts`            | Component test setup                                              |
| `README.md`                    | Minimal — repo purpose, dev workflow                              |

No card/game/auth code in this plan.

---

## Task 1: Initialize the repo and tooling

**Files:**

- Create: `package.json`, `pnpm-lock.yaml`, `.gitignore`, `tsconfig.json`, `tsconfig.node.json`, `.eslintrc.cjs`, `.prettierrc.json`, `vite.config.ts`, `vitest.config.ts`, `playwright.config.ts`, `index.html`, `.env.development`, `.env.production`, `src/main.ts`, `src/ui/design.css`

This task gets the project to "vite dev runs and serves an empty `#root`".

- [ ] **Step 1: Create `.gitignore`**

```gitignore
node_modules/
dist/
.DS_Store
*.log
.env.local
.vite/
coverage/
playwright-report/
test-results/
```

- [ ] **Step 2: Create `package.json`**

```json
{
  "name": "spades-ts",
  "private": true,
  "version": "0.0.0",
  "type": "module",
  "scripts": {
    "dev": "vite",
    "build": "tsc -p tsconfig.json --noEmit && vite build",
    "preview": "vite preview",
    "lint": "eslint . --max-warnings=0",
    "format": "prettier --write .",
    "format:check": "prettier --check .",
    "test:unit": "vitest run --project=unit",
    "test:component": "vitest run --project=component",
    "test:watch": "vitest",
    "test:e2e": "playwright test",
    "test": "pnpm test:unit && pnpm test:component"
  },
  "dependencies": {
    "@preact/signals-core": "^1.8.0",
    "lit-html": "^3.2.0",
    "navaid": "^1.2.0"
  },
  "devDependencies": {
    "@playwright/test": "^1.48.0",
    "@testing-library/dom": "^10.4.0",
    "@types/node": "^22.0.0",
    "@typescript-eslint/eslint-plugin": "^8.0.0",
    "@typescript-eslint/parser": "^8.0.0",
    "eslint": "^9.0.0",
    "eslint-config-prettier": "^9.1.0",
    "happy-dom": "^15.0.0",
    "prettier": "^3.3.0",
    "typescript": "^5.6.0",
    "vite": "^5.4.0",
    "vitest": "^2.1.0"
  },
  "packageManager": "pnpm@9.12.0"
}
```

- [ ] **Step 3: Install deps**

Run: `pnpm install`
Expected: lockfile created; `node_modules/` populated; no errors.

- [ ] **Step 4: Create `tsconfig.json`**

```json
{
  "compilerOptions": {
    "target": "ES2022",
    "lib": ["ES2022", "DOM", "DOM.Iterable"],
    "module": "ESNext",
    "moduleResolution": "Bundler",
    "strict": true,
    "noUncheckedIndexedAccess": true,
    "noImplicitOverride": true,
    "exactOptionalPropertyTypes": true,
    "noFallthroughCasesInSwitch": true,
    "isolatedModules": true,
    "esModuleInterop": true,
    "skipLibCheck": true,
    "resolveJsonModule": true,
    "useDefineForClassFields": true,
    "allowImportingTsExtensions": false,
    "verbatimModuleSyntax": true,
    "jsx": "preserve",
    "types": ["vite/client"]
  },
  "include": ["src", "tests"],
  "exclude": ["dist", "node_modules"]
}
```

- [ ] **Step 5: Create `tsconfig.node.json`**

```json
{
  "compilerOptions": {
    "target": "ES2022",
    "module": "ESNext",
    "moduleResolution": "Bundler",
    "strict": true,
    "skipLibCheck": true,
    "esModuleInterop": true,
    "types": ["node"]
  },
  "include": ["vite.config.ts", "vitest.config.ts", "playwright.config.ts", "happydom.setup.ts"]
}
```

- [ ] **Step 6: Create `.eslintrc.cjs`**

```js
/* eslint-env node */
module.exports = {
  root: true,
  parser: '@typescript-eslint/parser',
  parserOptions: { ecmaVersion: 2022, sourceType: 'module' },
  plugins: ['@typescript-eslint'],
  extends: ['eslint:recommended', 'plugin:@typescript-eslint/recommended', 'prettier'],
  ignorePatterns: ['dist', 'node_modules', 'coverage', 'playwright-report'],
  rules: {
    '@typescript-eslint/no-unused-vars': ['error', { argsIgnorePattern: '^_' }],
    '@typescript-eslint/consistent-type-imports': ['error', { fixStyle: 'inline-type-imports' }],
  },
};
```

- [ ] **Step 7: Create `.prettierrc.json`**

```json
{
  "semi": true,
  "singleQuote": true,
  "trailingComma": "all",
  "printWidth": 100,
  "arrowParens": "always"
}
```

- [ ] **Step 8: Create env files**

`.env.development`:

```
VITE_API_URL=http://localhost:3000
```

`.env.production`:

```
VITE_API_URL=https://spades.wlim.dev
```

- [ ] **Step 9: Create `vite.config.ts`**

```ts
import { defineConfig } from 'vite';
import { execSync } from 'node:child_process';

const buildVersion = (() => {
  try {
    return execSync('git rev-parse --short HEAD').toString().trim();
  } catch {
    return 'dev';
  }
})();

export default defineConfig({
  base: '/',
  build: { outDir: 'dist', sourcemap: true },
  server: { port: 5173, strictPort: true },
  define: {
    __BUILD_VERSION__: JSON.stringify(buildVersion),
  },
});
```

- [ ] **Step 10: Create `index.html`**

```html
<!doctype html>
<html lang="en-us">
  <head>
    <meta charset="UTF-8" />
    <meta name="viewport" content="width=device-width, initial-scale=1" />
    <title>Spades</title>
    <link rel="stylesheet" href="/src/ui/design.css" />
  </head>
  <body>
    <main id="root"></main>
    <script type="module" src="/src/main.ts"></script>
  </body>
</html>
```

- [ ] **Step 11: Create `src/ui/design.css` (minimal tokens + reset)**

```css
:root {
  --color-bg: #f7ede2;
  --color-fg: #011627;
  --color-muted: #6b6b6b;
  --color-accent: #2a9d8f;
  --color-danger: #c33;
  --color-card-red: #c33;
  --color-card-black: #011627;

  --space-1: 0.25rem;
  --space-2: 0.5rem;
  --space-3: 0.75rem;
  --space-4: 1rem;
  --space-6: 1.5rem;
  --space-8: 2rem;

  --radius-sm: 4px;
  --radius-md: 8px;
  --radius-lg: 12px;

  --font-family-sans:
    system-ui, -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, Oxygen, Ubuntu, Cantarell,
    'Open Sans', 'Helvetica Neue', sans-serif;
  --font-size-base: 1rem;
  --font-size-sm: 0.875rem;
  --font-size-lg: 1.125rem;
}

* {
  box-sizing: border-box;
}

html,
body {
  margin: 0;
  padding: 0;
}

body {
  font-family: var(--font-family-sans);
  font-size: var(--font-size-base);
  background: var(--color-bg);
  color: var(--color-fg);
  min-height: 100vh;
  display: flex;
  flex-direction: column;
}

main#root {
  flex: 1;
  display: flex;
  flex-direction: column;
  align-items: center;
  padding: var(--space-6) var(--space-4);
}
```

- [ ] **Step 12: Create `src/main.ts` (smoke-only for now)**

```ts
import './ui/design.css';

const root = document.getElementById('root');
if (root) {
  root.textContent = 'spades-ts boot ok';
}
```

- [ ] **Step 13: Verify `pnpm dev` runs and serves the page**

Run: `pnpm dev` (in a separate terminal, or background it: `pnpm dev &`)
Expected: Vite serves on `http://localhost:5173`; `curl -s http://localhost:5173 | grep "<main id=\"root\">"` succeeds. Stop the dev server.

- [ ] **Step 14: Verify lint and format**

Run: `pnpm lint && pnpm format:check`
Expected: both succeed. (If `format:check` fails on auto-generated files, run `pnpm format` first.)

- [ ] **Step 15: Commit**

```bash
git add .
git commit -m "chore: scaffold vite + ts + lint + format"
```

---

## Task 2: Wire Vitest with unit + component projects

**Files:**

- Create: `vitest.config.ts`, `happydom.setup.ts`, `tests/unit/sanity.spec.ts`, `tests/component/sanity.spec.ts`

Two Vitest "projects" so unit tests are fast (node env) and component tests get `happy-dom`.

- [ ] **Step 1: Create `vitest.config.ts`**

```ts
import { defineConfig } from 'vitest/config';

export default defineConfig({
  test: {
    projects: [
      {
        extends: true,
        test: {
          name: 'unit',
          include: ['tests/unit/**/*.spec.ts'],
          environment: 'node',
        },
      },
      {
        extends: true,
        test: {
          name: 'component',
          include: ['tests/component/**/*.spec.ts'],
          environment: 'happy-dom',
          setupFiles: ['./happydom.setup.ts'],
        },
      },
    ],
  },
});
```

- [ ] **Step 2: Create `happydom.setup.ts`**

```ts
// happy-dom provides window/document globally; this file exists so we can
// add globals (e.g. fetch mocks) later. Empty for now.
export {};
```

- [ ] **Step 3: Write sanity unit test**

`tests/unit/sanity.spec.ts`:

```ts
import { describe, it, expect } from 'vitest';

describe('sanity', () => {
  it('runs unit tests in a node environment', () => {
    expect(typeof window).toBe('undefined');
    expect(2 + 2).toBe(4);
  });
});
```

- [ ] **Step 4: Write sanity component test**

`tests/component/sanity.spec.ts`:

```ts
import { describe, it, expect } from 'vitest';

describe('sanity', () => {
  it('runs component tests in a DOM environment', () => {
    expect(typeof window).toBe('object');
    const el = document.createElement('div');
    el.textContent = 'hi';
    expect(el.textContent).toBe('hi');
  });
});
```

- [ ] **Step 5: Run both projects**

Run: `pnpm test`
Expected: 2 tests across 2 projects pass.

- [ ] **Step 6: Commit**

```bash
git add vitest.config.ts happydom.setup.ts tests/
git commit -m "test: configure vitest unit + component projects"
```

---

## Task 3: Add Playwright with a placeholder smoke test

**Files:**

- Create: `playwright.config.ts`, `tests/e2e/smoke.spec.ts`

The smoke test is intentionally trivial here — Task 6 will rewrite it once the Home route exists.

- [ ] **Step 1: Install Playwright browsers**

Run: `pnpm exec playwright install --with-deps chromium`
Expected: Chromium downloaded. (Skip `--with-deps` on macOS where it isn't needed.)

- [ ] **Step 2: Create `playwright.config.ts`**

```ts
import { defineConfig, devices } from '@playwright/test';

export default defineConfig({
  testDir: 'tests/e2e',
  fullyParallel: true,
  retries: 0,
  reporter: [['list']],
  use: {
    baseURL: 'http://localhost:5173',
    trace: 'on-first-retry',
  },
  webServer: {
    command: 'pnpm dev',
    url: 'http://localhost:5173',
    reuseExistingServer: !process.env.CI,
    timeout: 30_000,
  },
  projects: [
    {
      name: 'chromium',
      use: { ...devices['Desktop Chrome'] },
    },
  ],
});
```

- [ ] **Step 3: Write placeholder smoke test**

`tests/e2e/smoke.spec.ts`:

```ts
import { test, expect } from '@playwright/test';

test('app boots', async ({ page }) => {
  await page.goto('/');
  await expect(page).toHaveTitle('Spades');
  await expect(page.locator('#root')).not.toBeEmpty();
});
```

- [ ] **Step 4: Run Playwright**

Run: `pnpm test:e2e`
Expected: 1 test passes (Vite is auto-started by `webServer`).

- [ ] **Step 5: Commit**

```bash
git add playwright.config.ts tests/e2e/smoke.spec.ts
git commit -m "test: add playwright with smoke test"
```

---

## Task 4: Router wrapper with unit test

**Files:**

- Create: `src/router.ts`, `src/lib/util.ts`, `tests/unit/router.spec.ts`

The router is responsible for: mapping a URL to a route module's `render` function, calling the previous route's `cleanup` before mounting the next, and re-rendering on `popstate`/`navigate`.

The wrapper is small and testable in node — we don't need a real browser to assert routing behavior. We'll fake `window` interactions through a thin abstraction.

- [ ] **Step 1: Create `src/lib/util.ts`**

```ts
export const API_URL: string = (import.meta.env.VITE_API_URL as string | undefined) ?? '';

export function navigateTo(path: string): void {
  if (typeof history !== 'undefined') {
    history.pushState(null, '', path);
    window.dispatchEvent(new PopStateEvent('popstate'));
  }
}
```

- [ ] **Step 2: Write failing router test**

`tests/unit/router.spec.ts`:

```ts
import { describe, it, expect, vi, beforeEach } from 'vitest';
import { createRouter, type RouteModule } from '../../src/router';

describe('createRouter', () => {
  let calls: string[];
  let cleanups: string[];

  const makeRoute = (name: string): RouteModule<Record<string, string>> => ({
    render: (params) => {
      calls.push(`${name}(${JSON.stringify(params)})`);
      return () => {
        cleanups.push(name);
      };
    },
  });

  beforeEach(() => {
    calls = [];
    cleanups = [];
  });

  it('calls the matching route render with params', () => {
    const router = createRouter({
      '/': makeRoute('home'),
      '/u/:name': makeRoute('profile'),
    });
    router.handle('/u/alice');
    expect(calls).toEqual(['profile({"name":"alice"})']);
  });

  it('runs the previous route cleanup before mounting the next', () => {
    const router = createRouter({
      '/': makeRoute('home'),
      '/u/:name': makeRoute('profile'),
    });
    router.handle('/');
    router.handle('/u/bob');
    expect(cleanups).toEqual(['home']);
    expect(calls).toEqual(['home({})', 'profile({"name":"bob"})']);
  });

  it('falls back to the wildcard route for unknown paths', () => {
    const router = createRouter({
      '/': makeRoute('home'),
      '*': makeRoute('notfound'),
    });
    router.handle('/nope');
    expect(calls).toEqual(['notfound({"wild":"nope"})']);
  });

  it('passes search params via second argument', () => {
    const seen: string[] = [];
    const router = createRouter({
      '/login': {
        render: (_p, ctx) => {
          seen.push(ctx.search.get('next') ?? '');
          return () => {};
        },
      },
    });
    router.handle('/login?next=/me');
    expect(seen).toEqual(['/me']);
  });
});
```

- [ ] **Step 3: Run test to verify it fails**

Run: `pnpm test:unit`
Expected: FAIL — `src/router.ts` does not exist.

- [ ] **Step 4: Implement `src/router.ts`**

```ts
import navaid from 'navaid';

export type RouteContext = {
  path: string;
  search: URLSearchParams;
};

export type RouteModule<P extends Record<string, string> = Record<string, string>> = {
  render: (params: P, ctx: RouteContext) => () => void;
};

type Routes = Record<string, RouteModule>;

export type Router = {
  handle: (path: string) => void;
  listen: () => void;
};

export function createRouter(routes: Routes): Router {
  const r = navaid('/', (uri) => {
    // navaid's wildcard handler — we get the unmatched URI.
    const mod = routes['*'];
    if (!mod) return;
    runRoute(mod, { wild: uri ?? '' }, uri ?? '');
  });

  let currentCleanup: (() => void) | null = null;

  function runRoute(mod: RouteModule, params: Record<string, string>, fullPath: string): void {
    if (currentCleanup) currentCleanup();
    const search = new URLSearchParams(
      fullPath.includes('?') ? fullPath.slice(fullPath.indexOf('?')) : '',
    );
    currentCleanup = mod.render(params, { path: fullPath, search });
  }

  for (const [pattern, mod] of Object.entries(routes)) {
    if (pattern === '*') continue;
    r.on(pattern, (params) => {
      runRoute(mod, (params ?? {}) as Record<string, string>, locationPath());
    });
  }

  function locationPath(): string {
    if (typeof location === 'undefined') return '/';
    return location.pathname + location.search;
  }

  return {
    handle: (path: string) => r.run(path),
    listen: () => r.listen(),
  };
}
```

- [ ] **Step 5: Run test to verify it passes**

Run: `pnpm test:unit`
Expected: PASS — all 4 router cases green.

- [ ] **Step 6: Commit**

```bash
git add src/router.ts src/lib/util.ts tests/unit/router.spec.ts
git commit -m "feat: add router wrapper with cleanup semantics"
```

---

## Task 5: App shell + header + Home route

**Files:**

- Create: `src/ui/templates.ts`, `src/ui/components/header.ts`, `src/ui/components/button.ts`, `src/routes/home.ts`, `src/routes/notfound.ts`
- Modify: `src/main.ts`

Home renders the four primary entry points (`Quick Play 5+3 / 10+5 / 15+10`, `Play with Friends`, `Play with Computers`) wired to `console.log`. No state, no API yet — just the menu shell so visual + routing work happens here.

- [ ] **Step 1: Create `src/ui/components/button.ts`**

```ts
import { html, type TemplateResult } from 'lit-html';

export type ButtonVariant = 'primary' | 'secondary' | 'danger';

export function button(opts: {
  label: string;
  onClick: (e: Event) => void;
  variant?: ButtonVariant;
  disabled?: boolean;
}): TemplateResult {
  const variant = opts.variant ?? 'primary';
  return html`<button
    type="button"
    class="btn btn--${variant}"
    ?disabled=${opts.disabled ?? false}
    @click=${opts.onClick}
  >
    ${opts.label}
  </button>`;
}
```

- [ ] **Step 2: Create `src/ui/components/header.ts`**

```ts
import { html, type TemplateResult } from 'lit-html';

export function header(): TemplateResult {
  return html`<header class="site-header">
    <a class="site-title" href="/" data-link>Spades</a>
    <nav class="site-nav">
      <!-- sign-in slot (filled in Plan 3) -->
    </nav>
  </header>`;
}
```

- [ ] **Step 3: Create `src/ui/templates.ts`**

```ts
import { html, type TemplateResult } from 'lit-html';
import { header } from './components/header';

export function appShell(children: TemplateResult): TemplateResult {
  return html`${header()}
    <section class="page">${children}</section>`;
}
```

- [ ] **Step 4: Add minimal styles for shell + buttons to `src/ui/design.css`**

Append:

```css
.site-header {
  width: 100%;
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: var(--space-3) var(--space-6);
  border-bottom: 1px solid rgba(0, 0, 0, 0.08);
  background: rgba(255, 255, 255, 0.4);
}
.site-title {
  font-weight: 600;
  font-size: var(--font-size-lg);
  color: var(--color-fg);
  text-decoration: none;
}
.site-nav {
  display: flex;
  gap: var(--space-3);
}

.page {
  width: 100%;
  max-width: 720px;
  display: flex;
  flex-direction: column;
  align-items: center;
  padding: var(--space-6) var(--space-4);
}

.btn {
  appearance: none;
  border: none;
  border-radius: var(--radius-md);
  padding: var(--space-3) var(--space-6);
  font: inherit;
  cursor: pointer;
}
.btn--primary {
  background: var(--color-fg);
  color: white;
}
.btn--secondary {
  background: rgba(0, 0, 0, 0.06);
  color: var(--color-fg);
}
.btn--danger {
  background: var(--color-danger);
  color: white;
}
.btn[disabled] {
  opacity: 0.5;
  cursor: default;
}

.menu {
  display: flex;
  flex-direction: column;
  gap: var(--space-3);
  width: 100%;
  max-width: 360px;
}
.menu__label {
  text-align: center;
  color: var(--color-muted);
  font-size: var(--font-size-sm);
  margin: var(--space-4) 0 var(--space-1);
}
.menu__quickplay {
  display: grid;
  grid-template-columns: repeat(3, 1fr);
  gap: var(--space-2);
}
```

- [ ] **Step 5: Create `src/routes/home.ts`**

```ts
import { html, render } from 'lit-html';
import { appShell } from '../ui/templates';
import { button } from '../ui/components/button';
import type { RouteModule } from '../router';

type TimerCfg = { initial_time_secs: number; increment_secs: number } | null;

const QUICKPLAY_TIMERS: { label: string; value: TimerCfg }[] = [
  { label: '5+3', value: { initial_time_secs: 300, increment_secs: 3 } },
  { label: '10+5', value: { initial_time_secs: 600, increment_secs: 5 } },
  { label: '15+10', value: { initial_time_secs: 900, increment_secs: 10 } },
];

function onSeek(timer: TimerCfg): void {
  // Plan 2 wires this to the matchmaking SSE call.
  console.log('seek quickplay', timer);
}

function onFriends(): void {
  // Plan 2 navigates to a challenge-create view.
  console.log('play with friends');
}

function onComputers(): void {
  // Plan 2 wires this to POST /games with num_humans=1.
  console.log('play with computers');
}

function template() {
  return appShell(html`
    <h1>Spades</h1>
    <div class="menu" data-testid="home-menu">
      <p class="menu__label">Quick Play</p>
      <div class="menu__quickplay">
        ${QUICKPLAY_TIMERS.map((t) =>
          button({
            label: t.label,
            onClick: () => onSeek(t.value),
            variant: 'primary',
          }),
        )}
      </div>
      ${button({ label: 'Play with Friends', onClick: onFriends, variant: 'secondary' })}
      ${button({ label: 'Play with Computers', onClick: onComputers, variant: 'secondary' })}
    </div>
  `);
}

export const home: RouteModule = {
  render: () => {
    const root = document.getElementById('root');
    if (!root) return () => {};
    render(template(), root);
    return () => {
      // No subscriptions to dispose yet — render nothing on cleanup.
      render(html``, root);
    };
  },
};
```

- [ ] **Step 6: Create `src/routes/notfound.ts`**

```ts
import { html, render } from 'lit-html';
import { appShell } from '../ui/templates';
import type { RouteModule } from '../router';

export const notFound: RouteModule = {
  render: () => {
    const root = document.getElementById('root');
    if (!root) return () => {};
    render(
      appShell(html`
        <h1>Not found</h1>
        <p><a href="/" data-link>Back home</a></p>
      `),
      root,
    );
    return () => render(html``, root);
  },
};
```

- [ ] **Step 7: Rewrite `src/main.ts` to mount the router**

```ts
import './ui/design.css';
import { createRouter } from './router';
import { home } from './routes/home';
import { notFound } from './routes/notfound';

const router = createRouter({
  '/': home,
  '*': notFound,
});

// Delegate <a data-link> clicks to client-side navigation.
document.addEventListener('click', (e) => {
  const target = (e.target as HTMLElement).closest('a[data-link]') as HTMLAnchorElement | null;
  if (!target) return;
  if (target.target === '_blank' || e.metaKey || e.ctrlKey) return;
  const url = new URL(target.href);
  if (url.origin !== location.origin) return;
  e.preventDefault();
  history.pushState(null, '', url.pathname + url.search);
  router.handle(url.pathname + url.search);
});

window.addEventListener('popstate', () => {
  router.handle(location.pathname + location.search);
});

router.listen();
```

- [ ] **Step 8: Run dev server and verify visually**

Run: `pnpm dev` (background it, e.g. `pnpm dev &`)
Then: `curl -s http://localhost:5173/ | grep -c "Spades"`
Expected: at least 2 matches (title + h1). Stop the dev server (`fg` then Ctrl-C, or `kill %1`).

- [ ] **Step 9: Run lint and type-check**

Run: `pnpm lint && pnpm tsc --noEmit -p tsconfig.json`
Expected: both succeed.

- [ ] **Step 10: Commit**

```bash
git add src/ui/ src/routes/ src/main.ts
git commit -m "feat: render home menu via lit-html + router"
```

---

## Task 6: Component test for Home

**Files:**

- Create: `tests/component/home.spec.ts`

- [ ] **Step 1: Write the failing test**

```ts
import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { home } from '../../src/routes/home';

describe('home route', () => {
  let logSpy: ReturnType<typeof vi.spyOn>;

  beforeEach(() => {
    document.body.innerHTML = '<main id="root"></main>';
    logSpy = vi.spyOn(console, 'log').mockImplementation(() => {});
  });

  afterEach(() => {
    logSpy.mockRestore();
  });

  it('renders the menu with five action buttons', () => {
    const cleanup = home.render({}, { path: '/', search: new URLSearchParams() });
    const menu = document.querySelector('[data-testid="home-menu"]');
    expect(menu).not.toBeNull();
    const buttons = menu!.querySelectorAll('button');
    expect(buttons.length).toBe(5);
    expect(Array.from(buttons).map((b) => b.textContent?.trim())).toEqual([
      '5+3',
      '10+5',
      '15+10',
      'Play with Friends',
      'Play with Computers',
    ]);
    cleanup();
  });

  it('logs seek payload for quickplay 10+5', () => {
    const cleanup = home.render({}, { path: '/', search: new URLSearchParams() });
    const buttons = document.querySelectorAll('[data-testid="home-menu"] button');
    (buttons[1] as HTMLButtonElement).click();
    expect(logSpy).toHaveBeenCalledWith('seek quickplay', {
      initial_time_secs: 600,
      increment_secs: 5,
    });
    cleanup();
  });

  it('cleanup empties the root', () => {
    const cleanup = home.render({}, { path: '/', search: new URLSearchParams() });
    expect(document.getElementById('root')!.childNodes.length).toBeGreaterThan(0);
    cleanup();
    expect(document.getElementById('root')!.textContent?.trim()).toBe('');
  });
});
```

- [ ] **Step 2: Run test to verify it passes**

Run: `pnpm test:component`
Expected: all 3 cases pass (the sanity test + the 3 new home tests).

- [ ] **Step 3: Commit**

```bash
git add tests/component/home.spec.ts
git commit -m "test: component test for home menu"
```

---

## Task 7: Rewrite the Playwright smoke test for the real Home view

**Files:**

- Modify: `tests/e2e/smoke.spec.ts`

- [ ] **Step 1: Replace `tests/e2e/smoke.spec.ts`**

```ts
import { test, expect } from '@playwright/test';

test('home renders the menu', async ({ page }) => {
  await page.goto('/');
  await expect(page).toHaveTitle('Spades');
  await expect(page.getByRole('heading', { name: 'Spades' })).toBeVisible();
  await expect(page.locator('[data-testid="home-menu"] button')).toHaveCount(5);
});

test('clicking a quickplay button logs to console', async ({ page }) => {
  const logs: string[] = [];
  page.on('console', (msg) => logs.push(msg.text()));
  await page.goto('/');
  await page.getByRole('button', { name: '5+3' }).click();
  // Console log fires synchronously after click handler.
  await expect.poll(() => logs.some((l) => l.includes('seek quickplay'))).toBe(true);
});

test('unknown route renders 404', async ({ page }) => {
  await page.goto('/no-such-path');
  await expect(page.getByRole('heading', { name: 'Not found' })).toBeVisible();
});
```

- [ ] **Step 2: Run Playwright**

Run: `pnpm test:e2e`
Expected: 3 tests pass.

- [ ] **Step 3: Commit**

```bash
git add tests/e2e/smoke.spec.ts
git commit -m "test: e2e smoke covers menu + 404"
```

---

## Task 8: README

**Files:**

- Create: `README.md`

- [ ] **Step 1: Write `README.md`**

````markdown
# spades-ts

TypeScript SPA front-end for the [rust-spades](https://github.com/wlim/rust-spades) game server.

## Status

Scaffold only. See `docs/superpowers/specs/2026-05-11-spades-ts-design.md` for the design and `docs/superpowers/plans/` for staged implementation plans.

## Dev

```sh
pnpm install
pnpm dev        # http://localhost:5173
```
````

Requires a running rust-spades server at `VITE_API_URL` (defaults to `http://localhost:3000` in dev).

```sh
cd ../rust-spades
cargo run -p spades-server -- --port 3000 --insecure-cookies \
  --cors-allow-origin http://localhost:5173
```

## Scripts

|                 |                                         |
| --------------- | --------------------------------------- |
| `pnpm dev`      | Vite dev server                         |
| `pnpm build`    | Type-check + production build → `dist/` |
| `pnpm preview`  | Serve the production build locally      |
| `pnpm test`     | Unit + component tests                  |
| `pnpm test:e2e` | Playwright end-to-end tests             |
| `pnpm lint`     | ESLint                                  |
| `pnpm format`   | Prettier write                          |

````

- [ ] **Step 2: Commit**

```bash
git add README.md
git commit -m "docs: add readme"
````

---

## Self-review

**Spec coverage (Phase 1 of the design doc):**

- Vite + TS + lint + format → Task 1 ✓
- Vitest unit + component → Task 2, Task 6 ✓
- Playwright → Task 3, Task 7 ✓
- Design tokens → Task 1 (Step 11), Task 5 (Step 4) ✓
- Router → Task 4 ✓
- Header → Task 5 (Step 2) ✓
- Home route wired to `console.log` → Task 5 (Step 5) ✓
- 404 route → Task 5 (Step 6) ✓
- `<a data-link>` navigation delegate → Task 5 (Step 7) ✓
- README → Task 8 ✓

**Out of scope for this plan, as planned:** `schema.d.ts` generation, `api/client.ts`, signal stores, card layer, game route, auth, OAuth — all in Plans 2 & 3.

**Placeholder scan:** None. Every step has the actual code or command.

**Type consistency:** `RouteModule.render` returns `() => void` everywhere. Used identically in `home.ts`, `notfound.ts`, router test. `RouteContext` shape is `{ path, search }` in `router.ts`; `home.ts` and `notfound.ts` ignore it (params are `{}` for unparam routes). The component test for home passes `{ path: '/', search: new URLSearchParams() }` — matches.

**Open caveats for the reviewer:**

- I assumed `pnpm` as the package manager. If you prefer `npm`, drop `packageManager` from `package.json` and replace `pnpm` with `npm` in scripts/docs.
- `noUncheckedIndexedAccess: true` is strict but recommended — flips the type of `array[i]` to `T | undefined`. Catches a class of bugs at the cost of more `if (x)` guards. Easy to turn off if it bites.
- `eslint v9` uses the flat config by default; I'm using legacy `.eslintrc.cjs` because it's simpler and well-supported. If you want flat config later, easy migration.
