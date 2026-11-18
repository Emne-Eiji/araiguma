# Closeout — WO-01R (reconstruction) and WO-02 (topology, routes, provider status)

Prepared for Emne's acceptance review. Two clearly separated sections as
required by the work order §9. Everything claimed below is backed by the
recorded gate run; nothing in the sandbox-based testing establishes
compatibility with real NetworkManager, systemd-resolved, systemd-networkd,
Tauri, or desktop environments.

---

# Part A — WO-01R reconstruction closeout

## A1. Architecture and workspace

Reconstructed exactly to the prescribed layout:

```
araiguma/  (single main branch, local only, no remote)
├── Cargo.toml + Cargo.lock   workspace, exact-pinned dependencies
├── rust-toolchain.toml       Rust 1.100.0
├── crates/core               model, provenance, merge, diff, topology (WO-02)
├── crates/sysfs              sysfs/procfs fallback provider (injectable roots)
├── crates/netlink            rtnetlink provider + multicast monitor
├── crates/cli                `araiguma snapshot | watch | topology`
├── apps/frontend             Svelte 5.58 + TS 5.9.3 + Vite 8.3.8 (+ Cytoscape 3.35)
├── fixtures/                 snapshots/, sysfs/, procfs/, topology/ (synthetic, labeled)
├── scripts/                  env, bootstrap, frontend, gen-*-fixtures, live-check,
│                             test-all, check-durable-size, check-dbus-harness,
│                             disk-usage, milestone
└── docs/                     README + architecture + discovery-notes +
                              environment-inventory + development + incident record +
                              design/wo-02-implementation + work-orders/ (verbatim
                              WO-01 text and WO-01R+WO-02 authorization)
```

Repository size: 3.4 MB working tree + 2.1 MB `.git`. Durable footprint
926 MB total (repository 4 MB; the rest is the toolchain area), under the
950 MB budget; persistent store 919 MB after the authorized cleanup.

## A2. Implemented functionality

- **Core domain** (commit `15fb275`): interfaces/addresses/routes with
  explicit not-observed semantics (`None` is never a negative fact; the one
  documented exception is `Route::gateway`, where the kernel itself encodes
  "no next hop"), per-item observation provenance, classification from
  netlink `IFLA_INFO_KIND` with ARPHRD fallback and sysfs caveats, RFC 2863
  operstate, verified `if_inet6` scope/flag encoding, little-endian
  `/proc/net/route` parsing (masks corrected — see A8), RTPROT/RT_SCOPE/RTN
  mappings with raw-value preservation.
- **Provider abstraction** with six-state health (available, degraded,
  partial, unavailable, not supported, not yet observed) and per-section
  outcomes; a failing provider never discards other providers' observations.
- **Merge layer**: netlink primary, sysfs fills `None` fields only,
  cross-source dedup with oif normalization and implied-table-254 handling.
- **Netlink provider** (`93ed847`): read-only dumps (links, addresses,
  routes in all tables), kernel-faithful pure translation, multipath
  first-hop representation (noted), **monitor** with the five change
  multicast groups, exponential-backoff reconnect (250 ms→8 s), and an
  explicit `Resynchronized` signal.
- **sysfs/procfs provider** (`9f188a3`): interface attributes with wireless/
  bridge detection and master symlinks, IPv6 addresses, main-table IPv4
  routes with implied-table notes; `Degraded` state reflecting honest
  classification limits.
- **CLI** (`bd07c59`): `snapshot [--pretty]`, `watch` (NDJSON: snapshot /
  debounced changed / resynchronized envelopes), SIGINT/SIGTERM handling.
- **Frontend foundation** (`bb1b42c`): dark charcoal design system, 56 px
  icon rail, ≥96% workspace, overlay details panel, persistent MOCK DATA
  badge, honest state chips; mock data exclusively from the schema-locked
  fixtures through a validating loader.

## A3. Exact tests and results (final gate, all green)

Command: `./scripts/test-all.sh` — **RESULT: PASS (12/12 steps)**.

| Step | Result |
|---|---|
| durable-storage budget | PASS (926 MB < 950 MB) |
| fixture regeneration stability (sysfs + topology goldens) | PASS |
| `cargo fmt --all -- --check` | PASS |
| `cargo clippy --workspace --all-targets -- -D warnings` | PASS |
| `cargo test --workspace` (incl. netns integration) | PASS — 52 tests |
| `svelte-check` | PASS — 0 errors, 0 warnings |
| vitest | PASS — 34 tests |
| frontend production build | PASS |
| Playwright e2e (production build, preview :5199) | PASS — 7 tests |
| private D-Bus harness capability check | PASS (session bus + ListNames) |
| live observation + iproute2 cross-validation (netns) | PASS — 11 checks |
| git identity audit | PASS (14 commits, all Emne, no remote, clean tree) |

