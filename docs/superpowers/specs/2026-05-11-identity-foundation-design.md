# Identity Foundation — Design Spec

**Date:** 2026-05-11
**Slice:** First slice of "port Lila patterns into rust-spades."
**Status:** Design approved by user. Awaiting written-spec review before plan.

---

## Context

`rust-spades` v2.0.0 is a working Spades game server: `spades-core` (game engine library) + `spades-server` (Axum REST + WebSocket + SSE + optional SQLite, with seek queue, lobbies, challenge links, Fischer timers, name validation, random AI). 238 tests pass. The server is single-binary, single-box, currently deployable to a Linux VM via the gitignored deploy scripts.

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
| Anonymous vs registered | Anon-first, claimable. Guests get a long-lived `__anon_id` cookie; on register/login, the anon-session's games attach to the new user. |
| Auth methods | Email/password and OAuth (Google, GitHub). No magic links. No OAuth-provider role. |
| Datastore | SQLite (becomes mandatory; was optional). No Postgres, no Redis in this slice. |
| Username model | Lila-style: immutable, case-insensitive, ASCII `[a-zA-Z0-9_-]`, length 2-20. |
| Email | Pluggable `Mailer` trait. Default `SmtpMailer` via `lettre`, configured via `SMTP_*` env vars. `LogMailer` for dev/CI. |
| Rate limiting | Auth endpoints only, in-process token bucket via `governor`, plus per-account lockout. No global tower-governor in this slice. |
| Architectural shape | New `auth/` module inside `spades-server`. Not a new crate. |
| Sessions | Server-side stored, opaque random ID, cookie-based, 30-day sliding. Tower-sessions or hand-rolled (decided at plan time). |
| Login identifier | Either email or username. `login` field; `'@'` discriminates. |

---

## Architecture

### Module layout

```
crates/spades-server/src/
├── auth/                       ← NEW module
│   ├── mod.rs                  ← AuthState, pub re-exports, Identity / AuthUser extractors
│   ├── users.rs                ← User struct, repo (CRUD), username rules
│   ├── sessions.rs             ← Session struct, repo, cookie helpers
│   ├── anon.rs                 ← AnonSession struct, claim logic, middleware
│   ├── oauth.rs                ← Google + GitHub flow, state CSRF, PKCE
│   ├── password.rs             ← argon2id hash/verify, weak-password reject list
│   ├── mailer.rs               ← Mailer trait + SmtpMailer + LogMailer
│   ├── rate_limit.rs           ← Per-endpoint token buckets, per-account lockout
│   └── error.rs                ← AuthError → HTTP response
├── sqlite_store.rs             ← MODIFIED: new tables (see Data model)
├── game_manager.rs             ← unchanged
├── matchmaking.rs              ← unchanged shape; handlers pass Option<&Identity> in
├── challenges.rs               ← same
├── validation.rs               ← unchanged
└── bin/server/
    ├── main.rs                 ← MODIFIED: build AuthState, mount /auth/* and /users/*
    ├── handlers/
    │   ├── auth.rs             ← NEW: register, login, logout, me, password-reset/*, verify-email
    │   ├── oauth.rs            ← NEW: /auth/oauth/:provider/{login,callback}
    │   ├── users.rs            ← NEW: GET /users/:username, GET /users/:username/games, PATCH /users/me
    │   ├── games.rs            ← MODIFIED: Identity extractor, write to game_seats
    │   ├── matchmaking.rs      ← MODIFIED: same
    │   ├── challenges.rs       ← MODIFIED: same
    │   └── players.rs          ← MODIFIED: rename rejects if seat is user-bound
    └── dto.rs                  ← MODIFIED: auth request/response DTOs
```

### Extractors

`auth::mod` exports two Axum extractors:

- **`AuthUser(User)`** — 401 if no valid `__sid`. Use on `/auth/me`, `PATCH /users/me`, etc.
- **`Identity`** — `enum Identity { Registered(User), Anonymous(AnonSession) }`. Never fails because anon middleware auto-provisions an anon-session if `__anon_id` is missing. Use on game/lobby/challenge endpoints.

