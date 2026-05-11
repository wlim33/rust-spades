# Identity Foundation — Design Spec

**Date:** 2026-05-11
**Slice:** First slice of "port Lila patterns into rust-spades."
**Status:** Design approved by user. Awaiting written-spec review before plan.

---

## Context

`rust-spades` v2.0.0 is a working Spades game server: `spades-core` (game engine library) + `spades-server` (Axum REST + WebSocket + SSE + optional SQLite, with seek queue, lobbies, challenge links, Fischer timers, name validation, random AI). 238 tests pass. The server is single-binary, single-box, currently deployable to a Linux VM via the gitignored deploy scripts.

**Existing identity infrastructure (discovered during planning, not previously documented):** the server already uses `tower-sessions` + `tower-sessions-sqlx-store` (SQLite-backed sessions, 30-day inactivity expiry, wired in `bin/server/main.rs:150-162`). It exposes:

- `UserSession { user_id: Uuid, display_name: Option<String> }` as the session payload (`bin/server/dto.rs:11-15`).
- `GET /player` — mints a fresh `user_id` into the session on first call and returns it.
- `PUT /player/name` — validates and stores a display name.

Crucially, **game/lobby/challenge handlers do not consult the session**; they take `name` in request bodies and ignore the session-stored `user_id`. So the in-place infrastructure provides per-browser anonymous identity (cookie-lifetime stable) but doesn't propagate into the game flow. This design leverages that infrastructure rather than building a parallel cookie/session system, and routes the existing `user_id` into game seats.

The product goal driving this design is **"ship a public Spades site"** — i.e., get rust-spades into a state where strangers on the internet can register, play, and come back. Lila (`github.com/lichess-org/lila`) is the reference architecture; this spec ports Lila's identity layer (the `user`, `oauth`, `security`, `pref` cluster) into rust-spades.

Out-of-scope for this slice but anticipated as future slices:
- Ratings (Glicko-2 adapted for partnerships)
- Spectator/TV
- Tournaments / Swiss
- Real-time topology split (lila-ws style with Redis pub/sub)
- Chat, DMs, follow/block, notifications
- Mod tools, anti-collusion, ban-for-abandonment
- Account session management UI, audit trail
- Personal API tokens / "be an OAuth provider"

---

## Goal

Add user accounts to rust-spades such that:

1. Anyone can play as a guest with zero signup (current flow preserved).
2. Guests can register at any point and keep their recent games.
3. Registered users can log in via email/password or OAuth (Google, GitHub).
4. Public profile pages exist at `/users/:username` and list a user's games.
5. The existing game/lobby/challenge flow continues to work; identity *enriches* seats without replacing the bearer `player_id` model.
6. `spades-core` is untouched — identity is a server concern, not a game-rules concern.

---

## Constraints and decisions

These were settled in brainstorming and are *not* open for re-litigation at plan-writing time:

| Axis | Decision |
|---|---|
| Anonymous vs registered | Anon-first, claimable. The existing `tower-sessions` session blob carries the anon `user_id`; on register/login, a `claimed_by` field is set in the same blob, and the anon's existing game seats are reattributed to the new registered user. |
| Auth methods | Email/password and OAuth (Google, GitHub). No magic links. No OAuth-provider role. |
| Datastore | SQLite (becomes mandatory; was optional). No Postgres, no Redis in this slice. |
| Username model | Lila-style: immutable, case-insensitive, ASCII `[a-zA-Z0-9_-]`, length 2-20. |
| Email | Pluggable `Mailer` trait. Default `SmtpMailer` via `lettre`, configured via `SMTP_*` env vars. `LogMailer` for dev/CI. |
| Rate limiting | Auth endpoints only, in-process token bucket via `governor`, plus per-account lockout. No global tower-governor in this slice. |
| Architectural shape | New `auth/` module inside `spades-server`. Not a new crate. |
| Sessions | **Reuse `tower-sessions` (already integrated).** The session cookie is the transport for both anon and registered identity. Per-user session invalidation uses a `token_version` counter on `users` (compared inside the auth extractor) — no separate `sessions` table. |
| Login identifier | Either email or username. `login` field; `'@'` discriminates. |

---

## Architecture

### Module layout