Live checks run the real binary inside a transient `unshare -Urn`
namespace: dummy classification, operstate-unknown-while-up, scopes, main +
local tables, provider states, gateway and address agreement with `ip -j`,
and a watch smoke (snapshot + change events). The host network namespace
was never modified.

## A4. Git commits and identity verification

> **Corrected by the WO-02 correction work order (§10).** The original text
> below reported "14 commits, `1a939dc`…`6744c4e`" — it omitted the final
> closeout commit `026af6b` ("docs: record WO-01R + WO-02 closeout") that
> this very document was written in. The recovery manifest
> (`araiguma-FINAL.sha256`) was correct: **the final WO-02 repository
> contains 15 commits**, of which 14 were the implementation milestones
> `1a939dc`…`6744c4e` and the 15th was the final closeout commit `026af6b`
> at HEAD. History was not rewritten; only this report is corrected.

15 commits, `1a939dc`…`026af6b`. Every commit's author and committer is
`Emne <https://github.com/Emne-Eiji>` (repository-local config;
the sandbox global identity was never used). No remote is configured;
working tree clean; single `main` branch. The original 11-commit history is
unrecoverable; the 11-vs-10 discrepancy is recorded as permanently
unresolved in the incident record — no reconciliation invented.

```
026af6b docs: record WO-01R + WO-02 closeout          <- final closeout commit
6744c4e docs: update README and architecture for WO-02 features
f58d5f5 feat: implement interactive topology, route presentation, and provider status
9da86b0 feat: add network topology transformation
53576d9 docs: record WO-02 implementation design
9e462d1 fix: comment manifest header lines for sha256sum -c compatibility
c490ff9 fix: write milestone checksums from the download directory
a14aeda docs: add architecture, discovery, environment, development, and incident documentation
7e5b354 test: add live cross-validation and the full validation suite
bb1b42c feat: add Svelte frontend foundation
bd07c59 feat: add read-only snapshot and watch CLI
93ed847 feat: add rtnetlink discovery and event monitoring
9f188a3 feat: add sysfs and procfs discovery provider
15fb275 feat: add core observation model with merge and provenance
1a939dc chore: scaffold workspace and durable-storage layout
```

## A5. Durable storage measurements

- Authorized cleanup executed (M0): removed `.cache/npm` (210 MB working /
  209 MB store), `toolchains/rustup-init` (21 MB), `toolchains/cargo/
  registry/src` (1 MB working / 33 MB store). Store 1180 MB → **919 MB**;
  crate archive cache verified intact (44 archives).
- Rules in force: `CARGO_TARGET_DIR=/tmp/araiguma-target`; ephemeral
  `CARGO_HOME=/tmp/araiguma-cargo` reusing the persistent archive cache;
  npm cache `/tmp/araiguma-npm-cache`; frontend tooling in the work copy
  `/tmp/araiguma-work` (see A9 deviation); automated `check-durable-size.sh`
  in the gate.
- Current: footprint 926 MB, repository 4 MB, ~24 MB headroom under budget.

## A6. Recovery artifacts

Written outside the repository under `download/`, each verified
(`git bundle verify`, `tar -tzf`, `sha256sum -c`):

- `araiguma-WO01R-FOUNDATION.bundle` / `.tar.gz` / `.sha256` (commit
  `9e462d1`, 10 commits)
- `araiguma-WO02-FEATURES.bundle` / `.tar.gz` / `.sha256` (commit
  `f58d5f5`, 13 commits)
- Final handoff archive at the closeout commit: `araiguma-FINAL-*.tar.gz`
  (complete repository including `.git`) + `araiguma-FINAL.sha256` manifest
  (see Part B, section B8).

## A7. Known limitations

- No Tauri/desktop shell in this environment (no WebKitGTK, no root); the
  frontend targets a browser. Transfer to a real desktop environment is
  required for shell integration.
- No NetworkManager / systemd-resolved / systemd-networkd services exist in
  the sandbox; nothing here claims integration with them.
- Multipath routes: only the first next hop represented (noted per route).
- WireGuard peers, actual upstream beyond gateways, and internet
  connectivity are unobservable and reported as such.

## A8. Reconstruction corrections (honest record)

- The surviving WO-01 fixture script wrote `/proc/net/route` **masks
  big-endian**; the kernel writes them little-endian like all values in
  that file. Masks corrected (`00FFFFFF` = /24, `000000FF` = /8), intent
  preserved, correction documented in the generator and discovery notes.
- Verified live during reconstruction: an absent `RTA_PRIORITY` is the
  kernel's encoding of effective metric 0 (matches procfs Metric column);
  the netlink translation records `Some(0)` so cross-source dedup is
  correct.