Anonymous-session middleware runs before extractors and ensures every request has an `__anon_id` cookie (issuing one if absent or stale).

### Dependencies added to `spades-server/Cargo.toml`

- `argon2` — password hashing
- `oauth2` — OAuth client
- `lettre` — SMTP mailer
- `governor` — token-bucket rate limit
- `cookie` (likely already transitive) — signed cookie helpers
- `tower-sessions` + `tower-sessions-sqlx-store` *or* hand-rolled session middleware on `rusqlite` (decision deferred to plan)

### Untouched surfaces

- `spades-core` — no changes. The library remains identity-free.
- `Game` JSON shape persisted in `games.state` — no changes. Identity lives in a sibling `game_seats` table.
- `GameManager` public surface — unchanged.
- `GameTransition` semantics, including bearer-`player_id` auth for `POST /games/:id/transition` — unchanged.

---

## Data model

All new tables. SQLite, WAL mode (already required).

```sql
-- Registered users
CREATE TABLE users (
    id              TEXT PRIMARY KEY,              -- UUID v4
    username        TEXT NOT NULL,                 -- display form ("Alice")
    username_canon  TEXT NOT NULL UNIQUE,          -- lowercase ("alice"), for lookup
    email           TEXT NOT NULL UNIQUE,
    email_verified  INTEGER NOT NULL DEFAULT 0,    -- bool
    password_hash   TEXT,                          -- argon2id PHC; NULL = OAuth-only account
    created_at      TEXT NOT NULL DEFAULT (datetime('now')),
    last_login_at   TEXT
);
CREATE INDEX users_username_canon ON users(username_canon);

-- Active and recent sessions
CREATE TABLE sessions (
    id              TEXT PRIMARY KEY,              -- opaque random 32B base64url
    user_id         TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    created_at      TEXT NOT NULL DEFAULT (datetime('now')),
    last_seen_at    TEXT NOT NULL DEFAULT (datetime('now')),
    expires_at      TEXT NOT NULL,                 -- 30 days from creation, sliding
    user_agent      TEXT,
    ip              TEXT
);
CREATE INDEX sessions_user_id ON sessions(user_id);
CREATE INDEX sessions_expires_at ON sessions(expires_at);

-- Anonymous browser identity (long-lived)
CREATE TABLE anon_sessions (
    id              TEXT PRIMARY KEY,              -- UUID, in __anon_id cookie
    created_at      TEXT NOT NULL DEFAULT (datetime('now')),
    last_seen_at    TEXT NOT NULL DEFAULT (datetime('now')),
    claimed_by      TEXT REFERENCES users(id) ON DELETE SET NULL
);

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
    game_id         TEXT NOT NULL,                 -- not FK; games may live in-memory only
    seat_index      INTEGER NOT NULL,              -- 0..3 for A,B,C,D
    player_id       TEXT NOT NULL,                 -- bearer-auth UUID (unchanged)
    user_id         TEXT REFERENCES users(id) ON DELETE SET NULL,
    anon_session_id TEXT REFERENCES anon_sessions(id) ON DELETE SET NULL,
    is_bot          INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (game_id, seat_index)
);
CREATE INDEX game_seats_user_id ON game_seats(user_id);
CREATE INDEX game_seats_anon_session_id ON game_seats(anon_session_id);
```

### Key data-model choices

- **`username_canon` as a separate column**, not `LOWER(username) UNIQUE`. Explicit, portable, easy to keep consistent in code.
- **`game_seats` is a separate table from `games`.** Keeps `spades-core::Game` JSON identity-free and avoids a destructive migration of existing rows. The `games.state` JSON column is left untouched.
- **`game_seats.game_id` is intentionally not a foreign key** to `games.id`. Games live in-memory in `GameManager` between transitions and are persisted to the `games` table on each state change; there's a small window where a game exists in memory before its first persist. `game_seats` rows are written eagerly on seat-assignment (game create, lobby join, challenge join), independently of whether the `games` row has hit disk yet. No FK keeps this decoupled.
- **Per-account lockout is per-account, not per-(account, IP).** Simpler and harder for a griefer to abuse to lock a victim's account from a third IP.

---