```
crates/spades-server/src/
├── auth/                       ← NEW module
│   ├── mod.rs                  ← AuthState, pub re-exports, Identity / AuthUser extractors
│   ├── users.rs                ← User struct, repo (CRUD), username rules, token_version
│   ├── session_ext.rs          ← UserSession helpers: get/set claimed_by, ensure_anon_id, token_version checks
│   ├── oauth.rs                ← Google + GitHub flow, state CSRF, PKCE, pending-signup store
│   ├── password.rs             ← argon2id hash/verify, weak-password reject list
│   ├── mailer.rs               ← Mailer trait + SmtpMailer + LogMailer
│   ├── rate_limit.rs           ← Per-endpoint token buckets, per-account lockout
│   ├── tokens.rs               ← auth_tokens repo (verify_email, password_reset)
│   ├── game_seats.rs           ← game_seats repo (insert on create, update on join, claim, list-by-user)
│   └── error.rs                ← AuthError → HTTP response
├── sqlite_store.rs             ← MODIFIED: new tables (see Data model)
├── game_manager.rs             ← unchanged
├── matchmaking.rs              ← unchanged shape; handlers pass Identity in
├── challenges.rs               ← same
├── validation.rs               ← unchanged
└── bin/server/
    ├── main.rs                 ← MODIFIED: build AuthState, mount /auth/*, /users/*, configurable Secure cookies
    ├── handlers/
    │   ├── auth.rs             ← NEW: register, login, logout, me, password-reset/*, verify-email
    │   ├── oauth.rs            ← NEW: /auth/oauth/:provider/{login,callback}, /auth/oauth/complete
    │   ├── users.rs            ← NEW: GET /users/:username, GET /users/:username/games, PATCH /users/me
    │   ├── games.rs            ← MODIFIED: Identity extractor, write to game_seats
    │   ├── matchmaking.rs      ← MODIFIED: same
    │   ├── challenges.rs       ← MODIFIED: same
    │   └── players.rs          ← MODIFIED: rename rejects if seat is user-bound
    └── dto.rs                  ← MODIFIED: auth request/response DTOs; `UserSession` grows `claimed_by: Option<Uuid>` and `token_version: i32`
```

Key change vs naive port: there is no `auth/sessions.rs` and no `auth/anon.rs` — both responsibilities are handled by `tower-sessions` (cookie + session blob transport) plus `auth/session_ext.rs` (typed helpers over the existing `UserSession` blob).

### Extractors

`auth::mod` exports two Axum extractors built on top of `tower_sessions::Session`:

- **`Identity`** — `enum Identity { Registered { user: User, anon_id: Uuid }, Anonymous { anon_id: Uuid } }`. Never fails. On extraction:
  1. Reads `UserSession` from the session; if absent, mints one with a fresh `user_id` and writes it back.
  2. If `UserSession.claimed_by` is `Some(uid)`, looks up `users(id = uid)`. Compares stored `users.token_version` with `UserSession.token_version`; mismatch (or missing user) → drops `claimed_by` and treats as anonymous.
  3. Returns either `Registered { user, anon_id }` or `Anonymous { anon_id }`.
- **`AuthUser(User)`** — wraps `Identity`; returns 401 if the variant is `Anonymous`.

Use `Identity` on game/lobby/challenge endpoints. Use `AuthUser` on registered-only endpoints (`/auth/me`, `PATCH /users/me`, etc.).

The "anon_id" is whatever `UserSession.user_id` currently holds. It's stable for the cookie's lifetime (30-day inactivity sliding), and becomes invalid only when tower-sessions garbage-collects the session.

### Dependencies added to `spades-server/Cargo.toml`

- `argon2` — password hashing
- `oauth2` — OAuth client (Google + GitHub)
- `lettre` — SMTP mailer (default backend behind the `Mailer` trait)
- `governor` — token-bucket rate limit
- `sha2` — for hashing email/password tokens before storage
- `reqwest` — OAuth provider userinfo fetch (Google /v1/userinfo, GitHub /user, /user/emails)

Already in tree (no change required):
- `tower-sessions` 0.14, `tower-sessions-sqlx-store` 0.15 (SQLite-backed) — sessions transport
- `time` 0.3 — timestamps
- `rand` 0.8, `uuid`, `serde`, `serde_json`, `rusqlite`

### Untouched surfaces

