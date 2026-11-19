#!/usr/bin/env bash
# Fresh-clone bootstrap check (WO-03-F / AUD-04).
#
# Reproduces a genuinely fresh checkout (git clone of the working
# repository into /tmp — no untracked working-tree files, no locally
# generated fixtures), runs the DOCUMENTED bootstrap path, and verifies
# the Rust test suite passes without any manual fixture preparation.
#
# Everything regenerable stays outside the persistent tree: the clone's
# BUILD OUTPUT lives under /tmp; the clone itself sits NEXT TO the main
# repository (a few MB, removed at the end) so the bootstrap's budget
# check measures the real persistent project area instead of all of /tmp.
set -euo pipefail
cd "$(dirname "$0")/.."

CLONE="$(cd .. && pwd)/araiguma-fresh-clone"
TARGET=/tmp/araiguma-fresh-target

echo "[fresh-clone] cloning the repository (working tree HEAD, no untracked files)"
rm -rf "$CLONE" "$TARGET"
git clone --quiet . "$CLONE"

# The documented one-time setup from docs/development.md, with the
# sandbox's shared toolchain location and an isolated build directory.
echo "[fresh-clone] running the documented bootstrap"
(
  cd "$CLONE"
  # Same toolchain area as the main workspace; isolated build output.
  ARIGUMA_TOOLCHAINS="${ARIGUMA_TOOLCHAINS:-$(cd .. && pwd)/toolchains}" \
  CARGO_TARGET_DIR="$TARGET" \
  ./scripts/bootstrap.sh
)

# The empty-directory fixtures must exist after bootstrap alone.
echo "[fresh-clone] verifying fixture generation"
for required_dir in \
  fixtures/sysfs/basic-workstation/class/net/wlan0/wireless \
  fixtures/sysfs/combined-laptop/class/net/wlan0/wireless \
  fixtures/sysfs/virtual-interfaces/class/net/br0/bridge \
  fixtures/sysfs/empty/class/net; do
  if [ ! -d "$CLONE/$required_dir" ]; then
    echo "[fresh-clone] FAIL: $required_dir missing after bootstrap" >&2
    exit 1
  fi
done

echo "[fresh-clone] running the sysfs + core test suites without manual preparation"
(
  cd "$CLONE"
  CARGO_TARGET_DIR="$TARGET" cargo test -p araiguma-sysfs -p araiguma-core -p araiguma-cli
)

echo "[fresh-clone] verifying regeneration is byte-stable"
(
  cd "$CLONE"
  ./scripts/gen-sysfs-fixtures.sh
  ./scripts/gen-topology-fixtures.sh > /dev/null
  if [ -n "$(git status --porcelain -- fixtures/)" ]; then
    echo "[fresh-clone] FAIL: regeneration produced drift" >&2
    git diff --stat -- fixtures/ >&2
    exit 1
  fi
)

rm -rf "$CLONE" "$TARGET"
echo "[fresh-clone] OK — a fresh clone bootstraps and passes tests with no manual steps"
