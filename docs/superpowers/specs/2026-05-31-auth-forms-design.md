# Frontend Redesign — Phase 3a: Auth & Forms (Design Spec)

**Date:** 2026-05-31
**Status:** Approved (visual direction validated via `web/auth-preview.html`, light + dark — the auth-card half)
**Phase:** 3a of the redesign. The final "Auth & account" work was split in two; this is the first half. Builds on Phases 0–2 (foundations, table, home), all merged to master.

**Split:** **3a (this spec) = Auth & forms** — login / signup / oauth-complete (auth card + branded OAuth + shared form-field refinement). **3b (later) = Setup & account** — create (segmented controls), lobby (seat-grid / share), profile, settings.

---

## 1. Context & goals

The credential pages (`login.ts`, `signup.ts`, `oauth-complete.ts`) are bare `.form-page` sections: a heading, `formField()`s, a primary button, and (login/signup) two plain-text OAuth buttons + a sign-in/up link. They're functional and on Phase-0 tokens but unstyled beyond that. This sub-phase gives them a polished, branded **auth card** and brings the OAuth buttons onto the icon system.

### Goals
- A shared **auth card**: centered card (surface, border, radius, shadow) with a small ♠ Spades wordmark + title, used by login / signup / oauth-complete.
- **Branded OAuth buttons**: full-width, with Google / GitHub brand icons (vendor `ri:google-fill` + `ri:github-fill` from the Apache-2.0 Remix backup), behind a clean "or" divider — shared by login + signup.
- **Form-field refinement**: the shared `formField` gains an optional inline **error/invalid** state (red border + message); the primary submit goes full-width.
- Light/dark + responsive; all `data-testid`s preserved.

### Non-goals (3b / out of scope)
- create / lobby / profile / settings (Phase 3b).
- Auth **logic** — `session.loginWithPassword`, `session.startOauth`, the signup/oauth-complete flows, error handling, and the `tagSubmit` test hook are unchanged; only presentation changes.
- Game/table/home (done).

---

## 2. Constraints & guardrails
- Stay on lit-html + `@preact/signals-core` + Vite, CSS-only, **no new runtime deps**. The two new OAuth icons are vendored static SVGs (same Apache-2.0 pipeline as Phase 0).
- Preserve a11y + tests: keep `data-testid`s (`form-error`, `submit`, and the `tagSubmit` behavior that sets `type="submit"` + `data-testid="submit"` on `.form-actions .btn--primary`), the `<form @submit>` handlers, input `autocomplete`, and labeled inputs. OAuth brand icons are decorative (`aria-hidden`; the button text names the action). `:focus-visible` honored.

---

## 3. Locked decisions (validated in the preview)

| Area | Decision |
| --- | --- |
| Auth card | Shared centered card (`.auth-card`) + ♠ wordmark + centered title, for login/signup/oauth-complete. |
| OAuth | Branded full-width Google/GitHub buttons (`icon('google-fill')`/`icon('github-fill')`) behind an "or" divider; shared by login + signup. |
| Icons | Vendor `google-fill` + `github-fill` from `cyberalien/RemixIcon` (Apache-2.0) into `web/src/ui/icons/`. |
| Form field | `formField` gains optional `error?: string` → invalid input border + `.field-error` message. |
| Submit | Primary action full-width (`.form-actions .btn` stretches). |

---

## 4. Architecture

Three small lit-html components keep the three routes DRY; the routes shrink to data + handlers.

### 4.1 `ui/components/auth-card.ts` — the shell
`authCard({ title, children })` → 
```
<section class="auth-card">
  <div class="auth-card__brand"><span class="auth-card__pip">♠</span> Spades</div>
  <h2>${title}</h2>
  ${children}
</section>
```
Used by login (`title: 'Sign in'`), signup (`'Create account'`), oauth-complete (`'Choose a username'`). Replaces the bare `<section class="form-page"><h2>…` wrapper.