- `spades-core` — no changes. The library remains identity-free.
- `Game` JSON shape persisted in `games.data` — no changes. Identity lives in a sibling `game_seats` table.
- `GameManager` public surface — unchanged.
- `GameTransition` semantics, including bearer-`player_id` auth for `POST /games/:id/transition` — unchanged.

---

## Data model

All new tables. SQLite, WAL mode (already required).

**Tower-sessions already owns its own tables** (`tower_sessions` migration creates a `tower_sessions` table for blob storage on startup, `main.rs:158`). No changes there — that's the transport. The tables below are all new, added by our own migration code in `sqlite_store.rs`.

```sql
-- Registered users
CREATE TABLE users (
    id              TEXT PRIMARY KEY,              -- UUID v4
    username        TEXT NOT NULL,                 -- display form ("Alice")
    username_canon  TEXT NOT NULL UNIQUE,          -- lowercase ("alice"), for lookup
    email           TEXT NOT NULL UNIQUE,
    email_verified  INTEGER NOT NULL DEFAULT 0,    -- bool
    password_hash   TEXT,                          -- argon2id PHC; NULL = OAuth-only account
    token_version   INTEGER NOT NULL DEFAULT 0,    -- bumped to invalidate all live sessions
    created_at      TEXT NOT NULL DEFAULT (datetime('now')),
    last_login_at   TEXT
);
CREATE INDEX users_username_canon ON users(username_canon);

-- OAuth provider accounts
CREATE TABLE oauth_accounts (
    provider        TEXT NOT NULL,                 -- 'google' | 'github'
    provider_uid    TEXT NOT NULL,                 -- provider's stable user id
    user_id         TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    email           TEXT NOT NULL,
    created_at      TEXT NOT NULL DEFAULT (datetime('now')),
    PRIMARY KEY (provider, provider_uid)
);
CREATE INDEX oauth_accounts_user_id ON oauth_accounts(user_id);

-- Single-use tokens (email verify, password reset)
CREATE TABLE auth_tokens (
    token_hash      TEXT PRIMARY KEY,              -- SHA-256 of random token sent in email
    user_id         TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    purpose         TEXT NOT NULL,                 -- 'verify_email' | 'password_reset'
    created_at      TEXT NOT NULL DEFAULT (datetime('now')),
    expires_at      TEXT NOT NULL,                 -- 24h verify, 1h reset
    used_at         TEXT
);

-- Failed-login tracking (per-account lockout)
CREATE TABLE login_failures (
    user_id         TEXT PRIMARY KEY REFERENCES users(id) ON DELETE CASCADE,
    failure_count   INTEGER NOT NULL DEFAULT 0,
    locked_until    TEXT
);

-- Seat-to-identity binding for past and live games
CREATE TABLE game_seats (
    game_id         TEXT NOT NULL,                 -- not FK; see note below
    seat_index      INTEGER NOT NULL,              -- 0..3 for A,B,C,D
    player_id       TEXT NOT NULL,                 -- the per-game seat UUID returned at create time
    user_id         TEXT REFERENCES users(id) ON DELETE SET NULL,  -- registered owner (if any)
    anon_user_id    TEXT,                          -- the UserSession.user_id of the seat's anon owner
    is_bot          INTEGER NOT NULL DEFAULT 0,
    created_at      TEXT NOT NULL DEFAULT (datetime('now')),
    PRIMARY KEY (game_id, seat_index)
);
CREATE INDEX game_seats_user_id ON game_seats(user_id);
CREATE INDEX game_seats_anon_user_id ON game_seats(anon_user_id);
```

### What is *not* in this list (vs the original draft)

- ❌ `sessions` table — tower-sessions owns the session-blob store; per-user invalidation is done via the `token_version` counter on `users`.
- ❌ `anon_sessions` table — the anonymous "user_id" is whatever `UserSession.user_id` holds in the session blob. It has no separate persistence; if the session expires, the anon identity is gone (and any unclaimed game seats become orphaned, which is acceptable).

### Key data-model choices

