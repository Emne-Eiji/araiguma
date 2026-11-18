#!/usr/bin/env bash
# Full validation suite for Araiguma (WO-01R / WO-02 gate).
#
# Runs every check with full strictness; a red step fails the suite. Steps
# are appended over milestones — checks are never weakened to go green.
set -uo pipefail
cd "$(dirname "$0")/.."
# shellcheck source=scripts/env.sh disable=SC1091
source scripts/env.sh

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
fixtures_stable()   {
  ./scripts/gen-sysfs-fixtures.sh >/dev/null
  ./scripts/gen-topology-fixtures.sh >/dev/null
  git diff --exit-code -- fixtures/
}
rust_fmt()          { cargo fmt --all -- --check; }
rust_clippy()       { cargo clippy --workspace --all-targets -- -D warnings 2>&1 | tail -n 20; }
rust_tests()        { cargo test --workspace 2>&1 | tail -n 40; }
frontend_check()    { scripts/frontend.sh check 2>&1 | tail -n 4; }
frontend_unit()     { scripts/frontend.sh test 2>&1 | tail -n 8; }
frontend_build()    { scripts/frontend.sh build 2>&1 | tail -n 4; }
frontend_e2e()      { scripts/frontend.sh e2e 2>&1 | tail -n 10; }
dbus_harness()      { scripts/check-dbus-harness.sh; }
live_observation()  { scripts/live-check.sh 2>&1 | tail -n 20; }
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

step "durable-storage budget" durable_size
step "fixture regeneration is stable" fixtures_stable
step "cargo fmt --check" rust_fmt
step "cargo clippy -D warnings" rust_clippy
step "cargo test --workspace (incl. netns)" rust_tests
step "svelte-check" frontend_check
step "vitest" frontend_unit
step "frontend production build" frontend_build
step "playwright e2e" frontend_e2e
step "private D-Bus harness check" dbus_harness
step "live observation + iproute2 cross-validation" live_observation
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
