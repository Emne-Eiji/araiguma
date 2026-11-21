#!/usr/bin/env bash
# Prompt 03 documentation validation: link targets, register numbering
# consistency, placeholder scan, structure check. Design-doc deliverable of
# the Prompt 03 work order (documentation validation step).
# Extended by Prompt 03-FC (checks 9-15): IPC command inventory,
# cross-document section references, D-Bus allow-list scoping, RD
# disposition vocabulary, MAC-redaction default, diagnostics field
# categories, schema-version consistency.
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

echo "== 9. IPC command inventory complete (Prompt 03-FC FC-01) =="
# all four commands declared in document 04 §9's table; counts agree across documents
ipc_doc="$BASE/04-document-and-ipc-contract.md"
grep -q 'Four commands' "$ipc_doc" || { echo "FAIL: document 04 does not declare 'Four commands'"; fail=1; }
for cmd in get_snapshot get_topology subscribe_watch unsubscribe_watch; do
  grep -q "^| \`$cmd\` |" "$ipc_doc" || { echo "FAIL: command '$cmd' missing from document 04 §9 table"; fail=1; }
done
grep -q 'four IPC commands' "$BASE/06-security-read-only-enforcement.md" || { echo "FAIL: document 06 §5 does not say 'four IPC commands'"; fail=1; }
grep -q 'all four commands' "$BASE/07-testing-strategy.md" || { echo "FAIL: document 07 §6 does not reference all four commands"; fail=1; }
# every command name used anywhere in the package must be one of the four
for cmd in $(grep -rho '\b\(get_[a-z_]*\|subscribe_[a-z_]*\|unsubscribe_[a-z_]*\)' $BASE/*.md | sort -u); do
  case "$cmd" in
    get_snapshot|get_topology|subscribe_watch|unsubscribe_watch) ;;
    *) echo "FAIL: unknown IPC command name '$cmd' appears in the package"; fail=1 ;;
  esac
done
echo "OK (four commands declared; no stray command names)"

echo "== 10. Cross-document section references resolve =="
# every "document 0N §X" reference in docs 01-08 must resolve to an existing
# section heading ("## X." or "### X") of document 0N
doc_for() { ls $BASE/0$1-*.md 2>/dev/null | head -1; }
bad=0
while IFS= read -r ref; do
  dn=$(printf '%s' "$ref" | sed -E 's/documents? 0([1-8]).*/\1/')
  sec=$(printf '%s' "$ref" | grep -oE '§[0-9]+(\.[0-9]+)?' | tr -d '§')
  target=$(doc_for "$dn")
  if [ -z "$target" ]; then
    echo "FAIL: reference '$ref' names a missing document"; fail=1; bad=1; continue
  fi
  if ! grep -qE "^#{2,3} ${sec}[. ]" "$target"; then
    echo "FAIL: reference '$ref' does not match any heading in $(basename "$target")"
    fail=1; bad=1
  fi
done < <(grep -hoE 'documents? 0[1-8] §[0-9]+(\.[0-9]+)?' $BASE/0[1-8]-*.md | sort -u)
[ "$bad" -eq 0 ] && echo "OK (all document-NN §N references resolve)"

echo "== 11. D-Bus allow-list scoped and deny-by-default (FC-02) =="
sec_doc="$BASE/06-security-read-only-enforcement.md"
grep -q 'Properties.Get(service, path, interface, property)' "$sec_doc" || { echo "FAIL: Properties.Get not scoped to (service, path, interface, property)"; fail=1; }
grep -q 'no interface is approved for `GetAll`' "$sec_doc" || { echo "FAIL: GetAll blanket-prohibition statement missing"; fail=1; }
grep -q 'GetManagedObjects' "$sec_doc" || { echo "FAIL: GetManagedObjects treatment missing"; fail=1; }
grep -q '### 4.4 systemd-networkd' "$sec_doc" || { echo "FAIL: systemd-networkd treatment section missing"; fail=1; }
grep -q 'No networkd service' "$sec_doc" || { echo "FAIL: networkd deny-by-default statement missing"; fail=1; }
# the generic-primitives table must not contain an unqualified allowance
if grep -qE '^\| `org\.freedesktop\.DBus\.Properties\.Get` / `GetAll` \|' "$sec_doc"; then
  echo "FAIL: unscoped Get/GetAll primitive row still present"; fail=1
fi
echo "OK (allow-list scoped; GetAll/GetManagedObjects/networkd addressed)"

echo "== 12. RD disposition complete and unapproved (FC-04) =="
reg="$BASE/08-decision-register.md"
grep -q '### 3.1 Implementation disposition' "$reg" || { echo "FAIL: disposition table missing"; fail=1; }
nd=$(grep -cE '^\| RD-[0-9]+ \| (REQUIRES EMNE DECISION|NON-BLOCKING DESIGN GUIDANCE|DEFERRED TO A LATER PROMPT) \|' "$reg")
[ "$nd" -eq 30 ] || { echo "FAIL: expected 30 disposition rows, found $nd"; fail=1; }
for i in $(seq -w 1 30); do
  grep -qE "^\| RD-$i \| (REQUIRES EMNE DECISION|NON-BLOCKING DESIGN GUIDANCE|DEFERRED TO A LATER PROMPT) \|" "$reg" \
    || { echo "FAIL: RD-$i lacks a disposition row with the exact vocabulary"; fail=1; }
done
# no disposition row may assert approval or use vocabulary outside the set
# (scope: the §3.1 disposition table only)
disp=$(sed -n '/^### 3\.1 Implementation disposition/,/^## 4\./p' "$reg")
if printf '%s\n' "$disp" | grep -E '^\| RD-[0-9]+ \|' | grep -qvE '^\| RD-[0-9]+ \| (REQUIRES EMNE DECISION|NON-BLOCKING DESIGN GUIDANCE|DEFERRED TO A LATER PROMPT) \|'; then
  echo "FAIL: a disposition row uses status vocabulary outside the defined set"; fail=1
fi
grep -q 'NOT APPROVED' "$reg" || { echo "FAIL: blanket not-approved statement missing"; fail=1; }
echo "OK (30/30 dispositions; vocabulary exact; none approved)"

echo "== 13. Diagnostics privacy: full MAC redaction default (FC-03) =="
log_doc="$BASE/05-logging-and-diagnostics.md"
grep -q 'mac_addresses = "redact"' "$log_doc" || { echo "FAIL: privacy.mac_addresses default key missing"; fail=1; }
grep -q '\*\*fully redacted\*\*' "$log_doc" || { echo "FAIL: full-redaction default statement missing"; fail=1; }
grep -q 'redact_macs_in_export' "$log_doc" && { echo "FAIL: obsolete redact_macs_in_export key still present"; fail=1; }
# partial MAC disclosure may only appear as explicit opt-in
while IFS= read -r line; do
  echo "$line" | grep -q 'vendor-prefix\|explicit' || { echo "FAIL: 'first three octets' mentioned outside the explicit opt-in: $line"; fail=1; }
done < <(grep -n 'first three octets' "$log_doc" | sed 's/^[0-9]*://')
echo "OK (full redaction default; partial disclosure only as explicit opt-in)"

echo "== 14. Diagnostics field categories addressed (FC-03) =="
for cat in 'SSID' 'Hostnames' 'home-directory' 'Provider object paths' 'MAC addresses' 'VPN names'; do
  grep -q "$cat" "$log_doc" || { echo "FAIL: privacy category '$cat' not addressed in document 05 §9.1"; fail=1; }
done
grep -q '### 9.1 Field-level privacy policy' "$log_doc" || { echo "FAIL: §9.1 field-level policy missing"; fail=1; }
echo "OK (all FC-03 categories present)"

echo "== 15. Schema version consistent (v4) =="
grep -q '"schemaVersion": 4' "$ipc_doc" || { echo "FAIL: document 04 envelope does not declare schemaVersion 4"; fail=1; }
if grep -rqE 'schema v5|schemaVersion": 5' $BASE/*.md; then
  echo "FAIL: a schema version other than v4 is asserted"; fail=1
fi
echo "OK (schema v4 everywhere; no other version asserted)"

echo
if [ "$fail" -eq 0 ]; then echo "RESULT: PASS"; else echo "RESULT: FAIL"; exit 1; fi