- **`username_canon` as a separate column**, not `LOWER(username) UNIQUE`. Explicit, portable, easy to keep consistent in code.
- **`game_seats` is a separate table from `games`.** Keeps `spades-core::Game` JSON identity-free and avoids a destructive migration of existing rows. The `games` table is left untouched.
- **`game_seats.game_id` is intentionally not a foreign key** to `games.id`. Games live in-memory in `GameManager` between transitions and are persisted by `SqliteStore::update_game` after each state change; there's a small window where a game exists in memory before its first persist. `game_seats` rows are written eagerly on seat-assignment (game create, lobby join, challenge join), independently of whether the `games` row has hit disk yet. No FK keeps this decoupled.
- **`game_seats.anon_user_id` is a TEXT, not a FK.** It references `UserSession.user_id`, which has no durable storage outside the session blob — there's nothing to FK to. The value is mostly used for "claim my anon games" on register/login (`UPDATE game_seats SET user_id = ? WHERE anon_user_id = ? AND user_id IS NULL`).
- **`users.token_version`** is the global per-user "invalidate everything" lever. Increment it on password change, password reset, or future "log out everywhere" feature. The `Identity` extractor compares it against the value stored in the session blob and drops `claimed_by` on mismatch.
- **Per-account lockout is per-account, not per-(account, IP).** Simpler and harder for a griefer to abuse to lock a victim's account from a third IP.

---

## Data flows

### Registration (email/password)

```
POST /auth/register
body: { username, email, password }

The handler takes the tower_sessions::Session extractor so it can read the
current anon user_id and write claimed_by back.

1. Validate: username 2-20 ASCII [a-zA-Z0-9_-], reserved-name reject list,
   email parseable (basic syntax + MX optional), password >= 8 chars, not in
   embedded top-10k weak list.
2. Check users.username_canon and users.email for uniqueness.
3. argon2id(password, m=64MB, t=3, p=4) → password_hash.
4. INSERT users (email_verified=0, token_version=0).
5. Read UserSession from the session (ensure_anon_id mints one if absent).
   anon_id = UserSession.user_id.
6. Claim anon's game seats:
     UPDATE game_seats SET user_id = :new_user_id
      WHERE anon_user_id = :anon_id AND user_id IS NULL;
7. Update UserSession in the session: claimed_by = Some(new_user_id),
   token_version = 0. Session blob is written back by tower-sessions on
   response.
8. Generate verification token (32B random); SHA-256 store in auth_tokens
   (purpose=verify_email, expires_at = now + 24h); mailer.send(verify_link).
9. Return 201 { user: { id, username, email, email_verified: false } }.
   The session cookie was set on a prior anon request (or by this response if
   no session existed yet). No new __sid cookie.
```

### Login (email or username + password)

```
POST /auth/login
body: { login, password }

The handler takes the tower_sessions::Session extractor.

1. Rate-limit by IP (10/min, 60/hr).
2. Lookup user:
   - login.contains('@') → SELECT * FROM users WHERE email = ?
   - else                → SELECT * FROM users WHERE username_canon = LOWER(?)
   Constant-time on miss: argon2.verify against a stored dummy hash so wall time matches.
3. Check login_failures.locked_until > now → 423 Locked.
4. argon2.verify(password, user.password_hash). NULL password_hash (OAuth-only) treated as wrong password.
5. On success:
     a. DELETE FROM login_failures WHERE user_id = :user_id.
     b. UPDATE users SET last_login_at = datetime('now') WHERE id = :user_id.
     c. Read UserSession, anon_id = UserSession.user_id (mint if missing).
     d. UPDATE game_seats SET user_id = :user_id
          WHERE anon_user_id = :anon_id AND user_id IS NULL.
     e. Write UserSession back with claimed_by = Some(user_id),
        token_version = user.token_version. tower-sessions persists on response.
     f. 200 with user object.
6. On failure: UPSERT login_failures.failure_count + 1.
     5 fails  → locked_until = now + 15min.
     10 fails → locked_until = now + 1hr.
   Return 401 invalid_credentials (or 423 if newly locked).
```

### OAuth (Google or GitHub)

