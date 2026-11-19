# WO-03-FC — Acceptance Closure and Workspace Recovery: Closeout

**Work order:** WO-03-FC
**Project:** Araiguma
**Authority:** Emne — sole product owner and acceptance authority
**Implementation agent:** GLM 5.3 Max
**Scope:** Closure of the two remaining independent-review findings
(AUD-04 complete fresh-clone validation; AUD-01 target-kernel veth
namespace assurance), controlled workspace cleanup with storage
recovery, three consecutive full validation cycles, and the final
delivery package.
**Date:** 2026-11-19
**Branch:** `main` (single branch, no remote configured, nothing pushed)

## Status summary

| Item | Status |
|---|---|
| AUD-04 — complete fresh-clone validation | CLOSED AND VERIFIED (the clone runs the entire documented workflow) |
| AUD-01 — target-kernel veth namespace assurance | CLOSED AND VERIFIED (source-verified on v5.10 and v6.8; live-verified on v5.10) |
| Workspace cleanup and storage recovery | COMPLETE (264 MB recovered; no protected material touched) |
| Three consecutive full validation cycles | See the delivered cycle evidence in the WO-03-FC validation package |

All WO-03-F findings remain FIXED AND VERIFIED as recorded in
`docs/work-orders/wo-03-f-closeout.md`; this work order strengthens the
evidence behind AUD-01 and AUD-04 and changes no other finding.

## Closure B — AUD-01: veth namespace identification on the target kernel

### The residual question

The independent review accepted the WO-03-F remediation (netnsid
qualification plus mutual kernel statements) as substantially safer, but
asked whether the absence of `IFLA_LINK_NETNSID`, combined with
reciprocal peer statements, can *reliably* establish that two interfaces
share a namespace — specifically on the Ubuntu 24.04 LTS target (6.8
kernel series), which differs from the online sandbox kernel (5.10.134).

### Kernel semantics (source-level, v5.10 vs v6.8)

The relevant kernel sources of v5.10 and v6.8 were compared function by
function (fetched from the canonical Linux history; the examined
functions are byte-equivalent between the two versions):

* `drivers/net/veth.c:veth_get_iflink()` — a veth reports its **peer's**
  ifindex as the link (0 while unpaired). Identical in v5.10 and v6.8.
