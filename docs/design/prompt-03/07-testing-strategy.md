# 07 — Testing strategy and validation matrix

Prompt 03 deliverable: the test architecture that makes this design
verifiable — every category the work order lists, the two-track
validation model, the OD-10 Ubuntu 24.04 checkpoint, and the read-only
state-capture comparison approach.

Sources: the existing test architecture and 17-step gate [REPO —
`docs/architecture.md`, `scripts/test-all.sh`, WO-02 correction §A5,
WO-03-FC fresh-clone discipline], OD-10 (local validation checkpoint),
WO-02 chapter 6 (capability matrix, Track 1/Track 2), work order §5.

Standing rules (existing, binding): checks are never weakened to go
green [REPO]; a test is only claimed when actually executed with its
result recorded; the sandbox makes **no** claim about services it does
not have (no system D-Bus, no NetworkManager, no systemd, no
WebKitGTK [ENV — WO-02 §6.1]) — those belong to Track 2.

## 1. The two-track model (unchanged, load-bearing)

- **Track 1 — sandbox, continuous** (every commit): core crates,
  engine, CLI, document contracts, frontend, browser GUI, netns kernel
  behavior, D-Bus client code against a private session-bus mock
  harness (verified available [ENV — WO-02 §6.1]), storage budget,
  static audits (document 06 §6).
- **Track 2 — local Ubuntu 24.04 machine, scheduled checkpoints**
  (OD-10): everything requiring the real desktop stack — §9.

## 2. Existing layers (kept, extended — never weakened)

The existing suite [REPO]: unit (model serde, merge precedence, dedup,
provider states, diff, kernel-format parsing, netlink translation
without sockets); fixtures (schema-locked, dual-language); netns
integration (transient `unshare -Urn` namespaces; host untouched);
live cross-validation (iproute2 JSON inside a netns); frontend unit
(validators, formatters, fixture contract); browser e2e (production
build); the gate (fmt/clippy/budget/identity/fresh-clone). New tests
land in these layers; the matrix in §10 maps every new requirement
into them.

## 3. Domain-model unit tests (new: v4 entities and evidence)

- `RoutingRule`, `DnsObservation`, conflict entries, health/capability
  types: serde round-trips (camelCase, defaults, unknown-provider
  passthrough — document 02 §2), identity/equality rules, sorted-order
  determinism.
- Evidence-vocabulary totality: every entity type has, for each of its
  evidence-bearing fields, a test per applicable state of document 03
  §1 (a field cannot accidentally conflate `notObserved` with
  `observedAbsent` — the v1 MAC class of defect, guarded generatively
  where practical).
- Projection tests: `build_topology` over v4 fixtures (rules and DNS
  contribute **no** edges — negative assertions; VPN evidence yields
  annotations, never edges).

## 4. Provider contract tests (conformance suite — new)

A shared conformance harness any provider implementation must pass
(mock sources injected per the existing fixture-root pattern [REPO]):

- capability report truthfulness (declared sections = attempted
  sections; unavailable/permissionDenied paths produce the right
  states, document 02 §3);
- section-outcome discipline: zero-item success is `Observed { count: 0 }`
  (observed empty), never failure [REPO]; missing source file is
  failure, never observed-empty [REPO — WO-03 M4 rule];
- partial-failure preservation (one failed section never discards
  others [REPO]);
- health metadata accuracy (lastSuccessMs, consecutiveFailures);
- shutdown: idempotent, resource-free afterwards, no orphaned tasks
  (document 02 §10) — asserted with task/spawn instrumentation;
- read-only proof: every provider type parameterized into the §8
  invariant test;
- D-Bus providers (Track 1): run against the private session-bus
  harness with a **mock service object** exposing exactly the
  allow-listed surface (document 06 §4) — including mock services that
  *offer* prohibited members, unlisted properties, `Properties.GetAll`,
  and `ObjectManager.GetManagedObjects`, asserting the restricted proxy
  never calls them and that unknown operations are unreachable
  (R-03 mitigation; document 06 §4.1/§4.6; Prompt 03-FC FC-02 —
  executable in Track 1).

## 5. Serialization, schema-compatibility, and validation tests

- Round-trip: every fixture parses in Rust and TypeScript (existing
  dual-lock discipline [REPO]) — extended to v4 fixtures (envelope,
  meta, new sections, conflicts, synthetic labeling).
- Schema compatibility (document 04 §4): committed golden documents at
  v1 (unambiguous-MAC acceptance + null rejection), v2, v3, v4 all
  parse; unknown versions reject with the supported range stated
  (existing tests [REPO], extended); `synthetic:false` +
  `test-fixture` channel rejects (document 04 §2).