```
GET /auth/oauth/:provider/login
  → 302 to provider authorize URL with:
    - state = random 32B (stored server-side, 10-min expiry, in-memory map)
    - PKCE code_challenge (S256)
  Provider routes return 404 if their env-var credentials are unset.

GET /auth/oauth/:provider/callback?code=...&state=...
  1. Verify state matches stored CSRF token (one-time use).
  2. Exchange code for access token (with PKCE verifier).
  3. Fetch userinfo:
     - Google: /v1/userinfo → { sub, email, email_verified, name }
     - GitHub: /user → { id, login } and /user/emails → [{ email, primary, verified }]
  4. provider_email_verified =
       - Google: response.email_verified
       - GitHub: emails[primary].verified
  5. Lookup oauth_accounts WHERE provider=? AND provider_uid=?
     - Hit → log that user in.
     - Miss AND provider_email_verified=true AND users.email = ? AND users.email_verified = 1
       → link: INSERT oauth_accounts; log that user in.
     - Otherwise → enter "pick username" flow:
       - Store a pending-signup record in-memory keyed by a fresh temp_id:
         `{ provider, provider_uid, email, email_verified, suggested_username, expires_at = now + 15min }`
       - Set Set-Cookie: __oauth_pending=<temp_id>; HttpOnly; Secure; SameSite=Lax; Max-Age=900.
       - Redirect to `/` (frontend detects __oauth_pending and prompts for username).

POST /auth/oauth/complete  body: { username }
  1. Read __oauth_pending cookie, look up pending record. Expired/missing → 410.
  2. Validate username.
  3. INSERT users (email_verified = pending.email_verified, password_hash = NULL)
     + INSERT oauth_accounts.
  4. Claim anon: read current session's UserSession, anon_id = UserSession.user_id;
     UPDATE game_seats SET user_id = :new_user_id WHERE anon_user_id = :anon_id AND user_id IS NULL.
  5. Write UserSession back with claimed_by = Some(new_user_id), token_version = 0.
     Clear __oauth_pending cookie. Drop the in-memory pending record.
  6. Return 201 with user object.
```

### Anonymous identity provisioning

Replaces the spec's earlier custom-middleware design. With tower-sessions in place, anon-id is just the `user_id` field on `UserSession`, lazily minted by `Identity::ensure_anon_id`:

```rust
// In auth/session_ext.rs
pub async fn ensure_anon_id(session: &Session) -> Result<Uuid, AuthError> {
    if let Some(s) = session.get::<UserSession>(SESSION_USER_KEY).await? {
        return Ok(s.user_id);
    }
    let s = UserSession { user_id: Uuid::new_v4(), display_name: None,
                          claimed_by: None, token_version: 0 };
    session.insert(SESSION_USER_KEY, &s).await?;
    Ok(s.user_id)
}
```

No middleware needed; the `Identity` extractor calls this. Tower-sessions handles cookie issuance, expiry, and persistence. The existing `GET /player` handler keeps working unchanged (it calls the same code path via the session API).

### Password reset

```
POST /auth/password-reset/request { email }
  → Always 202 regardless of whether email exists.
  → Rate-limit: 3/hr/IP, additionally 1/5min/email (in-memory map).
  → If user exists: INSERT auth_tokens (purpose=password_reset, 1h expiry); mailer.send(reset_link).

POST /auth/password-reset/confirm { token, new_password }
  1. SHA-256 token, lookup auth_tokens row.
  2. Check purpose, expires_at > now, used_at IS NULL.
  3. Validate new password (length, weak list).
  4. argon2id hash new password.
  5. UPDATE users SET password_hash = :hash, token_version = token_version + 1
       WHERE id = :user_id.
     The token_version bump is what invalidates every other live session for
     this user — when their browsers next hit an authenticated endpoint, the
     Identity extractor will compare the stored token_version against the
     stale one in their session blob and drop claimed_by.
  6. Mark auth_tokens.used_at = now.
  7. Write the current session's UserSession with claimed_by = Some(user_id),
     token_version = new_version. The reset flow leaves the requester logged in.
  8. 200.
```

### Email verification

```
GET /auth/verify-email?token=...
  1. SHA-256 token, lookup, check expiry + unused.
  2. UPDATE users.email_verified = 1.
  3. Mark token used.
  4. Redirect to /.
```

### Logout

```
POST /auth/logout
  1. Read UserSession; set claimed_by = None, leave user_id (anon_id) intact.
  2. Write UserSession back. tower-sessions persists. The session cookie is
     NOT cleared — the same browser stays as guest with the same anon_id.
  3. Return 204.
```

Note: this is intentionally not a `session.delete()`. Deleting the session would generate a new anon_id on the next request, orphaning the user's anon game seats. Keeping the session and just nulling `claimed_by` gives the user "back to guest" semantics while preserving their anon game history view.

---