## Data flows

### Registration (email/password)

```
POST /auth/register
body: { username, email, password }

1. Validate: username 2-20 ASCII [a-zA-Z0-9_-], reserved-name reject list,
   email parseable (basic syntax + MX optional), password >= 8 chars, not in
   embedded top-10k weak list.
2. Check users.username_canon and users.email for uniqueness.
3. argon2id(password, m=64MB, t=3, p=4) → users.password_hash.
4. INSERT users (email_verified=0).
5. Claim anon if __anon_id cookie present:
     BEGIN;
     UPDATE anon_sessions SET claimed_by = :user_id WHERE id = :anon_id AND claimed_by IS NULL;
     UPDATE game_seats   SET user_id = :user_id WHERE anon_session_id = :anon_id;
     COMMIT;
6. Generate verification token (32B random); SHA-256 store in auth_tokens; mailer.send(verify_link).
7. Create session (32B random id); INSERT sessions (expires_at = now + 30d).
8. Set-Cookie: __sid=<id>; HttpOnly; Secure; SameSite=Lax; Path=/; Max-Age=2592000.
9. Return 201 { user: { id, username, email, email_verified: false } }.
```

### Login (email or username + password)

```
POST /auth/login
body: { login, password }

1. Rate-limit by IP (10/min, 60/hr).
2. Lookup user:
   - login.contains('@') → SELECT * FROM users WHERE email = ?
   - else                → SELECT * FROM users WHERE username_canon = LOWER(?)
   Constant-time on miss: argon2.verify against a stored dummy hash so wall time matches.
3. Check login_failures.locked_until > now → 423 Locked.
4. argon2.verify(password, user.password_hash). NULL password_hash (OAuth-only) treated as wrong password.
5. On success: clear login_failures, create session, claim anon if cookie present, set __sid cookie, 200 with user object.
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
  4. Claim anon if __anon_id cookie present.
  5. Create session, set __sid cookie, clear __oauth_pending, drop pending record.
  6. Return 201 with user object.
```

### Anonymous identity provisioning

Middleware applied to every request (early in the stack):

```
1. Read __anon_id cookie.
2. If absent: generate UUID v4, INSERT anon_sessions, set Set-Cookie __anon_id (1-year sliding).
3. If present but row missing (stale or tampered): treat as absent, regenerate.
4. UPDATE anon_sessions.last_seen_at debounced — track an in-memory map of (id → last_write_at);
   only hit DB if >= 5 minutes since last write.
```

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
  4. argon2id hash new password, UPDATE users.password_hash.
  5. Mark auth_tokens.used_at = now.
  6. DELETE FROM sessions WHERE user_id = ?  -- invalidate all existing sessions.
  7. Create new session, set __sid cookie, 200.
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
  1. DELETE FROM sessions WHERE id = current.
  2. Set-Cookie: __sid=; Max-Age=0.
  3. Do NOT clear __anon_id — user reverts to guest identity with games still on profile.
  4. Return 204.
```

---

## Integration with existing endpoints

The seat-token (`player_id` UUID) keeps working unchanged everywhere. Identity *enriches* seats but does not replace them.

### `POST /games`

Response unchanged. New side effect: each of the four created seats is inserted into `game_seats` with `anon_session_id = <creator's anon>` (and `user_id = <creator's user_id>` if registered). The creator initially owns all four seats; later joins (lobby, challenge, seek) overwrite the relevant rows.

### `POST /matchmaking/seek`, `POST /lobbies`, `POST /lobbies/:id/join`, `POST /challenges/:id/join/:seat`

Each takes the `Identity` extractor. On join:

```sql
UPDATE game_seats
   SET user_id = :user_id, anon_session_id = :anon_id
 WHERE game_id = :game_id AND seat_index = :seat_index;