- Import validation and atomic replacement: malformed documents
  rejected whole with path-qualified reasons; current valid view
  unchanged after rejection (existing tests [REPO — AUD-09], extended
  to live-store replacement and IPC-carried documents).
- Validation totality: boundary validators reject every class of
  structural violation (the existing malformed-document corpus [REPO],
  grown with v4 cases).
- Diagnostics redaction tests (document 05 §9.1): field-allowlist
  totality (no field outside document 05 §9's list can appear in an
  export); the pattern pass on representative inputs — MAC addresses
  in all three `privacy.mac_addresses` modes (default asserts the
  `<redacted>` marker with no octets retained), IPv4/IPv6 literals in
  both `privacy.ip_addresses` modes, SSID-bearing, VPN-name-bearing,
  home-path (generalized to `~/…`), and secret-bearing strings (asserted
  absent under every setting); the §3 detail-string content rule corpus
  (no SSID, VPN-name, hostname, or path content in `detail`).
  Prompt 03-FC FC-03.

## 6. IPC request/event tests (headless — new)

- The transport-neutral command handlers (document 04 §10) are tested
  as plain functions: request/response shapes for **all four commands**
  (document 04 §9), error codes (`invalidRequest` with path-qualified
  detail, `shutdownInProgress`), and the complete subscription
  lifecycle (Prompt 03-FC FC-01):
  - subscribe → first envelope is `snapshot`, handle carries a fresh
    unguessable `subscriptionId`;
  - unsubscribe (active) → `Cancelled`; exactly one terminal `ended`
    envelope whose `seq` is the highest delivered document-event seq;
    no document-event envelope delivered after `ended`;
  - repeated unsubscribe (already terminated) → idempotent `Cancelled`,
    **no** second `ended`;
  - unsubscribe with an unknown id (well-formed, never issued this
    session) → `invalidRequest` naming the id;
  - disconnect cleanup: frontend/window close terminates and releases
    every subscription server-side — no leaked subscription state,
    channel, or delivery task survives (asserted with task/spawn
    instrumentation);
  - shutdown cleanup: every active subscription receives `ended`; the
    next command returns `shutdownInProgress`; no orphaned delivery
    task remains (asserted after engine shutdown);
  - `ended` consumes no seq: the engine's document-event counter is
    unchanged by terminations.
- Envelope contract: seq monotonicity, in-order delivery, envelope
  payload equality with CLI `watch` output for the same engine state
  (the two transports are one contract — OD-03, proven by construction
  here).
- `get_topology` projections: current-document and caller-supplied
  snapshot paths; invalid supplied documents reject at the boundary.

## 7. Event continuity and resynchronization tests

- Gap detection: simulated seq discontinuity → consumer recovery is
  snapshot-first (document 04 §8) — tested with a scripted engine
  (injected signals).
- Resync semantics: reconnect and ENOBUFS overrun both produce
  `resynchronized` with a full fresh snapshot (existing monitor tests
  [REPO — overrun path] extended through the engine); post-resync
  consumers must not derive "nothing changed" claims (asserted on the
  test consumer's state machine).
- Debounce: bursts collapse into one `changed` envelope (existing
  behavior [REPO], preserved through the engine).
- Monitor health (WO-05 Stage 1 amendment): disconnect, failed reconnect,
  successful reconnect, and recovery are exercised independently. A
  disconnect/failed attempt publishes `monitorHealth: degraded`; recovery
  publishes a fresh `resynchronized` snapshot before
  `monitorHealth: recovered`. The frontend preserves the last confirmed
  pair as STALE (or reports UNAVAILABLE when none exists) and restores LIVE
  only after the recovered signal.

## 8. Read-only invariant tests (before/after state comparison)

Specification of the state-capture approach (specification only — no
privileged or destructive testing is performed by this work order):

1. **Capture**: inside the observation environment, record the full
   observable network state with the *test harness's* tooling (iproute2
   in tests, as today [REPO] — the application itself never shells
   out):
   `ip -j link`, `ip -j address`, `ip -j route show table all`,
   `ip -j rule` — each snapshot JSON-normalized (sort keys, drop
   volatile fields: nothing in these outputs is timestamped; link
   statistics counters are volatile → stripped from comparison when a
   run crosses traffic, else byte-compared).
2. **Exercise**: run every provider's `observe()` (full pass), the
   monitor with signal subscription, and an engine cycle with
   debounced re-observation.
3. **Compare**: capture again; assert normalized equality of all four
   state families.
