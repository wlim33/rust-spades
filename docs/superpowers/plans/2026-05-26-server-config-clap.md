# Tier 3a — Server Config via clap — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace `main()`'s four inline `std::env::args()` scans with a single typed `clap` `Args` struct (`--help`, `--version`, fail-fast validation), preserving every flag/env name.

**Architecture:** Add `clap` (derive+env). Define a private `Args` struct inline in `main.rs` (kept simple — no new module), parse it once at the top of `main()`, and replace the four scans with field reads. Add a small `config_tests` module so the new code is covered (Tier 2's CI coverage gate). One commit (the struct must be used in `main()` in the same change, or `dead_code` fails clippy `-D warnings`).

**Tech Stack:** Rust, clap 4 (derive + env).

**Branch:** `dx/server-config-clap` (spec already committed there; `main.rs` is clean — the prior WIP feature is committed as `36ac576`).

**Spec:** `docs/superpowers/specs/2026-05-26-server-config-clap-design.md`

**Guardrail:** Other uncommitted changes remain in the tree (web WIP, `.wrangler/`, the `.travis.yml`/`web/.github` deletions, `setup.ts`). Commit ONLY the two files this task names, via pathspec. NEVER `git add -A`/`.`/`-a`/`--amend`.

---

### Task 1: Server config via clap

**Files:**
- Modify: `crates/spades-server/Cargo.toml` (add `clap`)
- Modify: `crates/spades-server/src/bin/server/main.rs` (add `Args` + tests; replace the 4 scans)

- [ ] **Step 1: Add the clap dependency**

In `crates/spades-server/Cargo.toml`, replace:
```toml
tracing-subscriber = { version = "0.3", features = ["env-filter"] }

[dev-dependencies]
```
with:
```toml
tracing-subscriber = { version = "0.3", features = ["env-filter"] }
clap = { version = "4", features = ["derive", "env"] }

[dev-dependencies]
```

- [ ] **Step 2: Add the `Args` struct + its tests**

In `crates/spades-server/src/bin/server/main.rs`, replace:
```rust
#[tokio::main]
async fn main() {
```
with:
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

#[cfg(test)]
mod config_tests {
    use super::Args;
    use clap::Parser;

    #[test]
    fn defaults() {
        let a = Args::try_parse_from(["spades-server"]).unwrap();
        assert_eq!(a.port, 3000);
        assert!(a.db.is_none());
        assert!(!a.insecure_cookies);
        assert!(a.cors_allow_origin.is_empty());
    }

    #[test]
    fn explicit_flags() {
        let a = Args::try_parse_from([
            "spades-server", "--port", "4000", "--db", "x.sqlite",
            "--insecure-cookies", "--cors-allow-origin", "http://a",
            "--cors-allow-origin", "http://b",
        ])
        .unwrap();
        assert_eq!(a.port, 4000);
        assert_eq!(a.db.as_deref(), Some("x.sqlite"));
        assert!(a.insecure_cookies);
        assert_eq!(a.cors_allow_origin, ["http://a", "http://b"]);
    }

    #[test]
    fn comma_separated_cors() {
        let a = Args::try_parse_from(["spades-server", "--cors-allow-origin", "http://a,http://b"]).unwrap();
        assert_eq!(a.cors_allow_origin, ["http://a", "http://b"]);
    }

    #[test]
    fn invalid_port_errors() {
        assert!(Args::try_parse_from(["spades-server", "--port", "notaport"]).is_err());
    }

    #[test]
    fn unknown_flag_errors() {
        assert!(Args::try_parse_from(["spades-server", "--nope"]).is_err());
    }
}

#[tokio::main]
async fn main() {
```

- [ ] **Step 3: Wire `main()` — replace the four scans**

These four edits are all inside `async fn main()`. The first introduces `let args = Args::parse();`; the others read its fields. `Args` is now used in `main()` (no `dead_code`).

Edit 3a — the `--db` scan. Replace:
```rust
    let db_path = std::env::args()
        .skip_while(|a| a != "--db")
        .nth(1)
        .or_else(|| std::env::var("DATABASE_URL").ok());
```
with:
```rust
    let args = Args::parse();
    let db_path = args.db;
```

Edit 3b — the `--insecure-cookies` scan. Replace:
```rust
    let insecure_cookies = std::env::args().any(|a| a == "--insecure-cookies");
```
with:
```rust
    let insecure_cookies = args.insecure_cookies;
```

Edit 3c — the CORS scan (flag loop + env). Replace:
```rust
    let mut cors_origins: Vec<String> = Vec::new();
    let args: Vec<String> = std::env::args().collect();
    for (i, a) in args.iter().enumerate() {
        if a == "--cors-allow-origin" {
            if let Some(v) = args.get(i + 1) {
                cors_origins.push(v.clone());
            }
        }
    }
    if let Ok(env_origins) = std::env::var("CORS_ALLOW_ORIGIN") {
        for o in env_origins.split(',') {
            let o = o.trim();
            if !o.is_empty() {
                cors_origins.push(o.to_string());
            }
        }
    }
```
with:
```rust
    let cors_origins = args.cors_allow_origin;
```

Edit 3d — the `--port` scan. Replace:
```rust
    let port: u16 = std::env::args()
        .skip_while(|a| a != "--port")
        .nth(1)
        .or_else(|| std::env::var("PORT").ok())
        .and_then(|p| p.parse().ok())
        .unwrap_or(3000);
```
with:
```rust
    let port = args.port;
```

- [ ] **Step 4: Build, lint, and run the tests**

Prefix cargo with the PATH export if needed (`export PATH="$HOME/.cargo/bin:$PATH"`).

Run: `cargo build -p spades-server`
Expected: compiles cleanly (no `dead_code`/unused-import warnings — `Args` is used by `main()` and the tests).

Run: `cargo clippy -p spades-server --all-targets -- -D warnings`
Expected: no warnings.

Run: `cargo test -p spades-server`
Expected: PASS — the five `config_tests` plus all existing bin/integration tests are green.

- [ ] **Step 5: Confirm `--help` / `--version` and that the server still starts**

Run: `cargo run -p spades-server -- --help`
Expected: usage text listing `--port`, `--db`, `--insecure-cookies`, `--cors-allow-origin` (and `--help`/`--version`); process exits 0 without starting the server.

Run: `cargo run -p spades-server -- --version`
Expected: prints `spades-server 2.0.0` (crate version); exits 0.

- [ ] **Step 6: Commit**

`cargo build` (Step 4) updated `Cargo.lock` to add clap; commit it alongside the source so the build stays reproducible — one commit, three files:
```bash
git add Cargo.lock crates/spades-server/Cargo.toml crates/spades-server/src/bin/server/main.rs
git commit -m "refactor(server): parse CLI config with clap (--help, validation)" -- Cargo.lock crates/spades-server/Cargo.toml crates/spades-server/src/bin/server/main.rs
```

---

### Final verification

- [ ] **Step 1: Full server test + clippy once more**

Run: `cargo test -p spades-server && cargo clippy -p spades-server --all-targets -- -D warnings`
Expected: tests green, no clippy warnings.

- [ ] **Step 2: Behavior parity — server boots with the real flags**

Run (then Ctrl-C / kill): `cargo run -p spades-server -- --port 3000 --insecure-cookies --cors-allow-origin http://localhost:5173`
Expected: logs `spades server listening ... addr=0.0.0.0:3000` and the `--insecure-cookies` warning — i.e. identical startup to before. (`make dev` / `make e2e`, which pass these same flags, continue to work.)

- [ ] **Step 3: Coverage didn't regress (optional locally; enforced in CI)**

If `cargo-tarpaulin` is installed: `hooks/coverage-check.sh` → `spades-server` at/above its 76.9% baseline. Otherwise this is checked by the CI `coverage` job (Tier 2) on the next push/PR.

- [ ] **Step 4: Scope check**

Run: `git diff --name-only master..HEAD`
Expected: only `Cargo.lock`, `crates/spades-server/Cargo.toml`, `crates/spades-server/src/bin/server/main.rs`, and the spec/plan docs. No web files, no `.wrangler/`.
