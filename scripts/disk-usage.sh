#!/usr/bin/env bash
# Report where storage is being used: repository, build trees, caches,
# and the persistent store. Read-only; safe to run at any time.
set -euo pipefail
cd "$(dirname "$0")/.."
# shellcheck source=scripts/env.sh disable=SC1091
source scripts/env.sh

section() { printf '\n== %s ==\n' "$1"; }

section "repository (persistent)"
du -sh "$ARIGUMA_ROOT"
(cd "$ARIGUMA_ROOT" && du -sh crates apps docs fixtures scripts 2>/dev/null || true)

section "ephemeral build state (/tmp)"
du -sh "$CARGO_TARGET_DIR" "$CARGO_HOME" "$ARIGUMA_NODE_MODULES_DIR" "$npm_config_cache" 2>/dev/null || true

section "toolchains (persistent)"
du -sh "$ARIGUMA_TOOLCHAINS" 2>/dev/null || true
du -sh "$ARIGUMA_TOOLCHAINS/cargo/registry/cache" 2>/dev/null || true

section "persistent project area ($(dirname -- "$ARIGUMA_ROOT"))"
(cd "$(dirname -- "$ARIGUMA_ROOT")" && du -sm .[!.]* * 2>/dev/null | sort -rn | head -15)

section "persistent store (/tmp/my-project)"
if [ -d /tmp/my-project ]; then
  du -sm /tmp/my-project 2>/dev/null
else
  echo "not mounted"
fi

section "filesystem"
df -h / /tmp 2>/dev/null | sed -n '1,3p'
