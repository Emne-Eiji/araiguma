#!/usr/bin/env bash
# Enforce the durable-storage budget (WO-01R §3.1): the persistent project
# footprint must stay below 950 MB so the sandbox persistence layer (boot
# restore cap: 1024 MB) can never drop project files again.
#
# The footprint is the parent of this repository (the persistent project
# area) minus the sandbox accounting repository's .git directory, which is
# infrastructure-owned and not synced to the store.
set -euo pipefail
cd "$(dirname "$0")/.."
# shellcheck source=scripts/env.sh disable=SC1091
source scripts/env.sh

PARENT="$(dirname -- "$ARIGUMA_ROOT")"
LIMIT_MB=950
WARN_MB=900

parent_kb="$(du -sk "$PARENT" 2>/dev/null | cut -f1)"
accounting_git_kb="$(du -sk "$PARENT/.git" 2>/dev/null | cut -f1 || echo 0)"
total_mb=$(( (parent_kb - accounting_git_kb) / 1024 ))
repo_mb="$(du -sm "$ARIGUMA_ROOT" 2>/dev/null | cut -f1)"
store_mb="not mounted"
if [ -d /tmp/my-project ]; then store_mb="$(du -sm /tmp/my-project 2>/dev/null | cut -f1) MB"; fi

printf 'durable footprint: %s MB total (araiguma repo %s MB, store %s) — limit %s MB\n' \
  "$total_mb" "$repo_mb" "$store_mb" "$LIMIT_MB"

if [ "$total_mb" -ge "$LIMIT_MB" ]; then
  echo "FAIL: durable footprint is ${total_mb} MB, at or above the ${LIMIT_MB} MB budget" >&2
  echo "Largest contributors:" >&2
  (cd "$PARENT" && du -sm .[!.]* * 2>/dev/null | sort -rn | head -12) >&2
  exit 1
fi
if [ "$total_mb" -ge "$WARN_MB" ]; then
  echo "WARN: durable footprint is within $((LIMIT_MB - WARN_MB)) MB of the budget" >&2
fi
exit 0
