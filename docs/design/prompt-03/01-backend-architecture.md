# 01 — Backend architecture

Prompt 03 deliverable: the backend's logical and physical architecture —
layers, crate boundaries, dependency direction, the observation engine,
runtime ownership, application lifecycle, and the integration surfaces
(CLI, file import, Tauri shell, logging, tray compatibility).

Sources: OD-01 (Tauri 2 shell), OD-02 (optional capability-probed
providers with graceful degradation), OD-03 (Tauri IPC + one document
contract), OD-08 (XDG locations), OD-09 (logging design), OD-10 (Ubuntu
24.04 checkpoint), WO-02 chapters 2/5/8 (boundaries, read-only
feasibility, tray compatibility), verified baseline behavior.

## 1. What exists and what this design adds

The repository at baseline `9808196` already contains a working, tested
observation pipeline (WO-01R through WO-04 Stage A): the authoritative
`araiguma-core` domain model and merge, the `netlink` and `sysfs`
providers, the read-only CLI (`snapshot` / `watch` / `topology`), and the
Svelte 5 frontend with a six-section shell and validated document import.
[REPO — verified inventory, WO-02 Table 2.1]

This design extends that architecture; it replaces nothing. The additions
required by the approved decisions are:

1. **An observation engine** (`crates/engine`, new) that centralizes the
   orchestration today duplicated inside the CLI `watch` loop: provider
   runs, change-driven re-observation, debouncing, resynchronization, and
   envelope construction. Both the CLI and the desktop shell consume it;
   neither owns orchestration logic (section 4).
2. **A generalized provider contract** (document 02) covering capability
   probing, permission state, event support, health metadata, and
   shutdown — the slots the NetworkManager / systemd-resolved providers
   (OD-02) will occupy.
3. **Schema v4 document envelope and new entities** (documents 03–04):
   document identity, observation timing, capability metadata, DNS and
   routing-rule observations, conflict records, permission-denied
   outcomes.
4. **The desktop shell crate** (`apps/desktop`, new; the only crate that
   may depend on Tauri — OD-01) exposing the IPC surface (OD-03).
5. **Logging and diagnostics** (document 05) at the OD-08/OD-09 locations.
6. **Read-only enforcement machinery** (document 06) beyond the existing
   structural guarantees.

## 2. Layers and dependency direction

Four layers, each with exactly one reason to change and one verification
track (WO-02 §2.2, adopted):

```
┌─────────────────────────────────────────────────────────────────────┐
│ Layer 3 — Presentation           apps/frontend                      │
│   Svelte 5 + TS; renders documents; no system access; the same     │
│   validating loaders for imported AND live documents               │
└───────────────────────────────┬─────────────────────────────────────┘
                                │ documents (JSON, validated boundary)
┌───────────────────────────────┴─────────────────────────────────────┐
│ Layer 2 — Application           apps/desktop  (NEW; OD-01)          │
│   Tauri 2 shell: window, IPC commands/events, app lifecycle,       │
│   logging bootstrap. The ONLY crate that depends on Tauri.         │
│   Thin: translates IPC ⇄ engine calls; forwards documents          │
│   verbatim; no discovery, no presentation logic.                   │
└───────────────────────────────┬─────────────────────────────────────┘
                                │ engine API (documents + event stream)
┌───────────────────────────────┴───────────────────┬─────────────────┐
│ Layer 1 — Observation & domain                    │ crates/cli      │
│   crates/engine  (NEW)  orchestration: providers, │ (existing)      │
│     debounce, resync, envelope construction       │ one-shot &      │
│   crates/core    (existing)  model, merge,        │ watch CLIs;     │
│     evidence semantics, topology projection       │ same engine     │
│   crates/netlink, crates/sysfs (existing)         │                 │
│     provider implementations; future              │                 │
│     providers-dbus (OD-04/Prompt 05)              │                 │
└───────────────────────────────────────────────────┴─────────────────┘
                                │
                     Layer 4 — Contracts (cross-cutting)
                       the shared JSON document contract (OD-03),
                       schema v4 (documents 03–04); the only
                       interchange format anywhere
```

