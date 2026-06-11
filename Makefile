# Makefile — local dev for rust-spades
.DEFAULT_GOAL := help
.ONESHELL:
# bash explicitly: -o pipefail is not POSIX, and Ubuntu's /bin/sh (dash) rejects it
SHELL := bash
.SHELLFLAGS := -eu -o pipefail -c

PORT     ?= 3000
DB       ?= dev.sqlite          # set DB= (empty) for an in-memory database
WEB      ?= http://localhost:5173
RUST_LOG ?= info
DB_FLAG  := $(if $(DB),--db $(DB),)
SERVER   := cargo run -p spades-server -- --port $(PORT) $(DB_FLAG) \
            --insecure-cookies --cors-allow-origin $(WEB)

help:        ## List targets
	@grep -hE '^[a-zA-Z0-9_-]+:.*## ' $(MAKEFILE_LIST) \
	  | awk 'BEGIN{FS=":.*## "}{printf "  \033[36m%-12s\033[0m %s\n",$$1,$$2}'

dev:         ## Backend + frontend together (Ctrl-C stops both)
	@trap 'kill 0' EXIT INT TERM
	RUST_LOG=$(RUST_LOG) $(SERVER) &
	curl -sf --retry 120 --retry-delay 1 --retry-connrefused http://localhost:$(PORT)/health >/dev/null
	pnpm -C web dev

backend:     ## Only the backend (DB= for in-memory)
	RUST_LOG=$(RUST_LOG) $(SERVER)
web:         ## Only the Vite dev server
	pnpm -C web dev

test: test-rust test-web   ## All tests (rust + web unit/component)
test-rust:   ## cargo test (workspace)
	cargo test --workspace
test-web:    ## web unit + component tests
	pnpm -C web test
e2e:         ## web e2e (Playwright auto-starts the backend)
	pnpm -C web test:e2e

fmt:         ## Format rust + web
	cargo fmt
	pnpm -C web format
fmt-check:   ## Verify formatting (rust + web)
	cargo fmt --check
	pnpm -C web format:check
lint:        ## Lint rust + web
	cargo clippy --workspace --all-targets -- -D warnings
	pnpm -C web lint
check: fmt-check lint test  ## Pre-push gate (formatting + lint + tests)

build:       ## Release build (server binary + web bundle)
	cargo build --release -p spades-server
	pnpm -C web build
clean:       ## Remove the local dev DB
	rm -f dev.sqlite dev.sqlite-wal dev.sqlite-shm

.PHONY: help dev backend web test test-rust test-web e2e fmt fmt-check lint check build clean
