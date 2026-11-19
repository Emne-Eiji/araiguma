# WO-03-F — Full Adversarial Audit Remediation: Closeout and Traceability

**Work order:** WO-03-F
**Project:** Araiguma
**Authority:** Emne — sole product owner and acceptance authority
**Implementation agent:** GLM 5.3 Max
**Scope:** All findings AUD-01 through AUD-14 plus the Section 5 fidelity
review, cross-cutting engineering requirements, three consecutive full
validation cycles, and this traceability record.
**Date:** 2026-11-19
**Branch:** `main` (single branch, no remote configured, nothing pushed)

## Status summary

| Finding | Severity | Status |
|---|---|---|
| AUD-01 | P1 | FIXED AND VERIFIED |
| AUD-02 | P2 | FIXED AND VERIFIED |
| AUD-03 | P2 | FIXED AND VERIFIED |
| AUD-04 | P2 | FIXED AND VERIFIED |
| AUD-05 | P3 | FIXED AND VERIFIED |
| AUD-06 | P3 | FIXED AND VERIFIED |
| AUD-07 | P3 | FIXED AND VERIFIED |
| AUD-08 | P3 | FIXED AND VERIFIED |
| AUD-09 | P3 | FIXED AND VERIFIED |
| AUD-10 | P3 | FIXED AND VERIFIED |
| AUD-11 | P3 | FIXED AND VERIFIED |
| AUD-12 | P3 | FIXED AND VERIFIED |
| AUD-13 | P3 | FIXED AND VERIFIED (constructed-message path; no live kernel instance possible on this sandbox) |
| AUD-14 | P3 | FIXED AND VERIFIED |
| §5.1 | review | REVIEWED — NO DEFECT (scope documented) |
| §5.2 | review | FIXED AND VERIFIED |
| §5.3 | review | FIXED AND VERIFIED (with AUD-10) |
| §5.4 | review | FIXED AND VERIFIED (with AUD-01) |
| §5.5 | review | FIXED AND VERIFIED (with AUD-02/03/05/06; plus diff-key completion and the table-not-observed display group) |

All findings are resolved; none is deferred. Remaining limitations are
listed per finding and consolidated at the end — none is hidden.

## Traceability

### AUD-01 — Cross-namespace veth false topology edge (P1)

- **Original defect:** veth peer resolution treated a peer ifindex as
  globally unique; an unrelated local interface with a colliding ifindex
  was connected with a `vethPeer` edge presented as netlink evidence.
- **Root cause:** ifindex is namespace-scoped; the model carried no
  namespace context for the link target, and the topology matched a bare
  local ifindex.
- **Reproduction:** live, with the real CLI binary, inside a transient
  network namespace — a veth end moved into a child namespace whose
  ifindex collided with a local dummy produced a false edge
  (`scripts/wo03f-reproduce-defects.sh` in the workspace evidence; the
  same scenario is now a permanent test).
- **Affected components:** `core::model::Interface`, `netlink::translate`,
  `core::topology`, serialization, frontend types/validators/panels.
- **Correction:** the kernel's `IFLA_LINK_NETNSID` is captured verbatim
  (`link_netnsid`, serde-defaulted). Evidence rules: a netnsid states the
  peer/parent lives in another namespace — never a local edge, annotation
  only; without a netnsid a same-namespace pair is drawn only on the
  kernel's MUTUAL statements (the candidate at the peer ifindex must
  itself be a veth naming this interface back). Cross-namespace vlan and
  tunnel parents get the same protection. Live-verified on kernel 5.10:
  cross-namespace targets always carry an allocated netnsid;
  same-namespace targets never do.
- **Regression tests:** unit (`crates/core/tests/topology.rs`: 8 tests
  covering mutual pairing, netnsid collision with dummy and veth,
  missing-namespace-information, asymmetry, self-reference, serialization)
  plus live netns tests reproducing both adversarial scenarios
  (`netns_cross_namespace_veth_peer_never_draws_a_local_edge`,
  `netns_cross_namespace_peer_without_collision_stays_unresolved`);
  frontend validation and display tests.
- **Commits:** `8b27065`.
- **Remaining limitations:** the mutual-statement rule relies on the
  live-verified kernel behavior that cross-namespace link targets always
  carry a netnsid at dump time (kernel 5.10). A hypothetical kernel that
  emits no netnsid for a cross-namespace target while a local interface
  collides AND mirrors the peer statement would still draw the pair; the
  evidence chain is documented in `docs/discovery-notes.md` item 15.

### AUD-02 — Multipath route merge collapses distinct kernel routes (P2)

- **Original defect:** the merge treated overlapping next-hop sets as
  proof of route identity; two `ip route append` ECMP routes under one
  superficial key collapsed, silently losing a unique next hop.
- **Root cause:** identity keyed on (family, table, destination, metric)
  plus ANY-hop overlap — overlap is not identity.
