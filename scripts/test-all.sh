#!/usr/bin/env bash
# Full validation suite for Araiguma (WO-01R / WO-02 gate; extended by the
# WO-02 correction and WO-03 work orders).
#
# Runs every check with full strictness; a red step fails the suite. Steps
# are appended over milestones — checks are never weakened to go green.
#
# Test accounting note (WO-02 correction §10): the netns integration tests
# re-execute themselves under `unshare -Urn` with `--exact`, so their child
# processes print their own "test result" lines ("... 2 filtered out")
# asynchronously. Summing those lines is therefore NOT a reliable count.
# The runner below captures the complete log and counts only harness
# summaries without "filtered out", which excludes the re-exec children and
# yields the deterministic total of parent-discovered tests.
set -uo pipefail
cd "$(dirname "$0")/.."
# shellcheck source=scripts/env.sh disable=SC1091
source scripts/env.sh

LOG_DIR="${ARIGUMA_TEST_LOG_DIR:-/tmp/araiguma-test-logs}"
mkdir -p "$LOG_DIR"

results=()
failed=0

step() { # step <label> <function>
  local label="$1" fn="$2"
  echo
  printf '════ %s ════\n' "$label"
  if "$fn"; then
    results+=("PASS  $label")
  else
    results+=("FAIL  $label")
    failed=1
  fi
}

durable_size()      { scripts/check-durable-size.sh; }
durable_selftest()  { scripts/test-check-durable-size.sh; }
scripts_syntax()    {
  local bad=0
  for script in scripts/*.sh; do
    if ! bash -n "$script"; then
      echo "syntax error in $script" >&2
      bad=1
    fi
  done
  if command -v shellcheck >/dev/null 2>&1; then
    # Advisory when available; failures above are only from bash -n.
    shellcheck scripts/*.sh || echo "note: shellcheck findings above (advisory)"
  fi
  return "$bad"
}
fixtures_stable()   {
  ./scripts/gen-sysfs-fixtures.sh >/dev/null
  ./scripts/gen-topology-fixtures.sh >/dev/null
  git diff --exit-code -- fixtures/
}
rust_fmt()          { cargo fmt --all -- --check; }
rust_clippy()       {
  cargo clippy --workspace --all-targets -- -D warnings > "$LOG_DIR/clippy.log" 2>&1
  local rc=$?
  tail -n 20 "$LOG_DIR/clippy.log"
  return "$rc"
}
rust_tests()        {
  local log="$LOG_DIR/cargo-test.log"
  cargo test --workspace 2>&1 | tee "$log"
  local harness_rc="${PIPESTATUS[0]}"
  echo
  if [ "$harness_rc" -ne 0 ]; then
    echo "cargo test exit status: $harness_rc" >&2
    return 1
  fi
  # Deterministic count: parent harness summaries only (children of the
  # netns re-exec pattern carry a nonzero "filtered out" and are excluded).
  local passed failed_tests
  passed="$(grep -E '^test result: ok\.' "$log" | grep -v ' [1-9][0-9]* filtered out' \
    | sed -E 's/^test result: ok\. ([0-9]+) passed.*/\1/' | awk '{s+=$1} END {print s+0}')"
  failed_tests="$(grep -E '^test result: FAILED\.' "$log" \
    | sed -E 's/^test result: FAILED\. ([0-9]+) failed.*/\1/' | awk '{s+=$1} END {print s+0}')"
  echo
  echo "workspace tests: ${passed} passed (parent summaries), ${failed_tests} failed"
  [ "$failed_tests" -eq 0 ]
}
frontend_check()    { scripts/frontend.sh check 2>&1 | tail -n 4; }
frontend_unit()     { scripts/frontend.sh test 2>&1 | tail -n 8; }
frontend_build()    { scripts/frontend.sh build 2>&1 | tail -n 4; }
frontend_e2e()      { scripts/frontend.sh e2e 2>&1 | tail -n 10; }
frontend_audit()    {
  # npm audit on the frontend dependency tree. The suite fails on any
  # vulnerability so a finding is always a conscious decision, never an
  # unnoticed drift. (Runtime exposure is the two production deps:
  # svelte + cytoscape; everything else is build/test tooling.)
  scripts/frontend.sh prepare >/dev/null 2>&1
  (cd /tmp/araiguma-work/apps/frontend && npm audit --omit=dev 2>&1 | tail -n 6)
  local full
  full="$(cd /tmp/araiguma-work/apps/frontend && npm audit 2>&1)"
  if printf '%s' "$full" | grep -qE 'found [1-9][0-9]* vulnerabilities'; then
    echo "npm audit reports vulnerabilities — assess before proceeding:" >&2
    printf '%s\n' "$full" | tail -n 20 >&2
    return 1
  fi
  echo "npm audit: 0 vulnerabilities (including dev dependencies)"
}
dbus_harness()      { scripts/check-dbus-harness.sh; }
live_observation()  { scripts/live-check.sh 2>&1 | tail -n 20; }
frontend_live_docs() {
  # WO-03 M1 integration: the real CLI documents exported by the live
  # step must import cleanly through the actual frontend validators.
  [ -d /tmp/araiguma-live-docs ] || { echo "no live documents exported" >&2; return 1; }
  ARAIGUMA_LIVE_DOCS=/tmp/araiguma-live-docs scripts/frontend.sh test 2>&1 | tail -n 6
}
git_identity() {
  local problems=""
  [ "$(git config user.name)" = "Emne" ] || problems+="user.name wrong; "
  [ "$(git config user.email)" = "https://github.com/Emne-Eiji" ] || problems+="user.email wrong; "
  [ -z "$(git remote)" ] || problems+="remote configured; "
  [ -z "$(git status --porcelain)" ] || problems+="working tree not clean; "
  local identities
  identities="$(git log --format='%an <%ae> / %cn <%ce>' | sort -u)"
  [ "$identities" = "Emne <https://github.com/Emne-Eiji> / Emne <https://github.com/Emne-Eiji>" ] \
    || problems+="unexpected commit identities: $identities"
  if [ -n "$problems" ]; then
    echo "git identity audit: $problems" >&2
    return 1
  fi
  echo "git identity audit: OK ($(git rev-list --count HEAD) commits, all by Emne <https://github.com/Emne-Eiji>, no remote, clean tree)"
}

step "durable-storage budget (working + persistent store)" durable_size
step "durable-size checker self-tests" durable_selftest
step "shell script syntax checks" scripts_syntax
step "fixture regeneration is stable" fixtures_stable
step "cargo fmt --check" rust_fmt
step "cargo clippy -D warnings" rust_clippy
step "cargo test --workspace (incl. netns)" rust_tests
step "svelte-check" frontend_check
step "vitest" frontend_unit
step "frontend production build" frontend_build
step "playwright e2e" frontend_e2e
step "npm audit" frontend_audit
step "private D-Bus harness check" dbus_harness
step "live observation + iproute2 cross-validation" live_observation
step "live CLI documents through the frontend validators" frontend_live_docs
step "git identity audit" git_identity

echo
echo "════ summary ════"
printf '  %s\n' "${results[@]}"
echo
if [ "$failed" -ne 0 ]; then
  echo "RESULT: FAIL" >&2
  exit 1
fi
echo "RESULT: PASS"