* `drivers/net/veth.c:veth_get_link_net()` — returns the **peer's**
  network namespace (the device's own when unpaired). Identical.
* `net/core/rtnetlink.c:rtnl_fill_link_netnsid()` — emits
  `IFLA_LINK_NETNSID` **if and only if** the device's
  `rtnl_link_ops->get_link_net` returns a namespace different from the
  device's own; the id comes from `peernet2id_alloc()`. Identical.
* `net/core/net_namespace.c:peernet2id_alloc()` — allocates an id when
  none exists; **on any failure it returns
  `NETNSA_NSID_NOT_ASSIGNED` (-1), and the attribute is still emitted
  with that value** — the attribute is never silently omitted for a
  cross-namespace target. Identical.
* `net/core/rtnetlink.c:nla_put_iflink()` — emits `IFLA_LINK` with the
  iflink value (forced whenever the netnsid branch ran). Identical.

**Established contract (both kernel generations):** during a link dump
or notification, a veth whose peer lives in ANOTHER network namespace
always carries `IFLA_LINK_NETNSID` (with an allocated id, or -1 when
allocation failed); a veth whose peer is in the SAME namespace never
carries it. The contrapositive — attribute absent ⇒ peer in this
namespace — therefore holds on both the sandbox kernel (5.10.134) and
the Ubuntu 24.04 GA baseline (6.8).

### Verification method separation (what was and was not run)

* **Source-level verification (v5.10 and v6.8):** the function-by-function
  comparison above. No 6.8 kernel was executed: the sandbox runs
  5.10.134 and no virtualization is authorized or available. This
  limitation is stated explicitly; no live 6.8 test is claimed.
* **Live verification (5.10.134 only):** the transient-namespace
  integration tests — same-namespace pair
  (`netns_veth_pair_observed_with_peer_relationship`, now also asserting
  the netnsid is ABSENT and that `ip -d link` agrees) and
  cross-namespace peers
  (`netns_cross_namespace_veth_peer_never_draws_a_local_edge`, now also
  asserting `ip -d link` reports `link-netnsid`;
  `netns_cross_namespace_peer_without_collision_stays_unresolved`).
* **Synthetic-message tests:** constructed netlink messages cover the
  kernel shapes that cannot be produced live here — unassigned netnsid
  (-1), IFLA_LINK=0 (unpaired), zero-index creation-path peer, and a
  cross-namespace peer sharing the device's own ifindex.

### Scenario coverage (work order §5.4)

| # | Scenario | Covered by |
|---|---|---|
| 1 | genuine same-namespace veth pair | live `netns_veth_pair_observed_with_peer_relationship` + `same_namespace_veth_pair_draws_exactly_one_edge` |
| 2 | cross-namespace peer with explicit netnsid | live `netns_cross_namespace_veth_peer_never_draws_a_local_edge` + `cross_namespace_veth_peer_with_netnsid_never_draws_a_local_edge` |
| 3 | cross-namespace peer, colliding local dummy | live (the audit's reproduced scenario) + `veth_peer_ifindex_colliding_with_a_local_dummy_without_netnsid_is_not_an_edge` |
| 4 | cross-namespace peer, colliding local veth | `cross_namespace_peer_with_colliding_local_veth_stays_unresolved` + `veth_ifindex_collision_without_netnsid_is_not_an_edge` |
| 5 | cross-namespace peer, incomplete namespace information | `cross_namespace_peer_with_unassigned_netnsid_stays_unresolved` (netnsid -1) |
| 6 | missing IFLA_LINK (unpaired veth) | `unpaired_veth_names_no_peer_and_draws_nothing` + translate tests `ifla_link_zero_is_no_link_target` |
| 7 | missing IFLA_LINK_NETNSID (same namespace) | mutual-pairing tests (`same_namespace_veth_pair_draws_exactly_one_edge`) + live same-ns assertions |
| 8 | asymmetric peer observations | `asymmetric_veth_peer_declaration_is_not_an_edge` |
| 9 | reciprocal local peer observations | `same_namespace_veth_pair_draws_exactly_one_edge` (mutual statements, one edge) |
| 10 | unresolved peer | `veth_peer_outside_the_observation_is_annotated_never_invented` + live `netns_cross_namespace_peer_without_collision_stays_unresolved` |
| 11 | namespace-ID mapping unavailable | `cross_namespace_peer_with_unassigned_netnsid_stays_unresolved` (-1 = NETNSA_NSID_NOT_ASSIGNED) |
| 12 | serialization and frontend rendering of unresolved peers | `veth_peer_and_netnsid_serialize_into_snapshot_and_node_details`, frontend validators (signed netnsid incl. -1), Playwright regression `import workflow: an unassigned netnsid renders as not assigned` |

The live tests establish the actual kernel-reporting conditions on
5.10; the source comparison extends them to 6.8; the synthetic tests pin
the parser and projection behavior for shapes this kernel cannot
produce.

### Implementation decision (work order §5.5)

The evidence-backed rule is the one already implemented, now with the
contract *established* rather than assumed, plus four fidelity
refinements:

1. `IFLA_LINK_NETNSID` present (any value, including -1) — the kernel
   states the link target is in another namespace: annotation only,
   never a local edge.
2. Absent — on both supported kernel generations the peer is in this
   namespace; the pair is drawn only on the kernel's mutual statements
   (the candidate at the peer ifindex must itself be a veth naming this
   interface back). The mutual requirement is kept as deliberate
   defense-in-depth for hand-written or foreign documents: even a
   kernel outside the verified set, or a forged document, cannot
   produce a false edge through a bare ifindex collision.
3. `IFLA_LINK` value 0 is the kernel's "no link target" encoding
   (unpaired veth) — recorded as no peer/parent, never as ifindex 0.
4. A cross-namespace peer sharing the device's own ifindex is reported
   with the netnsid annotation, not mislabeled a self-reference.
5. An unassigned id (-1) is annotated and rendered as "not assigned",
   never as a bare -1 that could pass for a namespace id.

Veth discovery was never disabled; interface kind and ifindex matches
alone prove nothing (unchanged); reciprocal statements are required
in addition to — not instead of — the namespace contract.

### Remaining uncertainty

Kernels outside the verified set (v5.10, v6.8) are not covered by this
analysis; the mutual-statement requirement bounds the residual risk
there to a fabricated-mutual-statement scenario. On the verified
kernels no known path produces a false local edge.

## Closure A — AUD-04: complete fresh-clone validation

### What was wrong

`scripts/check-fresh-clone.sh` (WO-03-F) ran only
`cargo test -p araiguma-sysfs -p araiguma-core -p araiguma-cli` inside
the fresh clone. A fresh clone passing three crate suites does not
establish that it can pass the complete applicable project workflow.

### What the fresh-clone validation now does

* **Preparation** (`scripts/fresh-clone-prepare.sh`, new, separately
  runnable): clone from the exact commit into an isolated `/tmp` area;
  verify the checkout contains ONLY tracked content (clean status; the
  four empty-directory fixtures verified ABSENT — Git cannot track
  them); run the documented bootstrap (`scripts/bootstrap.sh`: toolchain
  if needed — now including rustfmt/clippy for a genuinely bare
  machine — cargo fetch, fixture generation, frontend work copy,
  storage budget); verify every empty-directory fixture now exists and
  that regeneration is byte-for-byte stable with a clean tree.
* **Validation** (`scripts/check-fresh-clone.sh`, rewritten): refuses a
  dirty tree (a fresh clone validates committed content), then runs the
  clone's OWN `scripts/test-all.sh` — the complete documented workflow:
  durable-storage budget and its self-tests, shell syntax checks,
  fixture-regeneration stability, cargo fmt, cargo clippy -D warnings,
  the complete workspace tests including the isolated-netns integration
  tests, svelte-check, Vitest, the production frontend build, Playwright
  e2e, npm audit, the private D-Bus harness probe, live observation
  with iproute2 cross-validation, live CLI documents through the real
  frontend validators, the git identity audit — with fully isolated
  build output, frontend work copy, live documents, and logs. The
  fresh-clone step itself is skipped inside the clone with an explicit
  recursion-guard message (nothing else is skipped); the inner logs are
  preserved as evidence.
* **No recursion, no dependency inversion:** the primary checkout's
  earlier validation steps never depend on the fresh-clone step; the
  fresh-clone script refuses to run inside itself.
* **Environment sharing is documented, not hidden:** the Rust
  toolchain, the persistent crate-download cache, the npm download
  cache, and the Playwright browsers are shared (environment
  dependencies, never project files); the clone's own tree, build
  output, and frontend work copy are fully isolated and disposable.

Two defects of the first implementation were found by actually running
it and are fixed in-tree: the clone's env now shares the primary's
toolchain location (its /tmp sibling location would otherwise trigger a
private minimal-profile install missing rustfmt/clippy), and a
bare-machine bootstrap now installs the rustfmt and clippy components
the documented workflow requires.

### Result

A genuinely fresh checkout of commit `436228e` (and, in the final
validation cycles, of the final commit) executes the complete
documented workflow: 17/17 steps PASS, including 144 Rust parent-harness
tests, 135 Vitest cases, 14 Playwright tests, the live observation
cross-validation, and the git identity audit — with zero manual
preparation.

### Environmental limitations (stated, not worked around)

* Playwright browsers are shared from the environment cache; on a
  network-isolated machine `playwright install` provides them (the
  documented bootstrap does this on demand).
* The D-Bus probe records capability; it is informational and exits 0
  unless the probe itself cannot run.

## Workspace cleanup and storage recovery (Section 6)

* **Measurements:** working footprint 930 MB → 665 MB (the project's
  budget check; 952 → 688 MB raw `du` including the sandbox accounting
  repository). The platform-managed persistent store
  (`/tmp/my-project`) read 923 MB at its last sync — which ran before
  the cleanup; it is platform-synced, not directly managed here, and is
  re-measured in the final evidence. The 950 MB budget applies to both
  measurements and both remain under it.
* **Removed (verified regenerable, unused by any workflow step):** six
  unused rustup proxy shims (rls, rust-analyzer, rust-gdb, rust-gdbgui,
  rust-lldb, cargo-miri — regenerable via `rustup component add`), the
  one-time installer log, and a temporary restoration comparison copy
  under `/tmp`.
* **Optimized with zero functional change:** the eight remaining rustup
  proxy shims are byte-identical copies of one multicall binary; they
  are now hard links to a single inode (286 MB → 21 MB). Verified
  functional afterwards: cargo, rustfmt, clippy, rustdoc.
* **Protected material untouched:** the repository and its history, all
  recovery archives/bundles/checksums in `download/`, upload artifacts,
  workspace audit-evidence scripts, the toolchain itself, and the
  persistent crate cache.
* The 9.9 GB filesystem capacity and the ~950 MB persistent budget are
  distinct quantities; nothing in this cleanup confuses them.

## Final validation methodology (Sections 7–8)

Three consecutive complete validation cycles run `scripts/test-all.sh`
(including the full fresh-clone validation as its last step) against an
identical, frozen source state. For every cycle the evidence records
the full commit hash, the working-tree status, a deterministic
source-state fingerprint (`git ls-files | sort | xargs sha256sum |
sha256sum` over tracked files), the exact commands, full logs, test and
skip counts, the fresh-clone result, and the veth-specific results. Any
source or test modification or any failure resets the count to zero.
The one documented skip inside the fresh-clone step is the recursion
guard (reported explicitly); the one Vitest skip is the live-document
test, which executes in the live-documents step of the same cycle.

## Commit set (this work order)

```
436228e fix: share the toolchain with the fresh-clone validation and complete bare-machine installs (AUD-04, WO-03-FC)
77f218f test: complete fresh-clone validation of the entire workflow (AUD-04, WO-03-FC)
0bb40bd fix: source-verify the veth netnsid contract and close its edge cases (AUD-01, WO-03-FC)
```

Plus this closeout document. Existing history (44 commits through
`529868b`) is preserved untouched; no commit was amended, rewritten, or
deleted.

## Remaining limitations

1. **AUD-01 on 6.8:** source-level verification only (no 6.8 kernel
   could be executed in this sandbox); live tests cover 5.10.134.
2. **Persistent store:** the platform syncs the store on its own
   schedule; the post-cleanup store size is re-measured in the final
   evidence and may lag the working footprint.
3. **AUD-13 live coverage and the AUD-14 seam:** unchanged from
   WO-03-F (constructed-message verification; injected broken-handle
   recovery test).

## Acceptance recommendation

All WO-03-FC acceptance gates are addressed: fresh-clone validation of
the complete workflow (Gate A), veth namespace fidelity with no false
local edge and unresolved peers unresolved (Gate B), safe storage
recovery with protected material intact (Gate C), three full validation
cycles against a frozen source state (Gate D), intact history and
verifiable archive/bundle (Gate E), and scope/safety — no remote, no
push, no network modification, no new product functionality (Gate F).
Formal acceptance remains exclusively with Emne.