- **Reproduction:** live, with the real binary: the appended pair
  collapsed to one route and the second route's hop disappeared; the
  kernel demonstrably holds both (`ip -j route` cross-check).
- **Affected components:** `core::merge` (redesigned), `core::model::Route`
  (new `tos`), `netlink::translate` (rtm_tos), diff key, frontend.
- **Correction:** two-phase merge. Phase A: two netlink observations are
  the same route only when every observed discriminator agrees (family,
  destination, table, metric, protocol, scope, type, TOS, flags) AND the
  complete hop representations (gateway, via, oif, weight, hop flags) are
  equal. Phase B: a procfs row reconciles with a netlink route only when
  it is the UNIQUE unused match (first-hop containment, table
  requirement). Multiple matches are never resolved by iteration order.
- **Regression tests:** `overlapping_multipath_routes_stay_separate`
  (the audit shape), `routes_are_distinguished_by_tos_and_identical_ones_merge`,
  live `netns_appended_same_key_ecmp_routes_all_survive_the_merge`
  (kernel cross-checked), existing first-hop reconciliation retained.
- **Commits:** `4f8c01b`.
- **Remaining limitations:** none known.

### AUD-03 — Duplicate table-less IPv6 procfs routes collapse (P2)

- **Original defect:** identical `/proc/net/ipv6_route` rows merged into
  one observation, losing multiplicity (the kernel prints its
  unreachable-default sentinel twice, one per table, with no table
  column).
- **Root cause:** procfs rows deduplicated against each other on
  identical content although the source cannot establish identity.
- **Reproduction:** live — the file contains two identical sentinel rows;
  the merged snapshot carried one.