### 4.2 `ui/components/oauth-buttons.ts` — divider + branded buttons
`oauthButtons({ next })` →
```
<div class="auth-divider">or</div>
<button class="btn btn--secondary btn--block" type="button" @click=${() => session.startOauth('google', next)}>
  ${icon('google-fill')} Continue with Google
</button>
<button class="btn btn--secondary btn--block" type="button" @click=${() => session.startOauth('github', next)}>
  ${icon('github-fill')} Continue with GitHub
</button>
```
Used by login + signup (oauth-complete is already authenticated — no OAuth section). Imports `session` + `icon`.

### 4.3 `ui/components/form-field.ts` — invalid state
Extend the existing `formField` opts with `error?: string`. When present: add an `invalid` class to the field wrapper and render `<span class="field-error">${error}</span>` under the input (with `aria-invalid="true"` + `aria-describedby` wiring on the input). No change to existing call sites that omit `error`.

### 4.4 Vendor the OAuth icons
Fetch `google-fill.svg` + `github-fill.svg` from the Apache-2.0 `cyberalien/RemixIcon` backup (Logos category) into `web/src/ui/icons/` (same method as the Phase-0 plan's vendoring step), svgo-optional. **Verify** they exist in that snapshot; if a brand icon is absent from the Apache backup, fall back to text-only OAuth buttons (no icon) for that provider and note it — do not pull from a non-Apache source.

### 4.5 Routes
- `login.ts` / `signup.ts`: wrap the form in `authCard({ title, children })`; keep the `<form @submit>` + `formField`s + the full-width primary in `.form-actions`; replace the inline OAuth markup with `oauthButtons({ next })`; keep the switch link (`No account? Sign up` / `Have an account? Sign in`). Keep `tagSubmit`.
- `oauth-complete.ts`: wrap its username form in `authCard({ title: 'Choose a username' })`; no OAuth section.

### 4.6 CSS (`design.css`)
- `.auth-card` (centered, `max-width: 24rem`, `surface-raised`, `border`, `radius-lg`, `shadow-2`, padding, `gap`); `.auth-card__brand` (display-font, centered, `.auth-card__pip` accent); `.auth-card h2` centered.
- `.auth-divider` (flex with `::before`/`::after` hairlines + centered "or").
- `.btn--block { width: 100% }`; OAuth buttons get the brand icon via the existing `.icon` sizing.
- `.form-field.invalid input { border-color: var(--danger) }` (the `.field-error` rule already exists from Phase 0).
- Restyle/retire the old `.oauth-divider` and the bare `.form-page` rules as the card replaces them (remove if no longer used; `oauth-complete` uses the card too).

---

## 5. Testing & verification
- **Component:** `login.spec.ts` / `signup.spec.ts` — keep the existing assertions (form-error, submit testid + `type=submit`, navigation on success); add: the auth card renders the title + brand; the OAuth buttons render a brand `.icon svg`; a `formField` with `error` shows `.field-error` + `invalid`. (`oauth-complete` has a spec too — keep it green.)
- **Gate:** `pnpm -C web build && pnpm -C web test && pnpm -C web lint && pnpm -C web format:check` green.
- **e2e:** `web/tests/e2e/auth.spec.ts` (and any login flow) selects by `data-testid` (`submit`, `form-error`) — preserved; verify nothing selects by the old `.form-page`/`.oauth-divider` structure. Run with backend in CI.
- **Visual:** `web/auth-preview.html` is the reference (auth-card half); verify login/signup/oauth-complete in light + dark and at mobile width.

## 6. Risks & open questions
- **Brand-icon availability** in the Apache `cyberalien/RemixIcon` snapshot — verified at implementation; text-only fallback per provider if absent (no non-Apache source).
- **`tagSubmit` hook** depends on `.form-actions .btn--primary` existing — keep the primary button inside `.form-actions` so the hook still finds it.
- **Dead CSS:** remove old `.oauth-divider` / bare `.form-page` rules only once all three routes use the card (no orphans); but note `.form-page` may still be referenced by 3b routes (create) — if so, keep it until 3b. Check usages before removing.

## 7. Deliverables
`auth-card.ts` + `oauth-buttons.ts` components; `formField` invalid state; vendored `google-fill`/`github-fill` icons; login/signup/oauth-complete using the card (+ login/signup using `oauthButtons`); `design.css` auth-card/divider/block/invalid rules; updated login/signup/oauth-complete specs; gate green; light/dark + responsive verified.
