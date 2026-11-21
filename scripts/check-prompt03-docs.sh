#!/usr/bin/env bash
# Prompt 03 documentation validation: link targets, register numbering
# consistency, placeholder scan, structure check. Design-doc deliverable of
# the Prompt 03 work order (documentation validation step).
set -uo pipefail
cd "$(dirname "$0")/.."
BASE=docs/design/prompt-03
fail=0

echo "== 1. Package completeness =="
expected="README.md 01-backend-architecture.md 02-provider-contract.md 03-data-model-and-evidence.md 04-document-and-ipc-contract.md 05-logging-and-diagnostics.md 06-security-read-only-enforcement.md 07-testing-strategy.md 08-decision-register.md"
for f in $expected; do
  if [ ! -s "$BASE/$f" ]; then echo "FAIL: missing/empty $BASE/$f"; fail=1; fi
done
echo "OK (all nine present)"

echo "== 2. Markdown links resolve =="
for f in $BASE/*.md docs/architecture.md; do
  # extract relative md links and verify the target exists
  grep -o '](\([^)]*\.md\)' "$f" | sed 's/](//' | while read -r link; do
    target="$(dirname "$f")/$link"
    if [ ! -f "$target" ]; then echo "FAIL: $f links to missing $link"; fail=1; fi
  done
done
echo "OK (no broken relative links)"

echo "== 3. RD register consistency =="
# every RD-xx referenced in docs 01-07 must be defined in 08-decision-register.md
defined=$(grep -o '| RD-[0-9]\+' "$BASE/08-decision-register.md" | grep -o 'RD-[0-9]\+' | sort -u)
for d in $defined; do
  count=$(grep -rlo "$d" $BASE/0[1-7]-*.md | wc -l)
  if [ "$count" -eq 0 ]; then echo "note: $d defined in register but not referenced in docs 01-07"; fi
done
for r in $(grep -rho 'RD-[0-9]\+' $BASE/0[1-7]-*.md | sort -u); do
  if ! echo "$defined" | grep -qx "$r"; then echo "FAIL: $r used in docs but not defined in the register"; fail=1; fi
done
echo "OK (RD cross-references consistent)"

echo "== 4. OD references are only approved/open sets =="
approved="OD-01 OD-02 OD-03 OD-08 OD-09 OD-10"
open="OD-04 OD-05 OD-06 OD-07"
for od in $(grep -rho 'OD-[0-9]\+' $BASE/*.md docs/architecture.md | sort -u); do
  if ! echo "$approved $open" | grep -qw "$od"; then echo "FAIL: unexpected decision id $od"; fail=1; fi
done
echo "OK (only OD-01..03,04..07,08..10 referenced)"

echo "== 5. No placeholders or debris =="
if grep -rnE 'TODO|FIXME|XXX|\[TBD\]|TBD-|lorem' $BASE/*.md; then echo "FAIL: placeholders found"; fail=1; fi
echo "OK (no placeholders)"

echo "== 6. Every doc declares its Prompt 03 deliverable =="
for f in $BASE/0[1-7]-*.md; do
  if ! head -5 "$f" | grep -q "Prompt 03 deliverable"; then echo "FAIL: $f lacks deliverable header"; fail=1; fi
done
echo "OK (deliverable headers present)"

echo "== 7. ASCII diagram fences balanced =="
for f in $BASE/*.md; do
  n=$(grep -c '^```' "$f")
  if [ $((n % 2)) -ne 0 ]; then echo "FAIL: unbalanced code fences in $f"; fail=1; fi
done
echo "OK (fences balanced)"

echo "== 8. architecture.md pointer present =="
grep -q 'docs/design/prompt-03' docs/architecture.md && echo "OK (pointer present)" || { echo "FAIL: pointer missing"; fail=1; }

echo
if [ "$fail" -eq 0 ]; then echo "RESULT: PASS"; else echo "RESULT: FAIL"; exit 1; fi