- **Correction:** procfs rows never merge with each other; each row
  reconciles with at most one netlink route; identical standalone rows
  are preserved with an explicit multiplicity note ("appears N times …
  may be distinct routes in different tables — multiplicity preserved").
- **Regression tests:** `ipv6_procfs_only_observation_keeps_unknown_table_and_row_multiplicity`
  (updated from the pre-correction dedup assertion — the test name and
  body now pin the honest behavior), live sentinel assertions
  (`netns_merge_with_sysfs_fallback` now expects both rows plus notes),
  `duplicate_procfs_rows_reconcile_once_and_keep_the_remainder`.
- **Commits:** `4f8c01b`.
- **Remaining limitations:** none known.

### AUD-04 — Fresh-clone fixture generation is not self-sufficient (P2)

- **Original defect:** four empty fixture directories are untrackable by
  Git; a fresh clone failed three sysfs tests until the generator was run
  manually, and the documented bootstrap never ran it.
- **Root cause:** bootstrap omitted fixture generation; empty directories
  cannot be committed.
- **Reproduction:** the baseline run of this work order — a fresh clone
  from the WO-03 bundle failed exactly `basic_workstation_interfaces`,
  `virtual_interfaces_classification`, `empty_observation_is_valid`.
- **Correction:** `bootstrap.sh` generates the trees and verifies every
  empty-directory fixture afterwards (failure is loud); the generator
  refuses to run outside a repository fixtures tree (fixed write scope);
  `check-fresh-clone.sh` clones the COMMITTED state, runs the documented
  bootstrap, passes the test suites with no manual preparation, and
  re-verifies byte-stable regeneration; the check is part of
  `test-all.sh`. `docs/development.md` documents the one setup path.
- **Regression tests:** the fresh-clone check itself (mandatory test
  category satisfied by construction), fixture-stability step, generator
  safety guard.
- **Commits:** `ea812e9`, `07a3fb8`.
- **Remaining limitations:** none known.

### AUD-05 — Ambiguous IPv6 procfs provenance assigned arbitrarily (P3)

- **Original defect:** a table-less row matching multiple netlink routes
  was attributed to the first match by iteration order.
- **Correction:** multiple candidate matches keep the row independent
  with an explicit ambiguity note; no attribution is invented; the
  result is provably order-independent.
- **Regression tests:** `ambiguous_procfs_row_never_picks_a_candidate_by_order`
  (asserts identical output under reordered providers and routes), the
  live append test (both rows ambiguous, neither attributed).
- **Commits:** `4f8c01b`.
- **Remaining limitations:** none known.

### AUD-06 — `primary_source()` fabricates Netlink provenance (P3)

- **Original defect:** empty source lists fell back to
  `ObservationSource::Netlink`, inventing provenance (two call sites).
- **Correction:** `primary_source` returns `Option`; an unattributed
  kernel statement renders as an explicitly unattributed relationship
  (defense in depth), and the document boundary REJECTS observations
  with empty `sources` with a path-qualified reason (Rust projector and
  frontend validators, parity).
- **Regression tests:** `unattributed_observations_never_fabricate_a_provider`
  (core), `observations_without_provenance_are_rejected_at_the_boundary`
  (CLI), frontend validator tests for interfaces/addresses/routes.
- **Commits:** `bb55295`.
- **Remaining limitations:** gateway nodes are routing-derived facts, not
  observations; their empty detail sources are presentation metadata,
  not provenance claims.

### AUD-07 — Malformed procfs lines silently disappear (P3)

- **Original defect:** unparseable lines were skipped without trace.
- **Correction:** every rejected line produces a diagnostic (1-based line
  number + reason; raw content deliberately not echoed) surfaced as a
  provider note; empty file = valid zero observation, no note; partially
  parsed = visible note; missing/unreadable = Failed section.
- **Regression tests:** `empty_files_are_valid_zero_observations_without_notes`,
  `malformed_ipv6_route_rows_are_counted_and_noted`,
  `malformed_if_inet6_and_route_rows_are_noted_separately`,
  `entirely_malformed_file_is_observed_empty_with_the_reason`,
  `unreadable_files_fail_the_section_with_the_error`.
- **Commits:** `bb55295`.
- **Remaining limitations:** permission-denied is exercised through an
  unreadable-path error (EISDIR) rather than a chmod-000 file, because
  the test must stay correct under any invoking user.

### AUD-08 — Non-contiguous IPv4 masks normalized incorrectly (P3)

- **Original defect:** `prefix_len_from_mask(255.0.255.0)` returned /8,
  silently converting an invalid mask into a different valid network.
- **Correction:** only true 1s-then-0s masks translate; everything else
  rejects the row into the AUD-07 diagnostic channel with the mask named.
- **Regression tests:** `mask_contiguity_is_validated_not_normalized`
  (all /0../32, boundary patterns, the audit mask and friends),
  `non_contiguous_netmask_rows_are_rejected_with_the_reason_not_normalized`.
- **Commits:** `bb55295`.
- **Remaining limitations:** none known.

### AUD-09 — Stale route detail panel after document import (P3)

- **Original defect:** selections survived document imports; a route from
  the previous document kept rendering as if it belonged to the current
  one (and fed the topology highlight).
- **Correction:** documents carry a generation counter; on every change
  each selection re-resolves BY IDENTITY against the new documents
  (interface by ifindex, node by id, route by the full discriminator set
  — provenance excluded); unresolved selections clear. Route selection
  also clears on view navigation.
- **Regression tests:** unit (`tests/selection.test.ts`: absent route
  cleared, identity re-resolve, superficial-identifier impostor rejected,
  interface/node re-resolve), end-to-end
  (`import workflow: a selected route never survives as stale details` —
  including a second import proving re-resolution repeats).
- **Commits:** `26fcf55`.
- **Remaining limitations:** imported vs live distinction is carried by
  the always-visible provenance line and data badge (pre-existing,
  verified).

### AUD-10 — Frontend document validation accepts invalid structures (P3)

- **Original defect:** `:::` and `1:::2` passed as IPv6; duplicate
  ifindexes, self-referential edges accepted; a stale error message
  referenced v2 while the build writes v3.
- **Correction:** full RFC 4291 textual grammar (compression rules, IPv4
  tail, group counts, no stray colons, no zone ids — outside the document
  contract); duplicate ifindex rejection; self-edge rejection; v3
  message.
- **Regression tests:** 30-case IPv6 table (audit examples, §5.3 list:
  compressed, full, IPv4-mapped, repeated compression, group lengths,
  invalid characters, zone ids), structural tests, message tests.
- **Commits:** `26fcf55`.
- **Remaining limitations:** none known.

### AUD-11 — Documentation contradicts implemented IPv6 procfs behavior (P3)

- **Original defect:** discovery notes claimed `/proc/net/ipv6_route` is
  not parsed (stale since WO-03 M4); the multipath note described the
  collapsed overlap-merge rule.
- **Correction:** correction notes recorded alongside accurate current
  behavior (history preserved, not rewritten); procfs capability table
  updated; architecture merge rules describe the redesigned identity;
  three new kernel-behavior items (IFLA_LINK_NETNSID, `ip route append`,
  RTA_VIA).
- **Commits:** `ea812e9`.
- **Remaining limitations:** historical closeout documents are preserved
  verbatim by policy; their superseded claims are corrected here and in
  the discovery notes rather than by editing history.

### AUD-12 — Rust and TypeScript MAC validation disagree (P3)

- **Original defect:** Rust accepted one or more hex pairs; TS required
  at least two.
- **Correction:** one documented contract on both sides: one or more
  colon-separated hex pairs, case-insensitive, lowercased on
  normalization; the model deliberately supports longer-than-Ethernet
  hardware addresses (e.g. 20-byte IPoIB), so no six-octet assumption is
  introduced.
- **Regression tests:** TS format tests (valid six-octet, 20-byte,
  single pair; empty, bad hex, missing/extra separators, case) aligned
  with the Rust model tests.
- **Commits:** `26fcf55`.
- **Remaining limitations:** neither layer enforces a maximum length —
  the kernel defines none for link-layer addresses.

### AUD-13 — Investigate and correctly represent `RTA_VIA` (P3)

- **Investigation (completed):** `RTA_VIA` is a next-hop encoding with
  its OWN address family (kernel `struct rtvia`), used by MPLS and
  encapsulation routes; the pinned netlink-packet-route 0.34 parses it
  (`RouteVia` incl. AF_PACKET and other-family forms). The previous
  translator ignored the attribute, so a via-only route rendered gateway
  `None` — falsely on-link.
- **Correction:** `via` on routes and multipath hops preserves the
  address family and raw bytes verbatim; via participates in merge
  identity; IP vias draw gateway topology edges; every display path
  (table, details, aria labels) shows the via with its family and NEVER
  claims on-link; non-IP encodings stay raw bytes.
- **Verification path:** constructed netlink messages (the work order's
  sanctioned route where the kernel cannot produce instances): cross-
  family, same-family, AF_PACKET, other-family, and per-hop cases, plus
  serialization and frontend validation/display tests. This sandbox
  kernel (5.10) installs no MPLS routes, so no live instance is
  reproducible here — recorded, not hidden.
- **Commits:** `3c78e97`.
- **Remaining limitations:** live-kernel coverage is impossible on this
  sandbox; the representation is exercised through constructed messages
  only.

### AUD-14 — Netlink connection reset behavior is inconsistent (P3)

- **Original defect:** link and address dump failures reset the cached
  connection; route dump failures did not — a broken connection could
  poison the next run.
- **Correction:** one policy for all three dump kinds: any failure resets
  the connection; one attempt per section per run; no retry loops;
  failures preserved as Failed sections with the error; the next
  observation reconnects.
- **Regression tests:** `netns_dump_failures_reset_the_connection_uniformly_and_recover`
  — a really-broken socket (dropped connection) makes the run fail
  honestly, the NEXT run recovers on a fresh connection, and repeated
  failure/recovery cycles stay safe.
- **Commits:** `3c78e97`.
- **Remaining limitations:** the test seam injects a broken handle
  because kernel-level route-dump failure cannot be forced reliably from
  an unprivileged namespace.

## Section 5 fidelity review

- **§5.1 IFLA_ADDRESS semantics — reviewed, no defect.** Within the plain
  (unfiltered) dumps Araiguma performs, the kernel emits IFLA_ADDRESS
  exactly when the device has a nonzero hardware-address length;
  "not supplied"/"not collected" cannot occur inside a successfully
  translated message, and failed dumps report at the section level. The
  scope of the observed-absence claim is now documented in the discovery
  notes (item 10).
- **§5.2 silently discarded netlink messages — fixed.** Untranslatable
  dump messages are counted and surfaced as provider notes
  (`N message(s) … are not represented — the rest of the dump stands`);
  valid observations still translate. Commit `3c78e97`.
- **§5.3 frontend IPv6 validation — fixed with AUD-10** (full grammar,
  30-case table).
- **§5.4 namespace-aware interface identity — fixed with AUD-01**
  (netnsid qualification for veth peers AND parent links; ifindex is
  never treated as globally unique anywhere in the projection).
- **§5.5 other evidence-loss paths — fixed.** Beyond the AUD-covered
  paths: the snapshot diff key now includes protocol/scope/type/flags/
  TOS/via so watch events expose every discriminator change; table-less
  routes get an explicit "table not observed" display group instead of
  being folded into main. Reviewed and found already honest: provider
  failure degradation, MAC three-state merge, unknown-kind preservation,
  undecodable multipath markers, gateway-node annotations. Commits
  `4f8c01b`, `f997648`.

## Test accounting

Test declarations in the Rust workspace (parent-discovered, excluding the
netns re-exec children whose summaries carry a filtered-out count, per
the accounting rule in `docs/development.md`): reported per validation
cycle in the final report. Frontend: Vitest unit tests and Playwright
end-to-end tests, reported per cycle. No test was weakened, deleted, or
skipped to obtain a passing result; the three tests whose EXPECTATIONS
changed (AUD-01 asymmetry, AUD-03 sentinel multiplicity, table grouping)
pin the corrected honest behavior with the justification recorded in this
document.

## Final commit set (this work order)

`8b27065` AUD-01 · `4f8c01b` AUD-02/03/05 · `3c78e97` AUD-13/14/§5.2 ·
`bb55295` AUD-06/07/08 · `26fcf55` AUD-09/10/12 · `ea812e9`
AUD-04/11/§5.1 · `07a3fb8` fresh-clone env fix · `f997648` §5.5 display
grouping · plus this closeout.
