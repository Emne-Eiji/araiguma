#!/usr/bin/env bash
# Fresh-clone PREPARATION (WO-03-FC / AUD-04 closure).
#
# Creates a genuinely fresh, isolated clone of the repository at an exact
# commit and takes it through the DOCUMENTED bootstrap — nothing else. The
# VALIDATION half (running the complete test workflow inside that clone)
# lives in scripts/check-fresh-clone.sh; the two are separate so the
# preparation is reusable and independently testable.
#
# What "genuinely fresh" means here:
#   * the clone contains ONLY tracked repository content (verified);
#   * the empty-directory fixtures Git cannot track are verified ABSENT
#     before bootstrap — the bootstrap must create them;
#   * no file from the original checkout's working tree (untracked or
#     generated) is copied — a git clone cannot carry them;
#   * build output, the frontend work copy, and test logs live in
#     caller-provided /tmp locations, never inside the clone.
#
# Usage: scripts/fresh-clone-prepare.sh <dest-dir> [<commit>]
#   <dest-dir>  must not exist; its parent is created as needed
#   <commit>    defaults to this repository's HEAD
#
# The clone models the DELIVERED repository state: no configured remote
# (the origin that `git clone` writes points at this checkout and is
# harness residue, removed here) and the approved repository-local
# identity, so the clone's own git-identity audit applies unchanged.
set -euo pipefail
cd "$(dirname "$0")/.."

if [ $# -lt 1 ] || [ $# -gt 2 ]; then
  echo "usage: $0 <dest-dir> [<commit>]" >&2
  exit 2
fi
DEST="$(cd "$(dirname -- "$1")" && pwd)/$(basename -- "$1")"
COMMIT="${2:-HEAD}"

if [ -e "$DEST" ]; then
  echo "fresh-clone-prepare: destination exists: $DEST" >&2
  exit 1
fi

SRC="$(pwd)"
COMMIT="$(git rev-parse "$COMMIT")"
echo "[fresh-clone-prepare] cloning $COMMIT from $SRC into $DEST"

git clone --quiet "$SRC" "$DEST"

# Pin the exact commit being validated (a clone checks out the source's
# branch head; an explicit commit makes the target unambiguous).
if ! git -C "$DEST" rev-parse HEAD | grep -qx "$COMMIT"; then
  echo "fresh-clone-prepare: clone HEAD is not $COMMIT:" >&2
  git -C "$DEST" rev-parse HEAD >&2
  exit 1
fi

# Model the delivered state: no remote, approved local identity. The
# history itself (all commits by Emne) comes from the objects.
git -C "$DEST" remote remove origin
git -C "$DEST" config user.name "Emne"
git -C "$DEST" config user.email "https://github.com/Emne-Eiji"

# The initial checkout must contain ONLY tracked content: a clean status
# and none of the empty-directory fixtures (they are untracked by nature
# and must be missing until the bootstrap generates them).
if [ -n "$(git -C "$DEST" status --porcelain)" ]; then
  echo "fresh-clone-prepare: clone working tree is not clean:" >&2
  git -C "$DEST" status --short >&2
  exit 1
fi
for absent in \
  fixtures/sysfs/basic-workstation/class/net/wlan0/wireless \
  fixtures/sysfs/combined-laptop/class/net/wlan0/wireless \
  fixtures/sysfs/virtual-interfaces/class/net/br0/bridge \
  fixtures/sysfs/empty/class/net; do
  if [ -e "$DEST/$absent" ]; then
    echo "fresh-clone-prepare: $absent already exists in a fresh clone — the checkout is not genuinely fresh" >&2
    exit 1
  fi
done
echo "[fresh-clone-prepare] initial state verified: tracked content only"

# The DOCUMENTED bootstrap (docs/development.md): toolchain, cargo fetch,
# fixture generation, frontend work copy, storage budget. Runs with the
# caller's isolated locations (exported by check-fresh-clone.sh) — never
# the primary checkout's.
(
  cd "$DEST"
  ./scripts/bootstrap.sh
)

# The bootstrap must have created every empty-directory fixture.
for required in \
  fixtures/sysfs/basic-workstation/class/net/wlan0/wireless \
  fixtures/sysfs/combined-laptop/class/net/wlan0/wireless \
  fixtures/sysfs/virtual-interfaces/class/net/br0/bridge \
  fixtures/sysfs/empty/class/net; do
  if [ ! -d "$DEST/$required" ]; then
    echo "fresh-clone-prepare: $required missing after bootstrap" >&2
    exit 1
  fi
done

# Regeneration must be byte-for-byte stable and leave the tree clean: the
# goldens are committed, so drift means generation is not deterministic.
(
  cd "$DEST"
  ./scripts/gen-sysfs-fixtures.sh > /dev/null
  ./scripts/gen-topology-fixtures.sh > /dev/null
  if [ -n "$(git status --porcelain)" ]; then
    echo "fresh-clone-prepare: fixture regeneration produced drift:" >&2
    git status --short >&2
    exit 1
  fi
)

echo "[fresh-clone-prepare] OK — fresh clone at $COMMIT bootstrapped, fixtures generated and stable"
