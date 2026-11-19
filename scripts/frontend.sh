#!/usr/bin/env bash
# Frontend tooling runner.
#
# WHY A WORK COPY (deviation from the work order's prescribed mechanism,
# documented in docs/development.md and the closeout): the sandbox's command
# policy forbids creating the gitignored node_modules symlink that the work
# order prescribes, and a real node_modules directory (250+ MB) inside the
# persistent tree would break the durable-storage budget and risk another
# persistence-layer incident. All frontend tooling therefore runs inside an
# ephemeral work copy at /tmp/araiguma-work that mirrors the repository
# layout (fixtures/ + apps/frontend/): node_modules exists only there, and
# the generated package-lock.json is copied back so it stays committed.
# The persistent tree never contains node_modules at all.
#
# Usage: scripts/frontend.sh <command>
#   prepare   refresh the work copy and install dependencies (idempotent)
#   check     svelte-check
#   test      vitest run
#   build     production build
#   e2e       production build + playwright tests
#
# The work-copy location defaults to /tmp/araiguma-work and can be moved
# with ARIGUMA_FRONTEND_WORK (used by the fresh-clone validation, which
# must not touch the primary checkout's work copy).
set -euo pipefail
cd "$(dirname "$0")/.."
# shellcheck source=scripts/env.sh disable=SC1091
source scripts/env.sh

WORK="${ARIGUMA_FRONTEND_WORK:-/tmp/araiguma-work}"
COMMAND="${1:-prepare}"

prepare() {
  if [ ! -f apps/frontend/package.json ]; then
    echo "frontend.sh: apps/frontend/package.json not found" >&2
    exit 1
  fi
  mkdir -p "$WORK/fixtures" "$WORK/apps"
  # Refresh sources (never node_modules, dist, or vite cache).
  rm -rf "$WORK/apps/frontend"
  mkdir -p "$WORK/apps/frontend"
  (cd apps/frontend && tar cf - --exclude=node_modules --exclude=dist --exclude=.vite .) \
    | (cd "$WORK/apps/frontend" && tar xf -)
  rm -rf "$WORK/fixtures"
  cp -a fixtures "$WORK/fixtures"

  if [ ! -d "$WORK/apps/frontend/node_modules" ]; then
    (cd "$WORK/apps/frontend" && npm install --no-audit --no-fund)
  else
    (cd "$WORK/apps/frontend" && npm install --no-audit --no-fund --prefer-offline)
  fi

  # Keep the committed lockfile in the repository current.
  if [ -f "$WORK/apps/frontend/package-lock.json" ]; then
    cp "$WORK/apps/frontend/package-lock.json" apps/frontend/package-lock.json
  fi
}

case "$COMMAND" in
  prepare)
    prepare
    echo "frontend work copy ready: $WORK"
    ;;
  check)
    prepare
    (cd "$WORK/apps/frontend" && npx --no-install svelte-check)
    ;;
  test)
    prepare
    (cd "$WORK/apps/frontend" && npx --no-install vitest run)
    ;;
  build)
    prepare
    (cd "$WORK/apps/frontend" && npx --no-install vite build)
    ;;
  e2e)
    prepare
    (cd "$WORK/apps/frontend" \
      && npx --no-install playwright install chromium \
      && npx --no-install vite build \
      && npx --no-install playwright test)
    ;;
  *)
    echo "frontend.sh: unknown command '$COMMAND' (prepare|check|test|build|e2e)" >&2
    exit 2
    ;;
esac