Dependency rules (enforced by review and by the test that compiles every
crate below the shell without Tauri — document 07):

| Rule | Rationale |
|---|---|
| `core` depends on no other workspace crate and on no Tauri, D-Bus, or socket code | the domain model stays pure, deterministic, and headless-testable; it is already proven so by the CLI [REPO] |
| `netlink`, `sysfs` (and the future dbus provider crate) depend on `core` only, implementing its provider trait | providers are replaceable, fixture-testable, and never leak transport types into the domain [REPO — existing trait contract] |
| `engine` depends on `core` only (provider instances are injected as `Box<dyn DiscoveryProvider>`-style trait objects; change feeds likewise) | orchestration is testable with mock providers and no Linux, and both binaries share it |
| `cli` depends on `core`, the provider crates, and `engine` | the CLI remains the headless proof of the whole pipeline [REPO, extended] |
| `apps/desktop` depends on `tauri`, `engine`, the provider crates, and `core` (serialization only) | the shell stays thin; every command maps 1:1 to an engine call (OD-03 thin-command principle, WO-02 §2.3) |
| `apps/frontend` depends on nothing from the backend; it consumes documents only | presentation never gains a discovery path [REPO — existing enforced principle] |
| No crate depends upward; no cycles anywhere | review gate + dependency-audit test (document 07) |

Only `apps/desktop` may reference `tauri` in its manifest; a static audit
step in the validation gate verifies this (document 06 §5, document 07).

## 3. Domain/core responsibilities

`crates/core` remains the single authority for (existing, extended):

- the typed entity model and its evidence semantics (document 03);
- provider identity and the provider *contract* types (document 02);
- normalization and aggregation (the merge), including conflict recording
  without provenance loss (document 03 §9);
- document construction, serialization, and validation entry points
  (document 04);
- snapshot diffing and the topology projection (existing `diff_snapshots`,
  `build_topology` — pure and deterministic [REPO]);
- the status/resolver *vocabulary* reserved for the future tray (section
  9) — vocabulary only, no tray behavior.

Core never performs I/O: it reads nothing and writes nothing; all facts
arrive as typed values from provider observations. (True today; this
design keeps it true. [REPO])

## 4. The observation engine (`crates/engine`, new)

### 4.1 Responsibility

One component owns the live-observation lifecycle so that the CLI and the
desktop shell are interchangeable consumers rather than parallel
orchestrators. Today this logic lives inside `run_watch` in the CLI
[REPO — `crates/cli/src/lib.rs`]; the engine extracts it unchanged in
semantics:

- construct and hold the injected provider set (ordered by priority);
- run observation passes on demand and on change notification;
- debounce notification bursts (existing 150 ms window [REPO]);
- translate monitor signals into the envelope stream: `snapshot`,
  `changed` (pure snapshot diffs), `resynchronized` (full fresh snapshot;
  changes during any gap were NOT observed) [REPO — existing watch
  contract];
- maintain the *current document* (the latest validated snapshot) and a
  monotonic envelope sequence number (document 04 §7);
- expose engine health and per-provider health (document 02 §8);
- drive optional poll fallback when no event source is available
  (document 02 §6).

### 4.2 Interface shape (contract level)

```
ObservationEngine::new(providers, change_feeds, policy) -> Engine
Engine::snapshot_document() -> SnapshotDocument       // current, validated
Engine::observe_now() -> SnapshotDocument             // forced pass
Engine::subscribe() -> EventStream                    // envelope stream
Engine::shutdown()                                     // ordered teardown (§6.3)
EventStream item = Envelope { seq: u64, kind: snapshot|changed|resynchronized, ... }
```

