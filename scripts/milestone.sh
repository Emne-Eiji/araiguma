#!/usr/bin/env bash
# milestone.sh <NAME> — verify repository integrity at a milestone, then write
# recovery artifacts (git bundle, source archive, SHA-256 manifest) into the
# persistent download/ directory OUTSIDE the repository.
#
# Usage:   scripts/milestone.sh WO01R-M1
set -euo pipefail
cd "$(dirname "$0")/.."
[ $# -eq 1 ] || { echo "usage: scripts/milestone.sh <NAME>   (e.g. WO01R-M1)" >&2; exit 2; }
NAME="$1"
OUT="$(dirname -- "$PWD")/download"
mkdir -p "$OUT"

# 1. The working tree must be clean.
if [ -n "$(git status --porcelain)" ]; then
  echo "FAIL: working tree is not clean — commit or stash first" >&2
  git status --porcelain >&2
  exit 1
fi

# 2. Repository-local identity must be exactly the approved one.
[ "$(git config user.name)" = "Emne" ] || { echo "FAIL: git user.name is not 'Emne'" >&2; exit 1; }
[ "$(git config user.email)" = "https://github.com/Emne-Eiji" ] || { echo "FAIL: git user.email is not the approved identity" >&2; exit 1; }

# 3. Every commit must carry that identity for both author and committer.
identities="$(git log --format='%an <%ae> / %cn <%ce>' | sort -u)"
expected='Emne <https://github.com/Emne-Eiji> / Emne <https://github.com/Emne-Eiji>'
if [ "$identities" != "$expected" ]; then
  echo "FAIL: unexpected identities in history:" >&2
  printf '%s\n' "$identities" >&2
  exit 1
fi

# 4. No remote may be configured.
[ -z "$(git remote)" ] || { echo "FAIL: a remote is configured: $(git remote)" >&2; exit 1; }

# 5. Single main branch.
branch="$(git branch --show-current)"
[ "$branch" = "main" ] || { echo "FAIL: current branch is '$branch', expected 'main'" >&2; exit 1; }

COMMIT="$(git rev-parse HEAD)"
BUNDLE="$OUT/araiguma-$NAME.bundle"
TARBALL="$OUT/araiguma-$NAME.tar.gz"
SHAFILE="$OUT/araiguma-$NAME.sha256"

# 6. Write recovery artifacts.
git bundle create "$BUNDLE" --all
git archive --format=tar.gz --prefix="araiguma/" -o "$TARBALL" HEAD
{
  echo "# araiguma recovery manifest — $NAME"
  echo "# commit: $COMMIT"
  echo "# branch: $branch"
  echo "# commits: $(git rev-list --count HEAD)"
  echo "# date: $(date -u +%Y-%m-%dT%H:%M:%SZ)"
  echo "# identity: Emne <https://github.com/Emne-Eiji> (author and committer, all commits)"
  echo
  (cd "$OUT" && sha256sum "$(basename "$BUNDLE")" "$(basename "$TARBALL")")
} > "$SHAFILE"

# 7. Verify the artifacts are readable.
git bundle verify "$BUNDLE" >/dev/null
tar -tzf "$TARBALL" >/dev/null
(cd "$OUT" && sha256sum -c "$(basename "$SHAFILE")" --quiet)

echo "milestone $NAME recorded:"
echo "  commit $COMMIT ($(git rev-list --count HEAD) commits)"
echo "  bundle  $(du -h "$BUNDLE" | cut -f1)  $BUNDLE"
echo "  archive $(du -h "$TARBALL" | cut -f1)  $TARBALL"
echo "  sha256  $SHAFILE"