```

(`user_id` and `anon_session_id` are mutually exclusive — exactly one is non-null per `Identity` variant.)

**DTO change:** these requests keep optional `name` for backward compat, but **if `Identity` is `Registered`, the registered username overrides the request's `name`.** Anti-impersonation rule.

### `POST /games/:game_id/transition`

**Unchanged in this slice.** The current handler takes `(game_id, transition)` and authenticates nothing — it relies on the game's internal `current_player_id` to gate which seat can move when. Tightening transition auth (requiring `__sid` matching the current seat's `user_id`, or a bearer `player_id` header) is recognized as critical for a public site but is **deferred to its own slice**. The identity foundation laid here makes the eventual fix straightforward — every seat now has a queryable `user_id`/`anon_session_id` to compare the requester against.

### `PUT /games/:game_id/players/:player_id/name`

Currently any caller with the `player_id` may rename. After: if `game_seats.user_id IS NOT NULL` for that seat, the endpoint returns 403 — the registered username is canonical for that seat. Anon seats can still rename.

### `GET /games/:game_id/ws`

Both anon and registered can connect. The connection still authenticates as a specific seat via the `player_id` query param (existing behavior). State payloads gain an optional `user_id` next to `player_id` per seat.

### New endpoints

```
POST   /auth/register                      → 201, sets __sid
POST   /auth/login                         → 200, sets __sid
POST   /auth/logout                        → 204, clears __sid
GET    /auth/me                            → 200 with current AuthUser
GET    /auth/oauth/:provider/login         → 302 to provider
GET    /auth/oauth/:provider/callback      → 302 to / on success (sets __sid or __oauth_pending)
POST   /auth/oauth/complete                → 201, sets __sid, clears __oauth_pending
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
| Missing/invalid `__sid` on `AuthUser`-required endpoint | 401 | `{"error":"unauthenticated"}` |
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

```
Set-Cookie: __sid=<32B base64url>; HttpOnly; Secure; SameSite=Lax; Path=/; Max-Age=2592000
Set-Cookie: __anon_id=<UUID>;       HttpOnly; Secure; SameSite=Lax; Path=/; Max-Age=31536000
```

- `Secure` defaults to true. Opt out via `--insecure-cookies` CLI flag for dev. **Fail-closed.**
- `SameSite=Lax` so top-level navigations (including OAuth callback) carry the cookie; cross-site form posts and iframes don't.
- No CSRF token framework. SameSite=Lax + `POST`-only mutations cover the common case. OAuth callback uses the explicit `state` param for CSRF.
- Session IDs are opaque, generated by `getrandom`, base64url-encoded. Never derived from user_id.

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

- `SERVER_SECRET` (32 random bytes, hex) signs cookies. Rotation invalidates all sessions. Required at startup unless `--insecure-cookies` is set.
- Required env vars validated at startup. Missing required env → server refuses to start.

### Deferred to future slices

- **Transition-endpoint authentication.** Today any caller with the `game_id` can `POST /games/:id/transition`. The fix (require session-or-bearer match against the seat's `user_id`/`player_id`) is its own slice; this one lays the data needed to do it.
- `GET /auth/sessions` + `DELETE /auth/sessions/:id` (user-managed device list).
- Per-key rotation for `SERVER_SECRET`.
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
- `auth_logout_anon_preserved.rs` — logout clears __sid, leaves __anon_id

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

1. **Tower-sessions vs hand-rolled session middleware.** Tower-sessions is convenient but pulls in `tower-sessions-sqlx-store` (or a custom store) and an opinionated cookie model. Hand-rolled gives ~150 lines of code with `rusqlite` directly and zero new abstractions. Decide based on whether tower-sessions' cookie defaults match this spec without fighting them.
2. **Anon-session middleware vs per-handler extractor.** Middleware is simpler (every request gets an anon-id whether it needs one or not). Extractor avoids DB writes on truly anonymous endpoints like `/healthz`. Middleware probably wins on simplicity unless an unauthenticated read-heavy endpoint emerges.
3. **In-memory state for OAuth `state` CSRF + per-email password-reset throttle.** Both could be in SQLite for simplicity at the cost of writes. Probably fine in-memory; if the server restarts mid-OAuth-flow, the user just retries.
4. **Reserved-name list.** Must include at least: `me`, `admin`, `root`, `auth`, `oauth`, `api`, `users`, `games`, `lobbies`, `challenges`, `matchmaking`, `ws`, `static`, `assets`. Plan should finalize.
