# Closeout — WO-03 work order (Part 2)

Prepared for Emne's acceptance review. Part 2 proceeded under the standing
authorization after the Part 1 gate passed three consecutive times (recorded
in `wo-02-correction-closeout.md` §A5). Every claim below is backed by the
recorded three-cycle gate at commit `192a89a` (logs preserved under
`/tmp/gate2-cycle-{1,2,3}/` during the run; key figures reproduced in §A5).
The four workflows (M1–M4) were integrated as one application, not isolated
demos: the combined netns test and the extended live check exercise them
together.

---

## A1. Implementations (work order Part 2, milestones 12–15)

| Milestone | Delivered | Commit |
|---|---|---|
| M1 — observation document import | frontend file import of CLI-produced snapshot + topology JSON through the same document-boundary validators built in Part 1 (`lib/validate.ts` primitives; nested structure, discriminated outcomes, numeric ranges, duplicate/dangling checks); fully-validated documents accepted atomically, malformed ones rejected with a path-qualified reason and the previous valid state intact; imported mode is labeled with the file name, observation timestamp, and an explicit "not a live connection" notice; synthetic fixtures and imported observations are visually distinct (badge + provenance line); an explicit control returns to the labeled fixtures; no Tauri commands were implemented (excluded) | `bb9d29e` |
| M2 — veth peer discovery | `peerIfindex` on interfaces, populated only for devices the kernel itself classified as veth; on this kernel (5.10) the evidence is IFLA_LINK carrying the PEER's ifindex (verified live; dumps carry no IFLA_INFO_DATA; the netlink crate models IFLA_VETH_PEER_IFINDEX only on the creation path) — either source works, and the translator clears `parent_ifindex` on veth devices so no parent relation is fabricated; `vethPeer` topology edges drawn when the named peer is part of the same observation (deduplicated when both ends declare, drawn from the declaring side otherwise); an absent peer stays unknown with an honest caveat (it may live in another network namespace); non-veth kinds never produce peer edges even if a foreign document carries the field; schema v3 with all additions serde-defaulted | `5953f98` |
| M3 — full multipath representation | `NextHops`: `Single` (route-level fields ARE the hop) / `Multi(Vec<NextHop>)` (every observed hop with gateway, output interface, effective weight `rtnh_hops + 1` as iproute2 displays it, per-hop flags; route-level gateway/oif null by design — never flattened) / `Undecoded(reason)` (explicit marker, never silently dropped); merge identity is two-level (group key + next-hop-set overlap) so procfs's verified first-hop-only view of a multipath route unites with the netlink observation instead of duplicating it; hop interfaces enriched like route interfaces; `routes_using_interface` counts hops; frontend table shows a hop-count chip (never a false "on-link") and hop interfaces, details panel lists EVERY hop; the first-hop-only wording is gone everywhere; topology draws each hop as a gateway relationship with the kernel weight in the edge detail | `e1cbb6c`, `192a89a` |
| M4 — IPv6 procfs route fallback | `/proc/net/ipv6_route` parser (format verified live, cross-checked with `ip -6 route`: hex columns, big-endian addresses, metric = netlink's RTA_PRIORITY so dedup aligns); dedicated testable parser; injectable roots with synthetic fixtures; `table: None` + note because the file spans multiple tables with no table column; one evidence-based merge rule — a table-less observation matches a knowing one on family+destination+metric+hop and the knowing side's table stands; the kernel's unreachable-default sentinel kept as an honest procfs-only observation and never annotated as an "on-link next hop"; a missing file is a `Failed` section, never evidence of zero routes; nothing overrides higher-priority netlink fields | `916bc9c` |
| Integration (work order §16: one consistent application) | combined netns test (veth + multipath + IPv6 fallback in one namespace through both providers, the merge, and the projection, cross-validated with iproute2); live check extended 11 → 20 assertions including the real binary's multipath hop set, the v6 wildcard dedup, and the sentinel | `192a89a` |

**Defect found by the integration work and fixed:** the topology projection
read only route-level `gateway`/`output_interface` — null by design for
multipath routes — so every multipath gateway produced no node and no edge.
The projection now records each hop's (interface, gateway) pair through the
same evidence path as single-path routes (`192a89a`).

## A2. Files changed (Part 2)

Rust: `crates/core/src/{model,merge,snapshot,topology}.rs`,
`crates/netlink/src/translate.rs`, `crates/sysfs/src/{lib,parse}.rs`,
`crates/cli/src/lib.rs`; tests: `crates/core/tests/{merge,topology,fixtures_roundtrip}.rs`,
`crates/netlink/tests/netns.rs`, `crates/sysfs/tests/provider.rs`.

Frontend: `src/types.ts`, `src/lib/{snapshot,routes}.ts`,
`src/lib/import.ts` + `src/lib/dataMode.svelte.ts` (M1),
`src/components/{RoutesView,RouteDetails,TopologyView,TopologyDetails,ProvidersView}.svelte`,
`src/App.svelte`; tests: `tests/{routes,snapshot,topology,import,live-document}.test.ts`,
`e2e/features.spec.ts`.

Fixtures/docs/scripts: `fixtures/snapshots/*.json` (schema v3: `nextHops`,
v6 procfs scenarios; the workstation `fe80::/64` metric corrected 0 → 256 —
the real kernel value via both sources, verified live), regenerated
`fixtures/topology/*.json`, new `fixtures/procfs/*/net/ipv6_route`,
`scripts/{gen-sysfs-fixtures,live-check}.sh`,
`docs/{architecture,discovery-notes}.md` (schema table, merge rules,
edge table, discovery notes 13–14), `README.md`, this closeout.

## A3. Design decisions

- **Schema v3 additions are serde-defaulted**: `peerIfindex` and `nextHops`
  are optional on read, so v1/v2/M2-era documents parse unchanged. The CLI
  regression tests project all three document vintages through the real
  document boundary — a missing default would fail them (it did once, and
  the tests caught it before commit).
- **Multipath hops carry the gateways; route-level fields are null by
  design.** Flattening multiple hops into one gateway, or letting procfs's
  partial first-hop view write route-level fields onto a multipath
  observation, would both be fabrications. The UI never shows "on-link" for
  a multipath route.
- **The v6 procfs table is unknown, not main.** Unlike `/proc/net/route`
  (main-table-only rows, table genuinely implied), `/proc/net/ipv6_route`
  spans tables with no table column. `table: None` + note, plus one
  evidence-based merge rule, keeps every local-table route (`::1/128`,
  `ff00::/8`) from duplicating on every run without ever inventing a table
  id. Two observations that both observed their tables never merge across
  different tables.
- **The unreachable-default sentinel is kept and named.** It is a real
  kernel route the file reports and netlink does not; it is preserved as a
  procfs-only observation, and the topology says "only an unreachable
  (reject) default route observed" instead of claiming an on-link next hop.
- **Evidence-first implementation**: every kernel-behavior claim (veth
  IFLA_LINK semantics, `rtnh_hops` = weight − 1, the v6 file's column
  format and metric values, the sentinel rows, iproute2's `proto boot`
  display omission) was verified live inside transient namespaces or
  against this machine before being encoded; the uapi headers on this
  machine supplied the RTF_* constants.

## A4. Git history and identity

Part 2 added 5 commits on `main` (`bb9d29e`…`192a89a` plus this closeout
commit), all authored and committed by
`Emne <https://github.com/Emne-Eiji>` (repository-local identity; no remote
configured; single branch; verified by the gate's identity audit step each
cycle). The repository contained 34 commits at the gate commit `192a89a`.
History was never rewritten.

Commits: `feat: import observation documents in the frontend` (M1),
`feat: discover veth peer relationships (schema v3)` (M2),
`feat: represent multipath routes in full (schema v3)` (M3),
`feat: parse /proc/net/ipv6_route as the IPv6 fallback` (M4),
`test: expand WO-03 integration coverage`,
`docs: record WO-03 implementation` (this document).

## A5. Three consecutive verification cycles (the Part 2 gate)

Command per cycle: `ARIGUMA_TEST_LOG_DIR=/tmp/gate2-cycle-N
scripts/test-all.sh` (full suite; no subset). No source changes occurred
between cycles — the working tree was clean at `192a89a` throughout,
verified after each cycle by the suite's own identity-audit step.

| | Cycle 1 | Cycle 2 | Cycle 3 |
|---|---|---|---|
| Started (UTC) | 2026-11-18T20:20:10Z | 2026-11-18T20:21:22Z | 2026-11-18T20:22:22Z |
| Exit code | 0 | 0 | 0 |
| Steps | PASS 16/16 | PASS 16/16 | PASS 16/16 |
| Rust tests (parent summaries) | 112 passed, 0 failed | 112 passed, 0 failed | 112 passed, 0 failed |
| Declared Rust test functions | 112 | 112 | 112 |
| vitest | 92 passed, 1 conditionally skipped* | same | same |
| Playwright e2e (production build) | 12 passed | 12 passed | 12 passed |
| npm audit | 0 vulnerabilities | 0 vulnerabilities | 0 vulnerabilities |
| Durable budget | working 929 MB, store 919 MB — OK | OK | OK |

\* The skipped unit test is `live-document.test.ts`, which runs only with
`ARIGUMA_LIVE_DOCS` set; the suite's dedicated step
("live CLI documents through the frontend validators") re-runs the suite
with that variable pointing at the documents the live step exported from a
transient namespace — those documents (real multipath hops and real v6
routes from this kernel) parsed through the actual frontend validators in
every cycle.

Per-cycle details (identical command list): durable-storage budget on both
measurements; durable-size checker self-tests; shell syntax checks; fixture
regeneration stability; `cargo fmt --all -- --check`; `cargo clippy
--workspace --all-targets -- -D warnings`; `cargo test --workspace`
(including all eight netns integration tests — each child individually
asserted — and the ENOBUFS recovery test); `svelte-check` (0 errors,
0 warnings); vitest; production build; Playwright against the production
preview; npm audit; private D-Bus harness capability check; live
observation + iproute2 cross-validation (20 checks, up from 11 — now
covering the multipath hop set, the v6 gateway, the wildcard dedup, and the
sentinel); live CLI documents through the frontend validators; git identity
audit. Environmental limitations: the sandbox has no
NetworkManager/systemd-resolved/systemd-networkd, no WebKitGTK (no Tauri),
and no desktop session — nothing here claims compatibility with them; the
D-Bus step verifies only that a private session bus can be created and
talked to.

**Test count reconciliation**: 112 declared test functions
(`rg '#\[(tokio::)?test\]' crates -g '*.rs' | wc -l`) = 112 executed and
passed per the parent-summaries count. The netns re-exec children add 8
asynchronous "1 passed; 7 filtered out" summaries that are excluded from
the count by design (documented in `docs/development.md`). Nothing skipped,
nothing ignored, nothing filtered at the workspace level.

## A6. Durable storage and recovery artifacts

- Working footprint 929 MB / persistent store 919 MB against the 950 MB
  budget (both enforced; measurements from the gate). Both measurements
  warn within 50 MB of the budget — monitored, not breached; nothing was
  deleted to fit (deleting project files to save space is forbidden).
- Recovery artifacts at the WO-03 milestone (verified: `git bundle verify`,
  `tar -tzf`, `sha256sum -c`):
  - `download/araiguma-WO03.bundle`
  - `download/araiguma-WO03.tar.gz`
  - `download/araiguma-WO03.sha256` — checksums: see the manifest itself
    (verified by `milestone.sh`).

## A7. Remaining limitations

- Multipath: an undecodable kernel `RTA_MULTIPATH` is an explicit
  "not decodable" marker (reason verbatim) — the one remaining gap, and it
  is impossible to miss in the UI.
- `/proc/net/ipv6_route`: no table id (kept not-observed with a note), no
  protocol/scope/type, and a multipath IPv6 route prints as one row per
  hop in the file without any multipath marker, so procfs-only IPv6 ECMP
  appears as separate single-hop routes (netlink, when available, carries
  the true hop structure and takes priority).
- No Tauri shell, no NetworkManager/D-Bus providers, no publication —
  unchanged, per the exclusions. The frontend runs in a browser; imported
  documents are explicitly labeled as not a live connection.

---

## Part 2 completion statement

All four WO-03 milestones are implemented with regression and integration
coverage; the integration is real (a combined namespace test and a 20-check
live validation exercise the features together, and the combined run found
and fixed a genuine projection defect); three consecutive complete
verification cycles passed with no source changes between them; git history
and identity are correct; the working tree is clean; the durable storage
budget holds on both measurements; verified recovery artifacts exist at the
milestone; and the documentation reflects the implementation and its
limitations. **Part 2 (WO-03) of this work order is complete.**

*Prepared by the implementation agent. Emne is the sole acceptance
authority.*