## Integration with existing endpoints

The seat-token (`player_id` UUID) keeps working unchanged everywhere. Identity *enriches* seats but does not replace them.

### `POST /games`

Response unchanged. New side effect: each of the four created seats is inserted into `game_seats` with `anon_user_id = <creator's session user_id>` (always set) and `user_id = <creator's registered user_id>` (only if the creator is logged in). The creator initially owns all four seats; later joins (lobby, challenge, seek) overwrite the relevant rows.

### `POST /matchmaking/seek`, `POST /lobbies`, `POST /lobbies/:id/join`, `POST /challenges/:id/join/:seat`

Each takes the `Identity` extractor. On join:

```sql
UPDATE game_seats
   SET user_id = :user_id_or_null, anon_user_id = :anon_id
 WHERE game_id = :game_id AND seat_index = :seat_index;
```

`anon_user_id` is always set (every Identity carries an anon_id). `user_id` is set only when the joiner is registered; for anon joiners it's NULL. Both columns being populated on a registered join is intentional — it preserves the link "this registered user originated from session anon_X," which the anon-claim transaction relies on.

**DTO change:** these requests keep optional `name` for backward compat, but **if `Identity` is `Registered`, the registered username overrides the request's `name`.** Anti-impersonation rule.

### `POST /games/:game_id/transition`

**Unchanged in this slice.** The current handler takes `(game_id, transition)` and authenticates nothing — it relies on the game's internal `current_player_id` to gate which seat can move when. Tightening transition auth (requiring the request's `Identity` to match the current seat's `user_id`/`anon_user_id`, or a bearer `player_id` header) is recognized as critical for a public site but is **deferred to its own slice**. The identity foundation laid here makes the eventual fix straightforward — every seat now has a queryable owner to compare the requester against.

### `PUT /games/:game_id/players/:player_id/name`

Currently any caller with the `player_id` may rename. After: if `game_seats.user_id IS NOT NULL` for that seat, the endpoint returns 403 — the registered username is canonical for that seat. Anon seats can still rename.

### `GET /games/:game_id/ws`

Both anon and registered can connect. The connection still authenticates as a specific seat via the `player_id` query param (existing behavior). State payloads gain an optional `user_id` next to `player_id` per seat.

### New endpoints

```
POST   /auth/register                      → 201, writes claimed_by into session
POST   /auth/login                         → 200, writes claimed_by into session
POST   /auth/logout                        → 204, clears claimed_by; session cookie preserved
GET    /auth/me                            → 200 with current AuthUser
GET    /auth/oauth/:provider/login         → 302 to provider
GET    /auth/oauth/:provider/callback      → 302 to / on success (sets __oauth_pending if pick-username flow, else writes claimed_by)
POST   /auth/oauth/complete                → 201, writes claimed_by, clears __oauth_pending
POST   /auth/password-reset/request        → 202
POST   /auth/password-reset/confirm        → 200
GET    /auth/verify-email                  → 302 to /

GET    /users/:username                    → 200 public profile
                                             { username, created_at, games_played, last_seen_at }
GET    /users/:username/games              → 200 paginated list of past games (recent 20 default)
PATCH  /users/me                           → 200 (email change kicks off re-verify;
                                                   password change requires current password)
```

---

## Error handling and security

### HTTP status codes

| Condition | Status | Body |
|---|---|---|
| `AuthUser`-required endpoint with no `claimed_by` in session (or stale `token_version`) | 401 | `{"error":"unauthenticated"}` |
| Authenticated but unauthorized | 403 | `{"error":"forbidden"}` |
| Username taken on register | 409 | `{"error":"username_taken"}` |
| Email taken on register | 409 | `{"error":"email_taken"}` |
| Bad credentials on login | 401 | `{"error":"invalid_credentials"}` |
| Account locked | 423 | `{"error":"locked","retry_after_secs":N}` |
| Rate limited | 429 | `{"error":"rate_limited"}` + `Retry-After` |
| Expired/used token | 410 | `{"error":"token_invalid"}` |
| OAuth provider error / state mismatch | 400 | `{"error":"oauth_failed"}` |
| Validation failure | 422 | `{"error":"validation","details":[...]}` |
| Mailer failed | 502 | `{"error":"mailer_failed"}` |

All bodies use the existing `Display`-formatting convention. `AuthError` impls `Display`.

