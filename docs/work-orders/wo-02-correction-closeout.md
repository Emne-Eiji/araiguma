# Closeout — WO-02 correction work order (Part 1)

Prepared for Emne's acceptance review. Every claim below is backed by the
recorded three-cycle gate at commit `65338b7` (logs preserved under
`/tmp/gate-cycle-{1,2,3}.log` during the run; key figures reproduced in
§A5). Part 2 (WO-03) proceeded only after this gate passed three
consecutive times.

---

## A1. Findings and remediations (work order §2–§9)

| # | Finding | Remediation | Commit |
|---|---|---|---|
| §2 P1 | durable-size check measured the working directory only; the store measurement was informational | `check-durable-size.sh` rewritten: threshold enforced on BOTH measurements with per-measurement attribution; a missing store reported as "not present on this system" (never silent zero); unreadable store is a hard failure; parameterized for testability; 9 self-tests; wired into `milestone.sh` (before and after artifacts) and `test-all.sh` | `d38877e` |
| §3 P1 | route sort key `u8::MAX - prefix_len` sorted default routes LAST, contradicting the comment and intent | corrected sort key: family groups, then table groups, then observed destinations by ascending prefix length (defaults first, broader before more-specific), destination-not-observed last; deterministic ties (metric ascending with not-observed last, then gateway). Seven sequence-verifying regression tests | `d2280b0` |
| §4 P1 | route flags/notes/sources not exposed in the UI; incomplete multipath representation indistinguishable from complete | route details overlay panel (workspace never permanently narrowed): full flags list, observation notes verbatim, provenance, limitation warning; rows with notes carry a discoverable marker; rows keyboard-operable (Enter/Space, focus ring, aria labels via `routeAriaLabel`) | `c58918d` |
| §5 P1 | `mac_address: None` conflated "not observed" with "observed no hardware address" | three-state `MacObservation` (schema v2): plain string (present), `"absent"`, `"notObserved"`; JSON null rejected with the ambiguity explanation; netlink/sysfs translation, merge priority rules, TS contracts, validators, fixtures, goldens, and details panels updated together; v1 documents accepted only where unambiguous (CLI and frontend) | `284592f` |
| §6 P2 | every IFLA_MASTER relationship labeled "bridge port" — bond members mislabeled | edge kind carries the established flavor: `bridgeMember` / `bondMember` (from the master's OWN kind observation) or neutral `masterMember` when unestablished; missing master/parent → annotation, no dangling edge; self-references invalid; frontend labels each distinctly | `2339018` |
| §7 P2 | `NotObserved` treated like `Unsupported` in provider-state aggregation (all-unattempted → NotSupported; mixed → Available) | explicit aggregation rules (documented on `state()`): NotYetObserved / NotSupported / Unavailable / Partial (failed OR unattempted sections) / Degraded / Available; eight new tests for every mix; frontend words Partial as "partial / incomplete run" | `21f0a1d` |
| §8 P2 | frontend parsers validated top-level discriminants only | complete document-boundary validation via shared `lib/validate.ts` primitives: nested interfaces/addresses/routes (family-consistent IPs, prefix bounds, tagged-or-other enums), provider sections and outcome discriminants with counts, numeric integer/range constraints, node ids, duplicate identifiers, edge kinds/evidence, dangling-edge rejection | `537922b`, `9ef8602` |
| §9.1 | Overview folded `lowerLayerDown`/`notPresent` into "unknown state" while topology marked them inactive | explicit labels ("lower layer down", "not present") with the down tone — the views agree; both states are never presented as operational | `7047642` |
| §9.2 | unrecognized `IFLA_INFO_KIND` fell back to ARPHRD → unknown virtual devices confidently classified Ethernet | unrecognized kind string → `Other` + raw kind preserved + "classification uncertain" note; ARPHRD fallback only when no kind string exists (physical devices) | `7047642` |
| §9.3 | topology test hook unconditionally on `window`, never removed | exposed only in dev or with explicit `?araiguma-test-hooks`; deleted on unmount; e2e verifies inert-on-plain-visit against the production build | `0ceb170` |
| §9.4 | route rows pointer-click only | rows are keyboard-operable with visible focus, roles, aria-pressed, and per-row accessible labels; e2e exercises Enter and Space paths | `c58918d` |
| §9.5 | monitor reconnection tested structurally only; no practical socket-failure test | two defects found and fixed while implementing the test: (a) the monitor ignored `NLMSG_OVERRUN` (kernel dropped notifications) — now reported as `Resynchronized` with unit tests through the real `next_signal`; (b) the netns re-exec pattern called `process::exit` after the first child, orphaning siblings — one genuinely failing test had been hidden behind the green gate since WO-02; every child is now asserted (which also makes test accounting deterministic). A practical ENOBUFS recovery test floods a real socket inside a transient namespace and verifies resync + continued delivery | `1a8fad7` |
| §9.6 | multipath first-hop-only limitation not visible | route note surfaced in the details panel with a warning banner and a row marker; the note text remains on the route until full multipath support lands (WO-03 M3) | `c58918d` |
| §10 | closeout accounting discrepancies | see A4/A5 and the corrected `wo-01r-wo-02-closeout.md` (commit count 14 → 15 with the final closeout commit identified; test count 52 → 53 with the exact mechanism, which the correction then proved was hiding a real failure) | `65338b7` |

## A2. Files changed (Part 1)

Rust: `crates/core/src/{model,merge,provider,snapshot,topology}.rs`,
`crates/netlink/src/{translate,monitor}.rs`,
`crates/sysfs/src/lib.rs`, `crates/cli/src/lib.rs`; tests:
`crates/core/tests/{merge,provider_state,topology,fixtures_roundtrip,diff}.rs`,
`crates/netlink/tests/netns.rs`, `crates/sysfs/tests/provider.rs`.

Frontend: `src/types.ts`, `src/lib/{format,snapshot,topology,routes,validate}.ts`,
`src/components/{RoutesView,RouteDetails,DetailsPanel,TopologyDetails,TopologyView,ProvidersView}.svelte`,
`src/App.svelte`; tests: `tests/{format,snapshot,topology,routes,validation}.test.ts`,
`e2e/features.spec.ts`.

Scripts/docs/fixtures: `scripts/{check-durable-size,test-check-durable-size,milestone,test-all}.sh`,
`fixtures/snapshots/*.json` (schema v2 + limitation coverage), regenerated
`fixtures/topology/*.json`, `docs/{architecture,discovery-notes,development}.md`,
`README.md`, `docs/work-orders/wo-01r-wo-02-closeout.md` (corrections).

## A3. Design decisions

- **MAC three-state as strings, not a tagged object**: `Present` serializes
  as the plain address string, byte-identical to v1, so existing documents
  with real addresses parse unchanged; only the two sentinel states are new.
  Ambiguous v1 `null` is rejected with the reason, on both the Rust and
  TypeScript boundaries. Uppercase addresses canonicalize to lowercase;
  any count of colon-separated hex byte pairs is valid (the kernel's
  addr_len varies by hardware class).
- **Master relationship flavor from the master's own kind**: the kernel
  states membership (IFLA_MASTER); the master's kind observation states the
  flavor. No assumption bridges the two.
- **Provider-state aggregation keeps three outcome classes distinct** and
  defines Partial as "incomplete run" (failed or unattempted), so health is
  never claimed from missing observations.
- **ENOBUFS/Overrun → Resynchronized without reconnect**: the subscription
  survives a buffer overrun; the dropped events are the problem, and
  consumers re-dump. The old behavior (silently continuing) was the actual
  defect the review's §9.5 anticipated.
- **Netns re-exec guard instead of exit**: the parent asserts each child's
  status and skips the body (`if !ensure_netns(...) { return; }`), keeping
  the never-run-outside-a-namespace safety property while making every
  child's result count — deterministically reconcilable accounting.

## A4. Git history and identity

Part 1 added 13 commits on `main` (`d38877e`…`65338b7`), all authored and
committed by `Emne <https://github.com/Emne-Eiji>` (repository-local
identity; no remote configured; single branch; verified by the gate's
identity audit step). The repository now contains 28 commits total.
History was never rewritten; the WO-02 closeout's commit-count error was
corrected in documentation only.

## A5. Three consecutive verification cycles (the Part 1 gate)

Command per cycle: `ARIGUMA_TEST_LOG_DIR=/tmp/gate-cycle-N scripts/test-all.sh`
(full suite; no subset). No source changes occurred between cycles — the
working tree was clean at `65338b7` throughout, verified after each cycle.

| | Cycle 1 | Cycle 2 | Cycle 3 |
|---|---|---|---|
| Started (UTC) | 2026-11-18T19:09:06Z | 2026-11-18T19:10:13Z | 2026-11-18T19:11:08Z |
| Exit code | 0 | 0 | 0 |
| Steps | PASS 15/15 | PASS 15/15 | PASS 15/15 |
| Rust tests (parent summaries) | 86 passed, 0 failed | 86 passed, 0 failed | 86 passed, 0 failed |
| Declared Rust tests | 86 | 86 | 86 |
| vitest | 65 passed | 65 passed | 65 passed |
| Playwright e2e (production build) | 9 passed | 9 passed | 9 passed |
| npm audit | 0 vulnerabilities | 0 vulnerabilities | 0 vulnerabilities |
| Durable budget | working 926 MB, store 919 MB — OK | OK | working 928 MB, store 919 MB — OK |

Per-cycle details (identical command list): durable-storage budget on both
measurements; durable-size checker self-tests (9 cases); shell syntax
checks; fixture regeneration stability; `cargo fmt --all -- --check`;
`cargo clippy --workspace --all-targets -- -D warnings`;
`cargo test --workspace` (including all four netns integration tests —
each child individually asserted — and the ENOBUFS recovery test);
`svelte-check` (0 errors, 0 warnings); vitest; production build; Playwright
against the production preview; npm audit; private D-Bus harness capability
check; live observation + iproute2 cross-validation inside a transient
namespace (11 checks); git identity audit. Environmental limitations: the
sandbox has no NetworkManager/systemd-resolved/systemd-networkd, no
WebKitGTK (no Tauri), and no desktop session — nothing here claims
compatibility with them; the D-Bus step verifies only that a private
session bus can be created and talked to.

**Test count reconciliation (§10)**: 86 declared test functions
(`rg '#\[(tokio::)?test\]' crates -g '*.rs' | wc -l`) = 86 executed and
passed per the parent-summaries count. The netns re-exec children add 4
asynchronous "1 passed; 2 filtered out" summaries that are excluded from
the count by design (documented in `docs/development.md`). Nothing skipped,
nothing ignored, nothing filtered at the workspace level.

## A6. Durable storage and recovery artifacts

- Working footprint 928 MB / persistent store 919 MB against the 950 MB
  budget (both enforced; measurements from the gate).
- Recovery artifacts at the Part 1 milestone (verified: `git bundle verify`,
  `tar -tzf`, `sha256sum -c`):
  - `download/araiguma-WO02-CORRECTIONS.bundle` (260 KB)
  - `download/araiguma-WO02-CORRECTIONS.tar.gz` (176 KB)
  - `download/araiguma-WO02-CORRECTIONS.sha256` — checksums:
    see the manifest itself (verified by `milestone.sh`).

## A7. Remaining limitations

- Multipath routes still represent only the first next hop — now
  impossible to miss in the UI (row marker, note, warning), with full
  representation the next work item (WO-03 M3).
- veth endpoints are nodes without pairing edges (WO-03 M2).
- IPv6 procfs routes are not parsed (WO-03 M4).
- No Tauri shell, no NetworkManager/D-Bus providers, no publication —
  unchanged from WO-02, per the exclusions.

---

## Part 1 completion statement

Every finding in work-order sections 2–9 is remediated with regression
coverage; the closeout accounting discrepancies are reconciled and
corrected; three consecutive complete verification cycles passed with no
source changes between them; git history and identity are correct; the
working tree is clean; the durable storage budget holds on both
measurements; verified recovery artifacts exist at the milestone; and the
documentation reflects the implementation and its limitations. **Part 1 of
this work order is complete; Part 2 (WO-03) proceeds.**

*Prepared by the implementation agent. Emne is the sole acceptance
authority.*
