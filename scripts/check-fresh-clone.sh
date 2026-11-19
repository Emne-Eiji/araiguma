#!/usr/bin/env bash
# Fresh-clone FULL VALIDATION (WO-03-F / AUD-04, closed by WO-03-FC).
#
# Proves the property that matters: a genuinely fresh checkout of the
# exact commit under validation can execute the COMPLETE applicable
# documented test workflow — Rust fmt/clippy/workspace tests (including
# the isolated-netns integration tests), svelte-check, Vitest, the
# production frontend build, Playwright e2e, npm audit, the private
# D-Bus harness probe, live observation with iproute2 cross-validation,
# live CLI documents through the real frontend validators, the git
# identity audit, shell syntax checks, fixture-regeneration stability,
# and the durable-storage budget — with nothing but the documented
# bootstrap as preparation.
#
# The previous version (WO-03-F) ran only
#   cargo test -p araiguma-sysfs -p araiguma-core -p araiguma-cli
# inside the clone; the independent review correctly found that
# insufficient. This version runs the clone's own scripts/test-all.sh.
#
# Structure (separation is deliberate):
#   * scripts/fresh-clone-prepare.sh — PREPARATION: isolated clone from
#     the exact commit, tracked-only verification, documented bootstrap,
#     fixture generation + stability check. Reusable on its own.
#   * this script — VALIDATION: runs the complete workflow inside the
#     prepared clone, preserves the logs, cleans up.
#
# Never recursive: this script refuses to run when invoked from inside a
# fresh-clone validation (ARIGUMA_FRESH_CLONE_VALIDATION=1), and the
# fresh-clone step inside the clone's own test-all.sh skips itself with
# an explicit recursion-guard message for the same reason.
#
# The primary checkout's validation does not depend on this step's
# outcome for its EARLIER steps (this is the last step of test-all.sh);
# a failure here fails the suite, as any other step failure would.
#
# Regenerable material lives outside the persistent tree:
#   /tmp/araiguma-fresh-clone-area  — the fresh clone (removed at exit)
#   /tmp/araiguma-fresh-target      — isolated cargo build output (wiped
#                                     per run: the build is from scratch)
#   /tmp/araiguma-fresh-work        — isolated frontend work copy
#   /tmp/araiguma-fresh-test-logs   — the clone's own test logs (copied
#                                     into the primary logs dir, then
#                                     removed)
set -euo pipefail
cd "$(dirname "$0")/.."

if [ "${ARIGUMA_FRESH_CLONE_VALIDATION:-}" = "1" ]; then
  echo "check-fresh-clone: refusing to run — already inside a fresh-clone validation (no recursion)" >&2
  exit 1
fi

CLONE_AREA=/tmp/araiguma-fresh-clone-area
CLONE="$CLONE_AREA/araiguma"
TARGET=/tmp/araiguma-fresh-target
WORK=/tmp/araiguma-fresh-work
LOGS=/tmp/araiguma-fresh-test-logs
KEEP_LOGS="${ARIGUMA_TEST_LOG_DIR:-/tmp/araiguma-test-logs}"

# Toolchain location: the shared sandbox toolchain next to the PRIMARY
# checkout (resolved through the primary's env.sh). The clone lives in /tmp
# and cannot find it by the default sibling rule — without this, the
# clone's bootstrap would install a private toolchain on every run (and a
# `--profile minimal` one at that, which lacks rustfmt/clippy). The
# toolchain is an environment dependency (like node itself), not project
# content: sharing it is the documented sandbox behavior
# (docs/development.md). On a genuinely bare machine the bootstrap still
# installs its own — complete with the components the workflow needs.
# shellcheck source=scripts/env.sh disable=SC1091
source scripts/env.sh
export ARIGUMA_TOOLCHAINS

cleanup() {
  local rc=$?
  rm -rf "$CLONE_AREA" "$TARGET" "$WORK" "$LOGS"
  exit "$rc"
}
trap cleanup EXIT INT TERM

# A fresh clone reproduces COMMITTED content. Validating against a dirty
# tree would claim results about content that is not in the commit.
if [ -n "$(git status --porcelain)" ]; then
  echo "check-fresh-clone: working tree is dirty — commit or stash first (a fresh clone can only validate committed content):" >&2
  git status --short >&2
  exit 1
fi
SOURCE_COMMIT="$(git rev-parse HEAD)"
echo "[fresh-clone] validating commit $SOURCE_COMMIT"

# --- preparation ------------------------------------------------------------
rm -rf "$CLONE_AREA" "$TARGET" "$WORK" "$LOGS"
mkdir -p "$CLONE_AREA" "$KEEP_LOGS"
scripts/fresh-clone-prepare.sh "$CLONE" "$SOURCE_COMMIT"

# --- validation: the complete workflow inside the clone ---------------------
# Isolated build output, isolated frontend work copy, isolated logs; the
# recursion marker makes the clone's own fresh-clone step skip itself
# visibly. Shared BY DESIGN (documented in scripts/env.sh and
# docs/development.md, never project files): the Rust toolchain, the
# persistent crate-download cache behind the CARGO_HOME symlink, the npm
# download cache, and the Playwright browsers.
echo "[fresh-clone] running the complete test workflow inside the clone"
(
  cd "$CLONE"
  export ARIGUMA_FRESH_CLONE_VALIDATION=1
  export CARGO_TARGET_DIR="$TARGET"
  export ARIGUMA_FRONTEND_WORK="$WORK"
  export ARIGUMA_TEST_LOG_DIR="$LOGS"
  export ARIGUMA_LIVE_DOCS_DIR=/tmp/araiguma-fresh-live-docs
  ./scripts/test-all.sh
)

# --- preserve the clone's validation logs as evidence -----------------------
if compgen -G "$LOGS/*" > /dev/null; then
  cp -r "$LOGS" "$KEEP_LOGS/fresh-clone-$(date +%Y%m%dT%H%M%S)"
  echo "[fresh-clone] inner logs preserved under $KEEP_LOGS/"
fi

echo "[fresh-clone] OK — commit $SOURCE_COMMIT passes the complete documented workflow from a genuinely fresh checkout"