### Leak-prevention rules

- **Login** returns the same 401 for unknown user, wrong password, and OAuth-only-no-password account. Constant-time path on user-not-found via a dummy hash verify.
- **Password-reset request** always returns 202 regardless of whether the email exists.
- **Registration** *does* return field-specific 409s for username and email conflicts — necessary for usable UX. The email-collision leak ("this email has an account here") is deliberate, matches Lila, and is accepted.

### Cookies

There is exactly **one** session cookie, owned by `tower-sessions`. Default name `id`; we override to a more descriptive name (`spades_session`) at `SessionManagerLayer` construction. Configuration:

```rust
SessionManagerLayer::new(session_store)
    .with_name("spades_session")
    .with_secure(!insecure_cookies_flag)   // default true; opt-out via --insecure-cookies
    .with_same_site(SameSite::Lax)
    .with_http_only(true)
    .with_expiry(Expiry::OnInactivity(Duration::days(30)));
```

- `Secure` defaults to true. Opt out via `--insecure-cookies` CLI flag for dev. **Fail-closed** — production deployments without the flag get Secure-only cookies.
- `SameSite=Lax` so top-level navigations (including OAuth callback) carry the cookie; cross-site form posts and iframes don't.
- 30-day inactivity expiry (sliding). Tower-sessions handles the sliding bookkeeping.
- No CSRF token framework. SameSite=Lax + `POST`-only mutations cover the common case. OAuth callback uses the explicit `state` param for CSRF.
- Tower-sessions generates opaque session IDs; we never derive them from user_id.

A second short-lived `__oauth_pending` cookie is set only during the OAuth pick-username flow (15-minute Max-Age) and cleared on completion.

### Password storage

- argon2id, m=64MB, t=3, p=4. Per-password 16B salt. PHC-string format stored in `users.password_hash`.
- No pepper (single-box deployment; pepper adds key-management complexity for marginal benefit).
- Min length 8, no upper bound below 256, no composition rules (per NIST 800-63B).
- Reject top-10k common passwords from an embedded list. (No `zxcvbn` — too heavy for v1.)

### OAuth

- Provider client secrets in env: `GOOGLE_OAUTH_CLIENT_ID`, `GOOGLE_OAUTH_CLIENT_SECRET`, `GITHUB_OAUTH_CLIENT_ID`, `GITHUB_OAUTH_CLIENT_SECRET`. Unset for a provider → that provider's routes return 404 (effectively disabled).
- `OAUTH_REDIRECT_BASE_URL` env var defines the public callback origin.
- `state` param is one-time, 10-min expiry, in-memory store.
- PKCE (S256) used for both providers.
- GitHub: trust `verified: true` on the *primary* email from `/user/emails`. Unverified primary → forced "pick username" flow (no auto-link to existing account by email).
- Google: trust `email_verified` from userinfo.

### Rate limiting

In-process token-bucket via `governor`:

- `POST /auth/login` — 10/min/IP, 60/hr/IP
- `POST /auth/register` — 3/min/IP, 20/hr/IP
- `POST /auth/password-reset/request` — 3/hr/IP, plus 1/5min/email
- `POST /auth/password-reset/confirm` — 10/hr/IP
- OAuth callback — 30/min/IP

Per-account lockout (see Data flows / Login) operates in addition to per-IP buckets.

### Session lifecycle

- 30-day sliding expiry. Each authenticated request bumps `last_seen_at` (debounced to once per 5 min in memory).
- Background cleanup task on startup + every hour: `DELETE FROM sessions WHERE expires_at < datetime('now')`; same for `auth_tokens`.

### Secrets

- No global signing-key environment variable. Tower-sessions cookies are opaque session IDs (no signing required — the cookie is just a pointer to a server-side blob); the prior draft's `SERVER_SECRET` is unnecessary.
- Required env vars validated at startup. Missing required env → server refuses to start.

### Deferred to future slices