## A9. Deviation from the work order (documented)

The work order prescribed a gitignored `node_modules` symlink to
`/tmp/araiguma-node-modules`. **This sandbox's command policy forbids
creating symlinks**, and persisting a symlink into the sync layer could
corrupt the persistence store. `scripts/frontend.sh` achieves the same
guarantee differently: all frontend tooling runs in an ephemeral work copy
(`/tmp/araiguma-work`, mirroring the repo layout so fixture imports resolve
identically); `node_modules` never exists inside the repository; the
generated `package-lock.json` is copied back and committed. All other
durable-storage rules are implemented as prescribed.

## A10. Incident documentation

`docs/incident-2026-11-18-repository-loss.md` records the evidence-backed
root cause (persistence store over its 1024 MB boot-restore cap: toolchain
971 MB + npm cache 209 MB; the repository never entered the store), the
permanent loss (including the 11-vs-10 commit-count discrepancy, recorded
as permanently unresolved), the response, and the corrective actions.

---

# Part B — WO-02 implementation closeout

## B1. Topology implementation and semantics

`crates/core::topology` (commit `9da86b0`) + `araiguma topology`
(file/stdin projection). Semantics exactly as designed in
`docs/design/wo-02-implementation.md`:

- **Nodes**: every observed interface (all kinds preserved, including
  unrecognized); gateway nodes only where a route names a next-hop IP.
  No internet/upstream/cloud nodes exist anywhere in the model or UI — a
  default route is a routing fact, not connectivity.
- **Edges** (evidence required, classes carried on every edge):
  `bridgeMember` (IFLA_MASTER / sysfs master), `linkParent` (IFLA_LINK:
  vlan/tunnel), `gatewayRoute` (main-table route naming oif + gateway;
  destinations aggregated per relationship), `gatewayInferred` (gateway
  without oif inside exactly one interface's subnet, derivation recorded;
  ambiguous → annotation, no edge).
- **Unknown stays unknown**: WireGuard peers, tunnel underlays, upstreams
  beyond gateways, and on-link defaults without gateways surface as node
  annotations; veth endpoints are nodes but no pairing edge is fabricated
  (pairing is not in the observation model).
- **Determinism**: pure projection; committed goldens
  (`fixtures/topology/*.json`) generated by the Rust projection and
  stability-checked by the gate.
- **Interaction** (commit `f58d5f5`): Cytoscape 3.35.0 (assessed: MIT, no
  peer dependencies, no extensions needed; adopted), styled by
  kind/state/evidence (solid observed / dashed inferred), legend stating
  what is and is **not** drawn, node selection opens the overlay details
  panel (relations with evidence labels, addresses, notes), route→topology
  linkage is highlight-only.

## B2. Route presentation and filtering

All requested fields render (destination/prefix, gateway, oif, protocol,
scope, type, metric, family, table, flags via tooltips). The default view
is the main table (254) only; other tables are offered behind an explicit
chip selector with per-table counts and clear labels — nothing discarded,
nothing merged across tables. Multiple default routes coexist, emphasized
and metric-ordered; no single "the default" is invented. Missing fields
render `—` with not-observed tooltips; `gateway: null` renders "on-link"
with an explanatory tooltip (an observed absence per the kernel encoding).
Long names/prefixes truncate with full values on hover. Selecting a route
highlights its interface and gateway nodes in the topology view
(highlight only; nothing implies control). The incomplete-observations
fixture drives the degraded rendering path.

## B3. Provider-status behavior

Dedicated view fed entirely by `snapshot.providers`: six-state chips
(available / degraded / partial / unavailable / not supported / not yet
observed), per-section outcomes with counts or verbatim errors, and notes
verbatim. Unsupported sections are distinguished from failures and from
valid zero observations. A provider failure never blanks or crashes the UI
— the frontend renders whatever the snapshot contains, and the snapshot
preserves every surviving observation plus every failure report (proven by
the netlink-unavailable fixture rendering across all views).

## B4. Frontend/backend consistency

Rust remains authoritative; the frontend never derives network facts.
**Type generation deferred** (rationale in the design doc §5): the
golden-fixture contract now covers both document kinds —
`fixtures/snapshots/*.json` and `fixtures/topology/*.json` are parsed by
Rust tests (authoritative serde model) and by the frontend tests (TypeScript
mirror + validating loaders), so drift fails the suite on either side. The
precise manual sync procedure is documented in `docs/development.md`.

## B5. Exact tests and results

