# Tier 3a — Server Config Unification (clap) — Design

- **Date:** 2026-05-26
- **Status:** Design approved; implementation pending
- **Scope:** `spades-server` CLI argument parsing only. The first of three Tier 3 sub-projects (the other two — pin prod off `:latest`, smoother provisioning — are separate specs).
- **Guiding constraint:** keep it as simple as possible.

## Context

`main()` scans `std::env::args()` four separate times, inline:

- `--db <path>` (env `DATABASE_URL`) — `skip_while(|a| a != "--db").nth(1)`
- `--insecure-cookies` — `.any(|a| a == "--insecure-cookies")`
- `--cors-allow-origin <origin>` (repeatable) + env `CORS_ALLOW_ORIGIN` (comma-split), the two **combined**
- `--port <n>` (env `PORT`, default 3000) — `.and_then(|p| p.parse().ok()).unwrap_or(3000)`

Problems: no `--help`/`--version`, typo'd flags are silently ignored, and a bad `--port` **silently** falls back to 3000. Env-only config (SMTP_*, OAuth_*, `OAUTH_REDIRECT_BASE_URL`, `RUST_LOG`) is read elsewhere via `SmtpConfig::from_env()` / `OauthState::from_env()` / `init_tracing()` and is out of scope.

## Goals

- Replace the four inline argv scans with a single typed `clap` `Args`, parsed once at the top of `main()`.
- Gain `--help`, `--version`, and fail-fast validation (bad/unknown flags error instead of being silently ignored/defaulted).
- **Preserve every flag and env-var name** so the Dockerfile CMD, Makefile, and CI invocations are unaffected.

## Non-goals (YAGNI / keep it simple)

- No broad `Config` object; SMTP/OAuth/`RUST_LOG` stay in their existing `from_env()` / `init_tracing()` — untouched.
- **No new module file** — `Args` lives inline in `main.rs` (fewer moving parts).
- No change to `validate_startup_config` / `collect_config_warnings`.
- Not touching Tier 3's other two items (`:latest` pin, provisioning).

## Design

### A. Dependency

`crates/spades-server/Cargo.toml` — add:
```toml
clap = { version = "4", features = ["derive", "env"] }
```

### B. `Args` struct (inline in `main.rs`)

Placed near the top of `crates/spades-server/src/bin/server/main.rs` (after the imports, before `AppState`):
```rust
use clap::Parser;

/// spades-server command-line + environment configuration.
#[derive(Parser, Debug)]
#[command(version, about = "Spades game server (HTTP/WebSocket + matchmaking + challenges)")]
struct Args {
    /// Port to listen on.
    #[arg(long, env = "PORT", default_value_t = 3000)]
    port: u16,

    /// SQLite database path; omit for in-memory (state not persisted).
    #[arg(long, env = "DATABASE_URL")]
    db: Option<String>,

    /// Drop the Secure flag on the session cookie (dev only, over http).
    #[arg(long)]
    insecure_cookies: bool,

    /// Allowed CORS origin(s); repeatable, or comma-separated via CORS_ALLOW_ORIGIN.
    #[arg(long = "cors-allow-origin", env = "CORS_ALLOW_ORIGIN", value_delimiter = ',')]
    cors_allow_origin: Vec<String>,
}
```

### C. `main()` wiring

At the top of `main()` (after `init_tracing()` and `validate_startup_config()`):
```rust
    let args = Args::parse();
    let db_path = args.db;
    let insecure_cookies = args.insecure_cookies;
    let cors_origins = args.cors_allow_origin;
    let port = args.port;
```
Delete the four inline scans:
- the `db_path` `std::env::args().skip_while(..="--db")...` block
- `let insecure_cookies = std::env::args().any(..)`
- the `cors_origins` loop over `args` + the separate `CORS_ALLOW_ORIGIN` env read
- the `port` `std::env::args().skip_while(..="--port")...` block

Everything downstream that consumes `db_path` (`Option<String>`), `insecure_cookies` (`bool`), `cors_origins` (`Vec<String>`), and `port` (`u16`) is unchanged — the bindings keep the same names and types.

### D. Behavior changes (all fail-fast improvements)

- Bad `--port` → clean clap error + non-zero exit (was: silently 3000).
- Unknown/typo'd flag → clap error (was: silently ignored).
- `--help` / `--version` (reports crate version `2.0.0`) now exist.
- CORS becomes flag **or** env (clap precedence) rather than flag+env **combined**, and `--cors-allow-origin a,b` comma-splits. Identical in real usage: prod sets only `CORS_ALLOW_ORIGIN`; `make dev`/CI pass only the flag.

The prod invocation (`--port 3000 --db /data/games.sqlite` + `CORS_ALLOW_ORIGIN` env, no `--insecure-cookies`) is fully accepted by this struct.

### E. Tests (protect the CI coverage gate)

Tier 2 now enforces the coverage ratchet in CI, so the new parsing code must be covered or `spades-server` (baseline 76.9%) regresses and the gate fails. Add a small `#[cfg(test)]` block (in `main.rs`'s existing test module) using `Args::try_parse_from(...)`:

- `defaults`: no args → `port == 3000`, `db == None`, `!insecure_cookies`, `cors_allow_origin.is_empty()`
- `explicit_flags`: `--port 4000 --db x.sqlite --insecure-cookies --cors-allow-origin http://a --cors-allow-origin http://b` parses to the expected values
- `comma_separated_cors`: `--cors-allow-origin http://a,http://b` → two origins
- `invalid_port_errors`: `--port notaport` → `is_err()`
- `unknown_flag_errors`: `--nope` → `is_err()`

Env-fallback (`PORT`/`DATABASE_URL`/`CORS_ALLOW_ORIGIN`) is not unit-tested — it mutates process-global env (flaky under parallel tests) and is clap's well-tested behavior.

## Files changed

| File | Change |
|------|--------|
| `crates/spades-server/Cargo.toml` | add `clap` dependency |
| `crates/spades-server/src/bin/server/main.rs` | add `Args` struct + `Args::parse()`, delete the 4 inline scans, add the test block |

No other runtime code changes; the WIP feature already committed (`36ac576`) did not touch the arg-parsing region.

## Verification

- `cargo build -p spades-server` clean; `cargo clippy --workspace --all-targets -- -D warnings` clean.
- `cargo test -p spades-server` green (existing bin tests + new `Args` tests).
- `cargo run -p spades-server -- --help` lists `--port/--db/--insecure-cookies/--cors-allow-origin`; `--version` prints `2.0.0`.
- `make dev` and `make e2e` still start the server (flags unchanged).
- `hooks/coverage-check.sh` shows no `spades-server` regression below 76.9%.

## Risk

Low. All flag/env names and the real prod/Makefile/CI invocations are preserved; the only behavior changes are fail-fast errors on previously-silent bad/unknown input. The change is confined to `main()`'s config parsing.

## Out of scope / remaining Tier 3

- **Pin prod off `:latest`** (docker-compose.yml) — separate spec.
- **Smoother provisioning** (install-docker.sh) — separate spec.