The engine is runtime-agnostic: it contains no `#[tokio::main]`, no
runtime construction, and no Tauri types; it is driven by whichever async
runtime the embedding binary provides (section 8). `ChangeFeed` is a
core-level trait ("yield the next change signal, or a resynchronization
marker"); the existing netlink `Monitor` semantics
(`Signal::Changed { kind }` / `Signal::Resynchronized`, exponential-backoff
reconnect, ENOBUFS overrun handling [REPO — `crates/netlink/src/monitor.rs`])
become its first implementation.

### 4.3 Normalization and aggregation

Aggregation stays exactly the existing merge discipline [REPO —
`crates/core/src/merge.rs`, WO-03-F AUD-02/03/05]: priority-ordered
field-wise merge (higher priority wins, lower fills gaps only), evidence-
based deduplication under complete route identity, ambiguity preserved
with explicit notes rather than resolved by iteration order. Schema v4
adds conflict records when two providers assert different non-null values
for the same field (document 03 §9): precedence still applies, but the
conflict is retained and visible — evidence is never silently discarded.

## 5. Provider implementations (Linux-specific layer)

Existing: `netlink` (rtnetlink dumps — the primary, priority 0; multicast
monitor) and `sysfs` (priority 1; `/sys/class/net`, `/proc/net/*`)
[REPO]. Planned within OD-02, contract specified in document 02,
internals deferred to Prompt 05:

- **NetworkManager D-Bus provider** — active connections, device-to-
  connection mapping, VPN evidence, Wi-Fi details where exposed;
  capability-probed, never required.
- **systemd-resolved D-Bus provider** — per-link and global resolver
  configuration; capability-probed, never required.
- **systemd-networkd** — only as far as observable read-only state
  warrants (WO-02 §4; the work order requires the architecture to account
  for it; its concrete sections are a Prompt 05 finding).

The universal baseline guarantee (OD-02, R-02): rtnetlink + sysfs alone
always produce a useful, honestly-degraded document. No provider is ever
assumed installed, active, accessible, or authoritative (document 02 §3).

## 6. Application state and lifecycle

### 6.1 Session model

A **session** is one process incarnation of the application (CLI one-shot,
CLI watch, or desktop run) with a process-unique session id (UUID v4,
document 04 §5). Sessions are the unit of log correlation and diagnostics
export (document 05). Documents produced by a session carry its
collection identity (session id + collection counter), never the session's
private state.

### 6.2 Startup sequence (all binaries)

1. Load configuration from `~/.config/araiguma/config.toml` (OD-08;
   defaults when absent; malformed file ⇒ defaults + warning, never a
   crash — document 05 §3).
2. Initialize logging to `~/.local/state/araiguma/logs/` (OD-08; stderr
   fallback when unwritable — document 05 §8).
3. Allocate the session id; log startup (versions, config path, XDG
   paths).
4. Construct providers; run capability probes (document 02 §4) — no
   observation yet, no network traffic, no D-Bus method calls beyond
   allow-listed presence checks.
5. First observation pass; construct and validate the first document.
6. Enter steady state: event subscriptions live; poll fallback armed
   where no feed exists; envelopes flow to subscribers.
7. Log provider capability/permission outcomes (startup evidence).

### 6.3 Shutdown sequence (all binaries)

Shutdown is cooperative and ordered, with a bounded grace period
(recommended default 5 s, configurable — RD-14):

1. Stop accepting new subscriptions / commands (desktop: emit a typed
   `shutdownInProgress` IPC error — document 04 §6).
2. Signal the engine; providers stop in-flight observation cooperatively.
3. Tear down change feeds and D-Bus matches; close sockets.
4. Flush and close the log (a shutdown record with session statistics).
5. Exit 0. On grace-period expiry: log the hung component and exit 1 —
   never hang silently.

Providers own their resources (sockets, matches, tasks); the engine never
reaches into provider internals (document 02 §10).

### 6.4 Crash behavior

No atexit-style mutation exists anywhere (nothing to clean up in the
network); a crashed session leaves at most a truncated last log line and
an unterminated session record. The next session's startup detects the
stale session record and marks it `unclean` in the session registry
(document 05 §6). A crashed session never corrupts a document: documents
are constructed in memory and serialized atomically (document 04 §8).

## 7. Integration surfaces

### 7.1 CLI (`crates/cli`, existing)

`araiguma snapshot [--pretty]`, `araiguma watch`, `araiguma topology
[--input FILE]` remain, now emitting schema v4 documents through the
engine (document 04). The CLI stays the headless proof that everything
below the shell works without a GUI — a property the existing 17-step gate
already enforces and this design keeps load-bearing [REPO]. CLI process
lifecycle: one-shot (snapshot/topology) or watch (subscribe + print
NDJSON to stdout until signal — existing behavior, now via the engine).

### 7.2 File import (existing, formalized)

Import is a pure document operation: parse, validate fully, and either
accept (document enters the imported-documents store with its ORIGINAL
observation time and a SEPARATE import time) or reject with a
path-qualified reason; an invalid document never disturbs the current
valid view (atomic replacement — document 04 §8; existing frontend
behavior, WO-03 M1 + WO-03-F AUD-09 [REPO]). Imported documents are never
presented as live observations (existing status-bar discipline [REPO];
formalized in document 04 §9). Import works fully offline.

### 7.3 Tauri desktop shell (`apps/desktop`, new — OD-01, OD-03)

The shell crate contains, and only contains:

- Tauri app bootstrap (window, tray registration only when a future work
  order authorizes it — OD-07 stays open);
- the IPC command handlers of document 04 §5 (each a thin mapping to one
  engine call);
- event emission of document 04 §7 (envelopes forwarded verbatim);
- logging/diagnostics bootstrap (document 05);
- shutdown wiring (§6.3).

No discovery, no presentation logic, no document mutation. The handlers
are transport-neutral functions the desktop crate binds to Tauri — so the
entire IPC logic is testable headless (document 07). Frontend live mode
receives the same documents through the same validating loaders as file
import (E-3, WO-02 §2.4): one contract, one boundary, three transports
(CLI stdout, file, IPC) — OD-03.

### 7.4 Logging and diagnostics

Owned by a small logging module used by all binaries (locations and
behavior per document 05; OD-08/OD-09). Providers log through the same
facility with their provider id as a structured field; the engine adds
session/collection correlation ids.

## 8. Asynchronous runtime ownership (Tokio)

Unambiguous rules (extending the WO-02 §2.3 runtime note; OD-01):

1. **The embedding binary owns the runtime.** The desktop shell uses
   Tauri 2's async runtime (Tokio-based `tauri::async_runtime`), the CLI
   uses `#[tokio::main]`, tests use `#[tokio::test]`. [OD-01]
2. **Library crates never construct a runtime.** `core`, `netlink`,
   `sysfs`, `engine`, and the future dbus provider crate contain no
   `Runtime::new`, no `block_on`, and no nested-runtime patterns. Tasks
   are spawned onto the *current* runtime context (the netlink monitor
   already documents this requirement [REPO — `monitor.rs`]). A static
   audit test enforces the rule (document 07).
3. **No blocking on the async executor.** File I/O and any blocking
   syscall in async context go through `spawn_blocking` (or dedicated
   blocking threads); provider `observe()` implementations document their
   blocking surface.
4. **Task ownership is explicit.** Every spawned task has a named owner
   (a provider, the engine, the shell) that is responsible for its
   cancellation at shutdown; the engine tracks its tasks and joins or
   aborts them in §6.3 order. No orphaned background tasks.
5. **Backend tests and CLI run without launching Tauri** — guaranteed by
   rule 1 and enforced by the gate's existing headless runs [REPO].

## 9. Future tray compatibility — architectural constraints only

WO-TRAY-01 remains separate and unauthorized (OD-07 open). This design
preserves exactly these integration points and nothing more (WO-02
chapter 8):

- **Status resolver home reserved, not built.** Document 03 §11 defines
  the status *vocabulary* (evidence-derived, total, non-lossy) and its
  absorbing Unknown; the resolver itself — a pure function over evidence
  states, platform-independent, living in core — is future work. The
  priority order Critical > Unread > New connection > Attention > Normal
  with Unknown absorbing (never coerced to Normal) is recorded there as
  a constraint the future resolver must satisfy.
- **Typed event contracts usable by multiple consumers.** The engine's
  envelope stream (document 04 §7) is consumed by the CLI printer and by
  the desktop emitter today; a future tray is just another subscriber.
  Event types carry provenance and unknowns; they never assert network
  facts the documents do not contain.
- **Event-stream discontinuity has resynchronization semantics.** The
  existing `resynchronized` discipline (changes during a gap were not
  observed; the fresh snapshot is authoritative again [REPO]) extends to
  IPC with sequence numbers (document 04 §7): a consumer that detects a
  gap must resynchronize rather than continue on incomplete state; a
  future tray must treat post-resync state as fresh evidence, never as
  "nothing happened" (R-10).
- **Non-lossy evidence-to-status mapping.** Any future status derivation
  consumes the evidence vocabulary (document 03 §8) as a total function;
  an evidence gap maps to Unknown, never to Normal.
- **Stable navigation destinations with optional selected-object
  references.** The Stage A destination API (six stable section ids;
  `navigate({section, selection?})`; identity-based resolution; an
  unresolvable selection clears rather than renders stale [REPO —
  `lib/navigation.svelte.ts`]) is the single navigation surface any
  future tray action routes through; no parallel navigation state is
  introduced by this design.
- **Poll-sourced statuses are timestamped observations** — any future
  status derived from poll fallback displays its observation age, never
  implying a live signal (document 02 §6).

Explicitly NOT designed or built here: the status resolver, tray event
bus, notification lifecycle, unread/acknowledgement lifecycle, tray menu,
native tray behavior, tray icons, icon integration (WO-TRAY-01 scope;
persisted ack-state location remains that work order's open question).

## 10. Traffic-accounting extension points (Prompt 06 — reserved only)

To keep Prompt 06 integrable without re-architecture, this design reserves
— and does not implement or fully specify (work order §3.4):

1. **Per-interface statistics observations** as an optional provider
   section (`interfaceStats`): sysfs `statistics` files are already
   fixture-present [REPO — `fixtures/sysfs/*/class/net/*/statistics`] and
   the section vocabulary is extensible (document 02 §5), so a counters
   section requires no contract change — only a Prompt 06 decision.
2. **Document-level optional sections**: schema evolution policy
   (document 04 §6) permits additive, defaulted sections; an
   `accounting` section (counters, allowance model, reset semantics)
   would be additive and versioned there.
3. **Identity stability**: interface identity (ifindex + name + namespaced
   peers, document 03 §4) is already sufficient to correlate counter
   series across observations.
4. **Timing**: the envelope's observation timing (document 04 §5) gives
   rate derivation its time base.

Nothing in the current pipeline computes, estimates, or displays traffic
accounting; no allowance model exists; Prompt 06 owns all of it.

## 11. Architecture decision summary (traceability)

| Decision (this document) | Source | Status |
|---|---|---|
| Four layers; only `apps/desktop` depends on Tauri | OD-01; WO-02 §2.2 | approved-derived |
| New `crates/engine` owning orchestration | WO-02 §2.3 (thin-command principle); avoids CLI/shell duplication | RD-01 (recommendation) |
| Core stays I/O-free, pure, deterministic | existing verified property [REPO] | binding invariant |
| Providers injectable; engine runtime- and platform-agnostic | OD-02; testability (Track 1) | approved-derived |
| Merge precedence retained; conflicts recorded, never discarded | work order §3.2; existing merge [REPO] | approved-derived |
| Binary owns runtime; libraries never construct one | OD-01; WO-02 §2.3 runtime note | binding rule |
| Session/collection identity; ordered cooperative shutdown | work order §3.1, §3.7 | RD-02 (recommendation) |
| Tray compatibility as constraints only | OD-07 (open); WO-02 ch. 8; work order §4 | approved-derived (scope) |
| Accounting extension points reserved | work order §3.4 (Prompt 06) | approved-derived (scope) |
