# Auth & Forms (Phase 3a) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Give the credential pages (login / signup / oauth-complete) a polished, branded auth card with branded OAuth buttons and a refined invalid-field state — without touching auth logic.

**Architecture:** Two new small lit-html components — `authCard` (card shell + ♠ wordmark + title) and `oauthButtons` (divider + Google/GitHub branded buttons) — plus an invalid-state tweak to the shared `formField`. The three routes swap their bare `.form-page` wrapper for `authCard`; login + signup add `oauthButtons`. Two brand icons are vendored into the Phase-0 Remix pipeline. `.form-page` stays (Phase 3b's create still uses it).

**Tech Stack:** TypeScript, Vite, lit-html, vitest (component=happy-dom). Icons vendored (Apache-2.0 `cyberalien/RemixIcon`); fonts/tokens in place.

**Reference:** Spec `docs/superpowers/specs/2026-05-31-auth-forms-design.md`. Visual reference: `web/auth-preview.html` (throwaway, auth-card half). On branch `auth-forms`.

**TDD note:** Components + the form-field tweak are test-first. Route wiring is **[VERIFY]** (existing specs + gate). Run all commands from the repo root.

---

### Task 1: Vendor the Google + GitHub brand icons **[VERIFY]**

**Files:** Create `web/src/ui/icons/google-fill.svg`, `web/src/ui/icons/github-fill.svg`

- [ ] **Step 1: Fetch from the Apache-2.0 backup** (run from repo root):

```bash
python3 - <<'PY'
import json, subprocess, base64
names = ["google-fill", "github-fill"]
tree = json.loads(subprocess.check_output(
    ["gh", "api", "repos/cyberalien/RemixIcon/git/trees/master?recursive=1"]))
by = {}
for t in tree["tree"]:
    if t["type"] == "blob" and t["path"].lower().endswith(".svg"):
        by.setdefault(t["path"].split("/")[-1][:-4].lower(), t["url"])
miss = []
for n in names:
    u = by.get(n.lower())
    if not u:
        miss.append(n); continue
    blob = json.loads(subprocess.check_output(["gh", "api", u]))
    open(f"web/src/ui/icons/{n}.svg", "wb").write(base64.b64decode(blob["content"]))
print("MISSING:", miss or "none")
PY
ls web/src/ui/icons/google-fill.svg web/src/ui/icons/github-fill.svg 2>&1
```

Expected: `MISSING: none` and both files listed. **If one is MISSING** (not in the Apache snapshot), proceed without it — the `icon()` helper returns `nothing` for an unknown name, so `oauthButtons` will gracefully render that provider's button text-only. Report which (if any) was missing.

- [ ] **Step 2: Commit**

```bash
git add web/src/ui/icons
git commit -m "feat(web): vendor Google + GitHub brand icons (Apache-2.0 Remix)"
```

---

### Task 2: `formField` invalid state **[TDD]**

**Files:** Modify `web/src/ui/components/form-field.ts`; Modify `web/src/ui/design.css`; Test `web/tests/component/form-field.spec.ts`

- [ ] **Step 1: Write the failing test** — `web/tests/component/form-field.spec.ts` already exists; append:

```ts
import { describe, it, expect, beforeEach } from 'vitest';
import { render } from 'lit-html';
import { formField } from '../../src/ui/components/form-field';

describe('formField invalid state', () => {
  beforeEach(() => {
    document.body.innerHTML = '<main id="root"></main>';
  });
  it('marks the field invalid + wires aria when error is present', () => {
    render(
      formField({ id: 'email', label: 'Email', value: '', onInput: () => {}, error: 'Required.' }),
      document.getElementById('root')!,
    );
    const wrap = document.querySelector('.form-field')!;
    expect(wrap.classList.contains('invalid')).toBe(true);
    const input = wrap.querySelector('input')!;
    expect(input.getAttribute('aria-invalid')).toBe('true');
    expect(input.getAttribute('aria-describedby')).toBe('email-error');
    expect(document.querySelector('#email-error.field-error')?.textContent).toBe('Required.');
  });
  it('is not invalid without an error', () => {
    render(
      formField({ id: 'x', label: 'X', value: '', onInput: () => {} }),
      document.getElementById('root')!,
    );
    expect(document.querySelector('.form-field')!.classList.contains('invalid')).toBe(false);
    expect(document.querySelector('input')!.hasAttribute('aria-invalid')).toBe(false);
  });
});
```

(Check the file's existing imports — if `render`/`formField` are already imported at top, don't duplicate; put the new `describe` blocks alongside the existing ones.)

- [ ] **Step 2: Run, verify FAIL** — `pnpm -C web test:component -- form-field`.

- [ ] **Step 3: Implement** — rewrite `web/src/ui/components/form-field.ts`'s `formField` to add the invalid class + aria wiring (keep the `FormFieldOpts` type as-is — it already has `error?`):

```ts
import { html, nothing, type TemplateResult } from 'lit-html';

// (keep the existing FormFieldOpts type unchanged)

export function formField(opts: FormFieldOpts): TemplateResult {
  const hasError = !!opts.error;
  const errId = `${opts.id}-error`;
  return html`<div class="form-field${hasError ? ' invalid' : ''}">
    <label for=${opts.id}>${opts.label}</label>
    <input
      id=${opts.id}
      name=${opts.id}
      type=${opts.type ?? 'text'}
      .value=${opts.value}
      placeholder=${opts.placeholder ?? ''}
      autocomplete=${opts.autocomplete ?? 'off'}
      maxlength=${opts.maxLength ?? 200}
      ?disabled=${opts.disabled ?? false}
      aria-invalid=${hasError ? 'true' : nothing}
      aria-describedby=${hasError ? errId : nothing}
      @input=${opts.onInput}
    />
    ${opts.error
      ? html`<span id=${errId} data-testid="field-error" class="field-error">${opts.error}</span>`
      : null}
  </div>`;
}
```

- [ ] **Step 4: Add the invalid border CSS** — in `web/src/ui/design.css`, after the `.form-field input` rule add:

```css
.form-field.invalid input {
  border-color: var(--danger);
}
```

- [ ] **Step 5: Run, verify PASS** — `pnpm -C web test:component -- form-field`. Then `pnpm -C web build && pnpm -C web lint`.

- [ ] **Step 6: Commit**

```bash
git add web/src/ui/components/form-field.ts web/src/ui/design.css web/tests/component/form-field.spec.ts
git commit -m "feat(web): formField invalid state (red border + aria)"
```

---

### Task 3: `authCard` + `oauthButtons` components **[TDD]**

**Files:** Create `web/src/ui/components/auth-card.ts`, `web/src/ui/components/oauth-buttons.ts`; Modify `web/src/ui/design.css`; Test `web/tests/component/auth-components.spec.ts`

- [ ] **Step 1: Write the failing test** — create `web/tests/component/auth-components.spec.ts`:

```ts
import { describe, it, expect, beforeEach } from 'vitest';
import { html, render } from 'lit-html';
import { authCard } from '../../src/ui/components/auth-card';
import { oauthButtons } from '../../src/ui/components/oauth-buttons';

describe('authCard', () => {
  beforeEach(() => {
    document.body.innerHTML = '<main id="root"></main>';
  });
  it('renders the brand, title, and children', () => {
    render(authCard({ title: 'Sign in', children: html`<p data-testid="kid">x</p>` }), document.getElementById('root')!);
    const card = document.querySelector('.auth-card')!;
    expect(card).not.toBeNull();
    expect(card.querySelector('.auth-card__brand')?.textContent).toContain('Spades');
    expect(card.querySelector('h2')?.textContent).toBe('Sign in');
    expect(card.querySelector('[data-testid=kid]')).not.toBeNull();
  });
});

describe('oauthButtons', () => {
  beforeEach(() => {
    document.body.innerHTML = '<main id="root"></main>';
  });
  it('renders a divider + Google + GitHub buttons', () => {
    render(oauthButtons({ next: '/' }), document.getElementById('root')!);
    expect(document.querySelector('.auth-divider')).not.toBeNull();
    const btns = document.querySelectorAll('button.btn--secondary');
    expect(btns.length).toBe(2);
    expect(btns[0]!.textContent).toContain('Google');
    expect(btns[1]!.textContent).toContain('GitHub');
  });
});
```

- [ ] **Step 2: Run, verify FAIL** — `pnpm -C web test:component -- auth-components`.

- [ ] **Step 3: Implement `auth-card.ts`**:

```ts
import { html, type TemplateResult } from 'lit-html';

export function authCard(opts: { title: string; children: TemplateResult }): TemplateResult {
  return html`<section class="auth-card" data-testid="auth-card">
    <div class="auth-card__brand"><span class="auth-card__pip">♠</span> Spades</div>
    <h2>${opts.title}</h2>
    ${opts.children}
  </section>`;
}
```

- [ ] **Step 4: Implement `oauth-buttons.ts`**:

```ts
import { html, type TemplateResult } from 'lit-html';
import { session } from '../../state/session';
import { icon } from '../icon';

export function oauthButtons(opts: { next: string }): TemplateResult {
  return html`<div class="auth-divider">or</div>
    <button
      class="btn btn--secondary btn--block"
      type="button"
      @click=${() => session.startOauth('google', opts.next)}
    >
      ${icon('google-fill')} Continue with Google
    </button>
    <button
      class="btn btn--secondary btn--block"
      type="button"
      @click=${() => session.startOauth('github', opts.next)}
    >
      ${icon('github-fill')} Continue with GitHub
    </button>`;
}
```

(Confirm `session.startOauth(provider, next)` exists in `web/src/state/session.ts` — login.ts already calls it that way.)

- [ ] **Step 5: Run, verify PASS** — `pnpm -C web test:component -- auth-components`.

- [ ] **Step 6: CSS** — append to `web/src/ui/design.css`:

```css
.auth-card {
  display: flex;
  flex-direction: column;
  gap: var(--space-3);
  width: 100%;
  max-width: 24rem;
  padding: var(--space-6);
  background: var(--surface-raised);
  border: 1px solid var(--border);
  border-radius: var(--radius-lg);
  box-shadow: var(--shadow-2);
}
.auth-card__brand {
  display: flex;
  align-items: center;
  justify-content: center;
  gap: var(--space-2);
  font-family: var(--font-display);
  font-weight: 600;
}
.auth-card__pip {
  color: var(--accent);
}
.auth-card h2 {
  text-align: center;
  margin: 0;
}
.auth-divider {
  display: flex;
  align-items: center;
  gap: var(--space-3);
  color: var(--fg-subtle);
  font-size: var(--text-sm);
}
.auth-divider::before,
.auth-divider::after {
  content: '';
  flex: 1;
  height: 1px;
  background: var(--border);
}
.btn--block {
  width: 100%;
}
```

- [ ] **Step 7: Verify gate** — `pnpm -C web build && pnpm -C web test && pnpm -C web lint && pnpm -C web format:check`. Green.

- [ ] **Step 8: Commit**

```bash
git add web/src/ui/components/auth-card.ts web/src/ui/components/oauth-buttons.ts web/src/ui/design.css web/tests/component/auth-components.spec.ts
git commit -m "feat(web): authCard + oauthButtons components"
```

---

### Task 4: Wire login / signup / oauth-complete to the card **[VERIFY]**

**Files:** Modify `web/src/routes/login.ts`, `web/src/routes/signup.ts`, `web/src/routes/oauth-complete.ts`, `web/src/ui/design.css`; Test: the three route specs

- [ ] **Step 1: `login.ts`** — replace the template's `appShell(html\`<section class="form-page"><h2>Sign in</h2> … </section>\`)` with `authCard`, moving the OAuth block into `oauthButtons`. Add imports `import { authCard } from '../ui/components/auth-card';` and `import { oauthButtons } from '../ui/components/oauth-buttons';`. New template body:

```ts
    const template = () =>
      appShell(
        authCard({
          title: 'Sign in',
          children: html`
            ${error.value
              ? html`<p data-testid="form-error" class="field-error">${error.value}</p>`
              : nothing}
            <form
              @submit=${(e: Event) => {
                e.preventDefault();
                void onSubmit();
              }}
            >
              ${formField({ id: 'email', label: 'Email', type: 'email', value: email.value, autocomplete: 'email', onInput: (e) => { email.value = (e.target as HTMLInputElement).value; } })}
              ${formField({ id: 'password', label: 'Password', type: 'password', value: password.value, autocomplete: 'current-password', onInput: (e) => { password.value = (e.target as HTMLInputElement).value; } })}
              <div class="form-actions">
                ${button({ label: submitting.value ? 'Signing in…' : 'Sign in', onClick: () => {}, variant: 'primary', disabled: submitting.value })}
              </div>
            </form>
            ${oauthButtons({ next })}
            <p class="switch">No account? <a href="/signup" data-link>Sign up</a></p>
          `,
        }),
      );
```

Keep everything else (signals, `onSubmit`, `tagSubmit` querying `.form-actions .btn--primary`, dispose). The primary stays inside `.form-actions` so `tagSubmit` + the `flex:1` full-width still apply.

- [ ] **Step 2: `signup.ts`** — same treatment: wrap in `authCard({ title: 'Sign up', children: … })`, keep the three fields + `.form-actions`, and ADD `${oauthButtons({ next: '/' })}` after the `</form>` and before the switch link `<p class="switch">Have an account? <a href="/login" data-link>Sign in</a></p>`. Add the `authCard` + `oauthButtons` imports. Keep `validate`/`onSubmit`/`tagSubmit`.

- [ ] **Step 3: `oauth-complete.ts`** — wrap in `authCard({ title: 'Choose a username', children: html\`<p>You're almost in. Pick a public username to finish creating your account.</p> ${error…} <form>…username field… <div class="form-actions">…Continue…</div></form>\` })`. Add the `authCard` import. NO `oauthButtons` here (already authenticated). Keep `onSubmit`/`tagSubmit`.

- [ ] **Step 4: Remove the dead `.oauth-divider` rule** — in `web/src/ui/design.css`, delete the `.oauth-divider { … }` rule (login no longer uses it; `.auth-divider` replaces it). Confirm no other usage: `grep -rn 'oauth-divider' web/src` → none.

- [ ] **Step 5: Update the route specs** — in `web/tests/component/login.spec.ts`, `signup.spec.ts`, and `oauth-complete.spec.ts` (whichever exist): keep all existing assertions (they target `data-testid`s `form-error`/`submit` and behavior — those are preserved). Where a spec asserted the page structure via `.form-page`, update the selector to `.auth-card`. Add to login + signup specs: `expect(document.querySelector('.auth-card h2')).not.toBeNull()` and `expect(document.querySelectorAll('button.btn--secondary').length).toBe(2)` (the OAuth buttons). Run each (`pnpm -C web test:component -- login`, `-- signup`, `-- oauth`) and fix selectors until green.

- [ ] **Step 6: Verify gate** — `pnpm -C web build && pnpm -C web test && pnpm -C web lint && pnpm -C web format:check`. Green.

- [ ] **Step 7: Commit**

```bash
git add web/src/routes/login.ts web/src/routes/signup.ts web/src/routes/oauth-complete.ts web/src/ui/design.css web/tests/component/login.spec.ts web/tests/component/signup.spec.ts web/tests/component/oauth-complete.spec.ts
git commit -m "feat(web): auth card on login/signup/oauth-complete + OAuth on signup"
```

---

### Task 5: Final verify & cleanup **[VERIFY]**

**Files:** Remove `web/auth-preview.html` (throwaway)

- [ ] **Step 1: Full gate** — `pnpm -C web build && pnpm -C web test && pnpm -C web lint && pnpm -C web format:check`. All green.

- [ ] **Step 2: Confirm no dead/missing refs**:

```bash
grep -rn 'oauth-divider' web/src || echo "oauth-divider gone (good)"
grep -rn 'class="form-page"' web/src/routes/login.ts web/src/routes/signup.ts web/src/routes/oauth-complete.ts || echo "auth routes off .form-page (good)"
grep -rn 'class="form-page"' web/src/routes/create.ts && echo "(.form-page still used by create — keep the rule)"
```
Expected: `.oauth-divider` gone; the three auth routes no longer use `.form-page`; `create.ts` still does (so the `.form-page` rule stays).

- [ ] **Step 3: e2e selector check** — `grep -rn "form-page\|oauth-divider\|getByTestId\|data-testid" web/tests/e2e | grep -iE 'login|auth|signup|form|oauth' | head`. Confirm the e2e auth flow selects by `data-testid` (`submit`/`form-error`) — preserved. Run `pnpm -C web test:e2e` with the Rust backend in CI.

- [ ] **Step 4: Remove the throwaway preview** — `rm -f web/auth-preview.html`

- [ ] **Step 5: Commit**

```bash
git add -A web/
git commit -m "chore(web): remove throwaway auth preview"
```

---

## Self-review

**Spec coverage:**
- §3/§4.4 vendor `google-fill`/`github-fill` (Apache backup, text-only fallback) → Task 1.
- §4.3 `formField` invalid state → Task 2.
- §4.1 `authCard` + §4.2 `oauthButtons` components → Task 3.
- §4.5 routes use the card (login/signup/oauth-complete; OAuth on login + signup) → Task 4.
- §4.6 CSS (`.auth-card`/`__brand`/`__pip`, `.auth-divider`, `.btn--block`, `.form-field.invalid`) → Tasks 2 (invalid) + 3 (card/divider/block); `.oauth-divider` removed in Task 4; `.form-page` kept (create/3b) — verified in Task 5.
- §5 tests (form-field invalid, authCard/oauthButtons render, route specs keep testids + add card/OAuth assertions; e2e by testid) → Tasks 2–5.

**Placeholder scan:** No TBD/"handle errors"/"similar to" — full component + test + CSS code given; the one conditional (icon MISSING → text-only) is graceful via `icon()` returning `nothing`, and explicitly handled.

**Type consistency:** `authCard({ title, children: TemplateResult })` and `oauthButtons({ next: string })` signatures are consistent across their definition (Task 3), tests (Task 3), and call sites (Task 4). `formField`'s `FormFieldOpts` is unchanged (already had `error?`); the implementation reads `opts.error`/`opts.id` consistently. `session.startOauth(provider, next)` matches login.ts's existing usage.

**Note for executor:** keep each route's `tagSubmit` hook and the primary button inside `.form-actions` (the hook queries `.form-actions .btn--primary`); the auth card wraps the form but the `.form-actions` block stays. Do NOT remove the `.form-page` CSS rule — `create.ts` (Phase 3b) still uses it.
