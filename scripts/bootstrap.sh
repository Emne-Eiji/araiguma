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

# --- 3. Fixture trees (WO-03-F / AUD-04) -----------------------------------
# The sysfs/procfs fixture trees contain EMPTY directories (wireless/,
# bridge/, class/net for the empty scenario) that Git cannot track. A
# fresh clone therefore lacks them and three sysfs tests fail until the
# generator runs. Generation is part of the documented bootstrap: it is
# idempotent, deterministic, and verified after running — the full test
# suite never depends on undocumented manual preparation.
if [ -f scripts/gen-sysfs-fixtures.sh ]; then
  log "generating sysfs/procfs fixture trees"
  ./scripts/gen-sysfs-fixtures.sh
  # Verify every empty-directory fixture actually exists; a generator that
  # silently stopped early must fail the bootstrap loudly.
  for required_dir in \
    fixtures/sysfs/basic-workstation/class/net/wlan0/wireless \
    fixtures/sysfs/combined-laptop/class/net/wlan0/wireless \
    fixtures/sysfs/virtual-interfaces/class/net/br0/bridge \
    fixtures/sysfs/empty/class/net; do
    if [ ! -d "$required_dir" ]; then
      echo "bootstrap: fixture directory missing after generation: $required_dir" >&2
      exit 1
    fi
  done
  log "fixture trees verified (empty-directory fixtures present)"
else
  log "no fixture generator found (non-repository layout?); skipping"
fi

# --- 4. Frontend tooling (ephemeral work copy; see scripts/frontend.sh) ----
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
