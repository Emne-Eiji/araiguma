#!/usr/bin/env bash
# Enforce the durable-storage budget (WO-01R §3.1, corrected by the WO-02
# correction work order §2): BOTH the working project footprint AND the
# persistent store must stay below the safety threshold, so the sandbox
# persistence layer (boot restore cap: 1024 MB) can never drop project files
# again. The original repository loss was caused by the persistent store
# exceeding that capacity — measuring only the working directory (the
# pre-correction behavior) would not have caught it.
#
# Measurements:
#   1. working footprint  = parent of this repository (the persistent project
#      area) minus the sandbox accounting repository's .git directory, which
#      is infrastructure-owned and not synced to the store;
#   2. persistent store   = /tmp/my-project (the sandbox persistence-layer
#      mount) when it exists on this system.
#
# Both measurements are enforced against LIMIT_MB (default 950, the
# established safety target). A missing store is reported explicitly and is
# NOT treated as zero bytes: on systems where the store does not exist
# (e.g. a real development machine), only the working footprint applies.
#
# Testable: --root, --store, --limit, --warn-mb override the measured
# locations and thresholds (see scripts/test-check-durable-size.sh).
#
# Exit status: 0 = within budget (possibly with a warning near it),
#              1 = a measurement failed the budget, 2 = usage error.
set -uo pipefail

ROOT=""
STORE=""
LIMIT_MB=950
WARN_MB=""

usage() {
  cat >&2 <<'USAGE'
usage: check-durable-size.sh [--root DIR] [--store DIR] [--limit MB] [--warn-mb MB]

  --root DIR    working project area to measure (default: repository parent)
  --store DIR   persistent store to measure (default: /tmp/my-project);
                a nonexistent path is reported as "not present on this
                system" and not enforced
  --limit MB    failure threshold in MB (default: 950)
  --warn-mb MB  warning threshold in MB (default: limit minus 50)
USAGE
}

while [ $# -gt 0 ]; do
  case "$1" in
    --root)    [ $# -ge 2 ] || { usage; exit 2; }; ROOT="$2"; shift 2 ;;
    --store)   [ $# -ge 2 ] || { usage; exit 2; }; STORE="$2"; shift 2 ;;
    --limit)   [ $# -ge 2 ] || { usage; exit 2; }; LIMIT_MB="$2"; shift 2 ;;
    --warn-mb) [ $# -ge 2 ] || { usage; exit 2; }; WARN_MB="$2"; shift 2 ;;
    -h|--help) usage; exit 0 ;;
    *) usage; exit 2 ;;
  esac
done

if [ -z "$ROOT" ]; then
  cd "$(dirname "$0")/.." || exit 2
  # shellcheck source=scripts/env.sh disable=SC1091
  source scripts/env.sh
  ROOT="$(dirname -- "$ARIGUMA_ROOT")"
fi
if [ -z "$STORE" ]; then
  STORE="/tmp/my-project"
fi
case "$LIMIT_MB" in
  ''|*[!0-9]*) echo "FAIL: --limit must be a positive integer (got '$LIMIT_MB')" >&2; exit 2 ;;
esac
if [ -z "$WARN_MB" ]; then
  WARN_MB=$(( LIMIT_MB > 50 ? LIMIT_MB - 50 : 0 ))
fi
case "$WARN_MB" in
  ''|*[!0-9]*) echo "FAIL: --warn-mb must be a positive integer (got '$WARN_MB')" >&2; exit 2 ;;
esac

failed=0

# measure <label> <path> <exclude-path-or-"">
# Prints "<label>: <MB> MB" and enforces the threshold. Returns nonzero on
# budget failure. A path that cannot be measured at all is a hard failure —
# it must never be silently treated as zero bytes.
measure() {
  local label="$1" path="$2" exclude="$3"
  if [ ! -e "$path" ]; then
    echo "$label: not present on this system ($path) — not measured, not treated as zero"
    return 0
  fi
  if [ ! -r "$path" ]; then
    echo "FAIL: $label exists but is not readable ($path) — cannot verify the budget" >&2
    failed=1
    return 0
  fi
  local kb total
  kb="$(du -sk "$path" 2>/dev/null | cut -f1)" || {
    echo "FAIL: $label could not be measured (du failed on $path)" >&2
    failed=1
    return 0
  }
  if [ -n "$exclude" ] && [ -e "$exclude" ]; then
    local exclude_kb
    exclude_kb="$(du -sk "$exclude" 2>/dev/null | cut -f1 || echo 0)"
    total=$(( (kb - exclude_kb) / 1024 ))
  else
    total=$(( kb / 1024 ))
  fi
  echo "$label: ${total} MB (limit ${LIMIT_MB} MB)"
  if [ "$total" -ge "$LIMIT_MB" ]; then
    echo "FAIL: $label is ${total} MB, at or above the ${LIMIT_MB} MB budget" >&2
    if [ -d "$path" ]; then
      echo "Largest contributors:" >&2
      (cd "$path" && du -sm .[!.]* * 2>/dev/null | sort -rn | head -12) >&2
    fi
    failed=1
  elif [ "$total" -ge "$WARN_MB" ]; then
    echo "WARN: $label is within $(( LIMIT_MB - WARN_MB )) MB of the budget" >&2
  fi
}

echo "durable-storage budget check (limit ${LIMIT_MB} MB)"
measure "working footprint" "$ROOT" "$ROOT/.git"
measure "persistent store" "$STORE" ""

if [ "$failed" -ne 0 ]; then
  echo "RESULT: FAIL — a durable measurement is over budget; see above" >&2
  exit 1
fi
echo "RESULT: OK"
exit 0
