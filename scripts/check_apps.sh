#!/usr/bin/env bash
# Lint and test the two crates the wheel actually turns on.
#
# They live under `apps/`, which the workspace excludes, so `cargo clippy
# --workspace` never sees them and `cargo test --workspace` never runs them. That
# is how the application accumulated 21 and 41 warnings while the gate stayed
# green. Each crate is its own workspace, so each is checked from its own
# directory.
set -uo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
APP="$ROOT/apps/rusty-brick-chess"

step() { printf '\n== %s\n' "$*"; }
fail() { printf 'FAIL: %s\n' "$*" >&2; exit 1; }

step "server crate: fmt, clippy, tests"
(cd "$APP" && cargo fmt --all -- --check) || fail "app fmt"
(cd "$APP" && cargo clippy --all-targets -- -D warnings) || fail "app clippy"
(cd "$APP" && cargo test) || fail "app tests"

step "frontend crate: fmt, clippy (wasm), tests"
(cd "$APP/frontend" && cargo fmt --all -- --check) || fail "frontend fmt"
# The frontend only compiles for wasm; a host-target clippy run would lint code
# that never ships.
(cd "$APP/frontend" && cargo clippy --target wasm32-unknown-unknown --all-targets -- -D warnings) \
  || fail "frontend clippy"
(cd "$APP/frontend" && cargo test) || fail "frontend tests"

printf '\nAPPS_PASS\n'
