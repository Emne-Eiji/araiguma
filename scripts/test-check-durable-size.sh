#!/usr/bin/env bash
# Self-tests for scripts/check-durable-size.sh (WO-02 correction §2).
#
# Runs the checker against tiny synthetic trees with a small threshold so
# every behavior is exercised cheaply: both measurements below/above the
# budget, the warning band, the missing store (reported explicitly, never
# treated as zero bytes), the accounting-.git exclusion, and unreadable
# paths. No real filesystem location is modified; everything lives under a
# temporary directory that is removed on exit.
set -uo pipefail
HERE="$(cd -- "$(dirname -- "$0")" && pwd)"
CHECKER="$HERE/check-durable-size.sh"
TMP="$(mktemp -d /tmp/araiguma-durable-test.XXXXXX)"
trap 'rm -rf "$TMP"' EXIT

pass=0
fail=0

# assert_case <name> <expected-exit> <pattern> <checker args...>
assert_case() {
  local name="$1" expected="$2" pattern="$3"
  shift 3
  local output exit_code
  output="$("$CHECKER" "$@" 2>&1)"
  exit_code=$?
  if [ "$exit_code" -eq "$expected" ] && printf '%s' "$output" | grep -q "$pattern"; then
    pass=$((pass + 1))
    printf 'PASS  %s\n' "$name"
  else
    fail=$((fail + 1))
    printf 'FAIL  %s\n      expected exit %s matching "%s", got exit %s\n      output: %s\n' \
      "$name" "$expected" "$pattern" "$exit_code" "$output"
    printf '      command: %s\n' "$*"
  fi
}

# mb_file <path> <size-in-mb> — a real (non-sparse) file of the given size.
mb_file() {
  mkdir -p "$(dirname -- "$1")"
  dd if=/dev/zero of="$1" bs=1M count="$2" status=none
}

# ── Fixtures ────────────────────────────────────────────────────────────────
# working: 1 MB of content. store-ok: 1 MB. store-big: 6 MB. working-big: 6 MB.
mb_file "$TMP/working/data.bin" 1
mb_file "$TMP/store-ok/data.bin" 1
mb_file "$TMP/store-big/data.bin" 6
mb_file "$TMP/working-big/data.bin" 6
# working-near: 4 MB with a 1 MB accounting .git that must be EXCLUDED,
# so the measured footprint is 3 MB (near but below a 4 MB limit).
mb_file "$TMP/working-near/data.bin" 3
mb_file "$TMP/working-near/.git/objects/pack.bin" 1

# ── Required cases ──────────────────────────────────────────────────────────
# 1. Working directory below threshold, store below threshold.
assert_case "both below budget"            0 "RESULT: OK" \
  --root "$TMP/working" --store "$TMP/store-ok" --limit 5

# 2. Working directory above threshold (store fine): the failure must be
#    attributed to the working footprint.
assert_case "working above budget"         1 "FAIL: working footprint is 6 MB" \
  --root "$TMP/working-big" --store "$TMP/store-ok" --limit 5

# 3. Persistent store above threshold (working fine): attributed to the store.
assert_case "store above budget"           1 "FAIL: persistent store is 6 MB" \
  --root "$TMP/working" --store "$TMP/store-big" --limit 5

# 4. Persistent store unavailable (path absent): reported explicitly as not
#    present, NOT treated as zero bytes, and not a failure on its own.
assert_case "store unavailable (reported)" 0 "persistent store: not present on this system" \
  --root "$TMP/working" --store "$TMP/no-such-store" --limit 5

# 5. Both measurements near the threshold: warning emitted, still passing.
#    working-near measures 3 MB (the 1 MB .git is excluded) with limit 4,
#    warn 3 → the warning band.
assert_case "near threshold warns"         0 "WARN: working footprint is within" \
  --root "$TMP/working-near" --store "$TMP/store-ok" --limit 4 --warn-mb 3

# 6. Accounting .git exclusion is actually applied (3 MB measured, not 4).
assert_case "accounting .git excluded"     0 "working footprint: 3 MB" \
  --root "$TMP/working-near" --store "$TMP/no-such-store" --limit 5

# 7. Both over budget: both failures identified in one run.
assert_case "both above budget"            1 "FAIL: persistent store is 6 MB" \
  --root "$TMP/working-big" --store "$TMP/store-big" --limit 5

# 8. Store exists but cannot be measured (unreadable): a hard failure —
#    never silently zero. (Skipped when running as root, which reads
#    everything regardless of mode.)
if [ "$(id -u)" -ne 0 ]; then
  mkdir -p "$TMP/store-locked"
  chmod 000 "$TMP/store-locked"
  assert_case "unreadable store fails"     1 "not readable" \
    --root "$TMP/working" --store "$TMP/store-locked" --limit 5
  chmod 755 "$TMP/store-locked"
else
  echo "SKIP  unreadable store (running as root)"
fi

# 9. An over-budget measurement lists the largest contributors (diagnosis).
assert_case "failure lists contributors"   1 "Largest contributors" \
  --root "$TMP/working-big" --store "$TMP/no-such-store" --limit 5

echo
printf 'durable-size self-tests: %d passed, %d failed\n' "$pass" "$fail"
[ "$fail" -eq 0 ]