4. **Environments**: Track 1 — a transient `unshare -Urn` namespace
   (the existing harness pattern [REPO]); Track 2 — the real host
   (OD-10 checkpoint), where the same script runs with the user's own
   privileges against the real system, proving the invariant on real
   NetworkManager/systemd-resolved D-Bus mediation.
5. **Scope honesty**: the comparison proves the *observable surface*
   (links/addresses/routes/rules) is untouched; it cannot prove
   non-observable side effects — which is why L1/L4 structural
   enforcement exists (document 06 §3). The test is the executed
   backstop, not the only mechanism.

Provider-failure and graceful-degradation tests: inject failures per
provider (socket errors, missing files, D-Bus denials in the mock
harness) and assert the surviving document (other providers'
observations stand, failures reported per section, states aggregate per
the existing rules [REPO], no crashes, no hangs, shutdown still clean).

## 9. Track 2 — the OD-10 Ubuntu 24.04 validation checkpoint

Scheduled after Prompt 03, before Tauri implementation (OD-10 —
binding). Contents and pass criteria (the checkpoint script is
authored at implementation time; this is its specification):

| # | Check | Pass criteria |
|---|---|---|
| 1 | Toolchain bring-up on the local machine | documented bootstrap completes; version report matches the pinned toolchain |
| 2 | **Thin Tauri slice** | a minimal `apps/desktop` window opens (WebKitGTK runtime OK — R-01), serves one IPC command end-to-end into the real frontend loaders |
| 3 | .deb lifecycle | install → launch → **upgrade** (older→newer .deb; XDG state survives — R-09) → remove; no privileged helper, no systemd unit, no postinst network configuration |
| 4 | Live NetworkManager probes | provider probes a real NM: capability report correct; **permission behavior validated** (unprivileged property reads succeed; every prohibited member demonstrably never called — audit-logged) |
| 5 | Live systemd-resolved probes | per-link + global DNS observations on a real resolver; permission behavior as above |
| 6 | Read-only invariant on real host | §8 script passes on the real system |
| 7 | Netns suite on real kernel | the full workspace test suite (including netns tests) green on the local machine |

Everything else in this document is Track 1 and must pass in the
sandbox before Track 2 begins.

## 10. Validation matrix (requirement → tests → track)

| Requirement (source) | Test layer (this document) | Track |
|---|---|---|
| Evidence semantics precise; unknown never coerced (work order §9.4) | §3 evidence totality; existing state-taxonomy tests [REPO] | 1 |
| Incomplete/stale/conflicting/unresolved representable (§9.5) | §3 (entities), §5 (validation), conflict-merge unit tests | 1 |
| No invented topology/gateway/DNS/connectivity facts (§9.6) | projection negative assertions [REPO pattern] + v4 no-edge tests | 1 |
| One document contract serves CLI/import/IPC (§9.7) | §6 envelope-equality-by-construction; §5 dual-language locks [REPO] | 1 |
| IPC recovery from discontinuity (§9.8) | §7 continuity suite | 1 |
| Runtime/task ownership unambiguous (§9.9) | §4 shutdown assertions; static audit (document 01 §8 rule 2) | 1 |
| Read-only enforceable and testable (§9.10) | §8 invariant (both tracks); document 06 §6 audits; §4 mock-service prohibitions | 1 + 2 |
| Logging bounded; privacy controls (§9.11) | document 05 tests: rotation/bound unit tests; redaction allowlist tests (§5 here) | 1 |
| Tray compatibility preserved (§9.12) | status-vocabulary totality tests (document 03 §11 reservation); destination API tests [REPO — Stage A] | 1 |
| Ubuntu-only validation identified (§9.13) | §9 checkpoint | 2 |
| Provider failure ≠ network absence (§9.3) | §8 degradation injections; existing aggregation tests [REPO] | 1 |
| Backend authoritative (§9.2) | frontend never-imports-network-facts audit (existing review discipline [REPO]); projection-authority tests | 1 |

## 11. Decision summary

| Decision | Source | Status |
|---|---|---|
| Two-track model retained; sandbox claims nothing about absent services | WO-02 ch. 6; existing discipline [REPO] | approved-derived |
| Shared provider conformance suite with injected mock sources | work order §5; existing fixture-root pattern [REPO] | RD-28 |
| Mock D-Bus services offering prohibited members; proxy proven not to call them | R-03; document 06 §4 | RD-29 |
| Before/after normalized state comparison (links/addresses/routes/rules) in netns + real host | WO-02 Layer 3; OD-10 | approved-derived |
| OD-10 checkpoint contents fixed as §9's seven checks | OD-10 | approved (binding) |
| Envelope equality between CLI and IPC asserted by construction | OD-03 | RD-30 |
