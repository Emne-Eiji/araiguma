#!/usr/bin/env bash
# Reproducible bootstrap: prepare toolchains and dependencies.
# Safe to re-run; recreates all ephemeral build state after a sandbox recycle.
set -euo pipefail
cd "$(dirname "$0")/.."
# shellcheck source=scripts/env.sh disable=SC1091
source scripts/env.sh

log() { printf '[bootstrap] %s\n' "$*"; }

# --- 1. Rust toolchain -----------------------------------------------------
if command -v cargo >/dev/null 2>&1 && cargo --version >/dev/null 2>&1; then
  log "rust ok: $(cargo --version | cut -d' ' -f2) ($(rustc --version | cut -d' ' -f2))"
else
  channel="$(sed -n 's/^channel = "\(.*\)"/\1/p' rust-toolchain.toml)"
  log "no usable cargo found — installing Rust $channel into $ARIGUMA_TOOLCHAINS"
  mkdir -p "$ARIGUMA_TOOLCHAINS"
  curl -sSfL https://sh.rustup.rs -o /tmp/rustup-init.sh
  RUSTUP_HOME="$ARIGUMA_TOOLCHAINS/rustup" CARGO_HOME="$ARIGUMA_TOOLCHAINS/cargo" \
    sh /tmp/rustup-init.sh -y --profile minimal --no-modify-path \
       --default-toolchain "$channel"
  source scripts/env.sh
  log "rust installed: $(cargo --version)"
fi

# --- 2. Cargo dependencies (fetch only; extraction is ephemeral) -----------
if cargo metadata --format-version 1 >/dev/null 2>&1; then
  if [ -f Cargo.lock ]; then
    log "cargo fetch --locked"
    cargo fetch --locked
  else
    log "cargo fetch (no lockfile yet)"
    cargo fetch
  fi
else
  log "workspace has no crates yet; skipping cargo fetch"
fi

# --- 3. Frontend tooling (ephemeral work copy; see scripts/frontend.sh) ----
if [ -f apps/frontend/package.json ]; then
  scripts/frontend.sh prepare
fi

# --- 4. Playwright browsers (informational) --------------------------------
if [ -n "$(ls -A "$PLAYWRIGHT_BROWSERS_PATH" 2>/dev/null || true)" ]; then
  log "playwright browsers present in $PLAYWRIGHT_BROWSERS_PATH"
else
  log "playwright browsers absent; scripts/frontend.sh e2e will fetch them"
fi

# --- 5. Durable-storage budget --------------------------------------------
scripts/check-durable-size.sh

log "bootstrap complete"