> **Corrected by the WO-02 correction work order (§10).** The original text
> reported "52 Rust" tests. The source contained **53 declared test
> functions** (50 `#[test]` + 3 `#[tokio::test]`), and all 53 executed —
> but the netns integration tests use a self-re-exec pattern (each test
> spawns itself under `unshare -Urn` with `--exact`), and that pattern
> called `process::exit` in the parent after the FIRST child completed.
> The netns parent harness therefore never printed its own summary; the
> re-exec'd children printed asynchronous "1 passed; 2 filtered out"
> summaries that the gate's `tail -n 40` capture truncated, and summing
> the visible lines yielded 52 — one child summary lost to the race. The
> deeper defect the correction then uncovered and fixed: the orphaned
> children's exit codes were never checked, which is how a genuinely
> failing netns test (`netns_monitor_detects…`, whose route setup never
> worked on this kernel — no connected route for an address on a DOWN
> interface) stayed hidden behind a green gate. With the corrected pattern
> (every child asserted, parent summaries deterministic) the declared and
> executed counts reconcile exactly. No test was skipped or filtered.

Same gate as A3 (**PASS 12/12**), with the WO-02 additions: 6 Rust topology
tests (including negative assertions), topology-golden stability in the
gate, 17 new vitest tests (golden invariants, parse rejection, filtering
semantics, missing-field semantics), 4 new Playwright tests (topology
counts/legend/selection/overlay-not-shrink, routes filtering/defaults/
linkage, provider honesty, cross-view navigation). Totals at the final
WO-02 gate: **53 Rust (declared = executed; see correction above), 34
vitest, 7 Playwright**, plus 11 live binary checks and the D-Bus harness
capability check.

## B6. Dependency changes and security advisories

- **Added**: `cytoscape` 3.35.0 (exact pin; the only runtime npm dependency
  besides svelte; ships its own TypeScript definitions — the separate
  `@types/cytoscape` conflicted and was removed).
- **Rust**: no new dependencies beyond the reconstructed pinned set.
- **Advisories (re-evaluated at final gate)**: `npm audit` reports **0
  vulnerabilities**, including dev dependencies. The previously recorded
  picomatch ReDoS (GHSA-fc5q-39xc-vjpf) no longer flags the resolved
  picomatch 4.0.7 used by svelte-check/vite/vitest. The assessment remains
  dev-toolchain-only exposure with zero runtime exposure; it is re-checked
  at every gate.

## B7. Remaining technical debt (proposals for a future work order —
recommendations, not authorized work)

1. **Live data in the frontend**: load real `araiguma snapshot` /
   `topology` documents (file input + Tauri command later); the documents,
   loaders, and type contracts are ready.
2. **veth pairing**: model `IFLA_INFO_DATA` veth peer ifindex and add the
   observed pairing edge (currently nodes only).
3. **Multipath expansion**: full `RTA_MULTIPATH` representation.
4. **IPv6 routes from procfs** (`/proc/net/ipv6_route`) as a further
   fallback section.
5. **Type generation (ts-rs or similar)** if the model grows; the
   golden-fixture contract covers current scale.
6. **Tauri 2 desktop shell** once a desktop environment is available.
7. **NetworkManager / systemd-resolved D-Bus providers** at their merge
   priorities (private-session harness capability already verified).
8. **Reconsider the layout dependency** (`cose`) if graphs grow large.
9. **`ip route get`-style active-path probing** — currently excluded by
   design (it is a probe, not an observation); requires its own decision.

## B8. Final handoff artifacts

At the closeout commit, in `download/`:

- `araiguma-FINAL.tar.gz` — the complete repository **including `.git`**,
  all source, tests, fixtures, lockfiles, and documentation, suitable for
  transfer to Emne's local machine for independent OpenCode audit and
  real-environment validation.
- `araiguma-FINAL.sha256` — SHA-256 checksums and a manifest identifying
  the final commit, branch, commit count, and identity.
- Earlier milestone bundles (`WO01R-FOUNDATION`, `WO02-FEATURES`) retained.

## B9. Scope compliance (explicit)

Implemented: exactly the WO-01R reconstruction scope (M0–M3) and the WO-02
scope (§7.1–7.5) as amended. Not done, not attempted, and not claimed:
Tauri, desktop/`.deb` packaging, GTK system installation, access to Emne's
machine, GitHub remote/push/CI, NetworkManager/systemd-resolved/
systemd-networkd providers, SQLite persistence, traffic accounting,
notifications, system tray, any network-control functionality, privileged
services or daemons, traffic interception, kernel modules, external
telemetry. The only deviation from the letter of the work order is the
node_modules mechanism (A9), adopted for the prescribed goal with the
reason documented. D-Bus appears only as a private-session capability
check. Testing was extended, never weakened; the sandbox-live results make
no claim about real-service compatibility.

*Prepared by the implementation agent. Emne is the sole acceptance
authority.*