- **Transition-endpoint authentication.** Today any caller with the `game_id` can `POST /games/:id/transition`. The fix (require session-or-bearer match against the seat's `user_id`/`player_id`) is its own slice; this one lays the data needed to do it.
- `GET /auth/sessions` + `DELETE /auth/sessions/:id` (user-managed device list).
- "Log out everywhere" UI (already enabled at the data layer by `token_version`).
- Audit / security history (Lila's `security_history`).
- Account deletion and data export (GDPR).
- Personal API access tokens.
- `tower-governor`-style global rate limiting on non-auth endpoints.

---

## Testing strategy

Tooling: same as the rest of the workspace. `cargo test --workspace`. Integration tests use `axum-test` (already in tree).

### Unit tests (`crates/spades-server/src/auth/`)

- `password.rs`: hash round-trip, verify-rejects-wrong, verify-resistant-to-truncation, PHC string stability.
- `users.rs`: username validation (length, charset, reserved names like `me`/`admin`/`auth`), case-insensitive uniqueness, email syntax.
- `sessions.rs`: opaque random ID generation, expiry math, sliding extension.
- `anon.rs`: cookie generation/parse, atomic claim transaction.
- `oauth.rs`: state CSRF gen + validation, PKCE verifier/challenge derivation, GitHub `/user/emails` parsing.
- `rate_limit.rs`: bucket refill, lockout escalation, per-account lockout independent of per-IP bucket.
- `mailer.rs`: `LogMailer` writes to configurable sink; `SmtpMailer` builds correct MIME (mocked at `lettre::Transport`).

### Integration tests (`crates/spades-server/tests/auth_*.rs`)

Each test gets a tempfile SQLite + `AuthState` with `LogMailer` + `wiremock` for OAuth providers.

- `auth_register_login_flow.rs`
- `auth_anon_claim.rs` — anon plays game → registers → game appears in `/users/:name/games`
- `auth_username_login.rs` — login with username (no `@`) works
- `auth_login_lockout.rs` — 5 wrong → 423; correct after lock expires → 200
- `auth_password_reset.rs` — request → mailer link → confirm → old sessions invalidated, new session works
- `auth_email_verify.rs`
- `auth_oauth_google.rs` — wiremock returns verified email → user created without password
- `auth_oauth_github.rs` — emails endpoint parsed; unverified primary → forced pick-username path
- `auth_oauth_complete.rs` — callback sets __oauth_pending; POST /auth/oauth/complete with username creates user and session
- `auth_oauth_link_existing.rs` — existing verified email → OAuth links instead of duplicating
- `auth_session_expiry.rs`
- `auth_rate_limit.rs` — 11th login in a minute → 429
- `auth_csrf_oauth.rs` — bad state → 400
- `auth_secure_cookie_default.rs` — without `--insecure-cookies`, Set-Cookie has Secure flag
- `auth_game_integration.rs` — registered user creates game and joins lobby; game_seats updated; lobby `name` field overridden by username
- `auth_logout_anon_preserved.rs` — logout clears UserSession.claimed_by; user_id (anon) and the session cookie itself are preserved

### Property tests

- Username validator: `canon(canon(x)) == canon(x)`.
- Session ID: 10k generated, all distinct, all round-trip cleanly.

### Manual smoke (not automated)

- Real Google OAuth roundtrip against a dev project.
- Real GitHub OAuth roundtrip.
- Real SMTP send via a transactional provider's test account.

### Out of test scope

- Cross-browser cookie behavior (manual smoke).
- Cryptographic primitive correctness (delegated to audited crates).
- Load / throughput (no benchmarks in this slice).

---

## Open questions for plan-writing time

These are deliberately *not* settled in the design — they don't change the shape, but the plan needs to pick:

1. **In-memory state for OAuth `state` CSRF + per-email password-reset throttle + OAuth pending-signup records.** All three could be in SQLite for simplicity at the cost of writes, or kept in `Arc<Mutex<HashMap<...>>>` for speed at the cost of "lost across restarts." Probably fine in-memory — restarts are rare, and the worst case is the user retries the flow. Plan picks.
2. **Reserved-name list.** Must include at least: `me`, `admin`, `root`, `auth`, `oauth`, `api`, `users`, `games`, `lobbies`, `challenges`, `matchmaking`, `ws`, `static`, `assets`, `docs`, `openapi`, `swagger-ui`, `player`. Plan should finalize.
3. **`/player` and `/player/name` deprecation path.** The existing endpoints overlap with `GET /auth/me` and `PATCH /users/me`. Options: (a) keep them as ergonomic anon-only aliases (no `claimed_by` returned), (b) deprecate and remove, (c) leave them and document the overlap. Plan picks.
