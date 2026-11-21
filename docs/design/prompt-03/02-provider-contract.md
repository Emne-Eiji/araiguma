# 02 — Provider contract and lifecycle specification

Prompt 03 deliverable: the common provider contract, its lifecycle, and
the reconciliation rules for observations from multiple providers.

Sources: OD-02 (optional, capability-probed NetworkManager/systemd-resolved
providers; graceful degradation), the existing `DiscoveryProvider` trait
and six-state provider health [REPO — `crates/core/src/provider.rs`,
WO-02 correction §7], WO-02 §4 (service assessment), §5 (read-only
feasibility), E-1/E-2 (extension points), R-02/R-03 (risks).

This document specifies the **contract**. Provider internals — D-Bus
session handling, specific property mappings, zbus adoption — are Prompt
05 scope (UD-01/OD-04 open). The contract is designed so Prompt 05 can
implement against it without architectural change.

## 1. Design posture

- **Providers are optional.** No provider is assumed installed, active,
  accessible, or authoritative on any target (OD-02). The universal
  baseline (netlink + sysfs) always yields a useful view [REPO].
- **Providers are read-only by construction.** The trait offers no
  mutation pathway [REPO]; document 06 defines the enforcement machinery
  (allow-lists, audits, tests).
- **Providers never throw away partial results.** A failing section is
  recorded as failed while other sections keep their observations [REPO].
- **A provider is a source of observations, not an oracle.** Provider
  health and network health are distinct facts; provider absence is never
  evidence that the corresponding network objects do not exist.

## 2. Provider identity (E-1 resolved)

The baseline `ProviderId`/`ObservationSource` enums are closed
(`netlink`, `sysfs`, and the source-level `procfs`) [REPO]. The contract
generalizes identity (schema v4, document 04):

- The serialized identity is a **stable lowercase camelCase string**
  (`"netlink"`, `"sysfs"`, `"procfs"`, `"networkManager"`,
  `"systemdResolved"`, `"systemdNetworkd"`).
- Readers accept **unknown identity strings** and preserve them verbatim
  (the model's existing `Other(...)` discipline for unmapped kernel
  values [REPO] applied to provider identity): a document written by a
  newer build must not lose provenance in an older reader.
- Identity participates in **merge priority ordering**; unknown providers
  sort after all built-ins (their observations merge, they never
  displace a built-in's field precedence — document 03 §9).
- Identity is *not* capability: two builds may expose different sections
  under the same id; the capability block (§5) states what this
  provider, in this environment, can observe.

## 3. Availability, permission, and support — three distinct axes

The contract separates four facts the baseline conflates into
section outcomes [extension of the existing six-state model]:

| Axis | Question it answers | Representation |
|---|---|---|
| **Availability** | Can the provider run at all here? (service/binary/kernel interface present; bus reachable) | `probe()` capability report |
| **Permission** | Is the application *allowed* to use it? (D-Bus policy, file access modes) | `probe()` capability report, per-section where applicable |
| **Support** | Which sections can this provider observe *in principle*? | capability declaration (§5) |
| **Health** | What did the last run actually achieve? | `ProviderState` per run [REPO] + health metadata (§8) |

Rules:

1. **A provider that cannot run is `unavailable`, and nothing more.** Its
   sections report `unavailable` (document 03 §8); the corresponding
   entities are `notObserved`, never "absent".
2. **Permission denial is explicit.** A section that failed *because
   access was denied* records `permissionDenied` with the mechanism
   (e.g. `dbus-denied`) — distinct from a generic failure. The provider
   state aggregates it as today's rules aggregate failures [REPO], but
   the discrimination survives into documents, logs, and the UI so a
   user can tell "not installed" from "not permitted" from "broke
   mid-run". This distinction is exactly what OD-10's live permission
   probes will validate on the Ubuntu 24.04 host.
3. **Degrade, never block.** An unavailable or denied provider never
   prevents startup, observation, or shutdown; the remaining providers'
   observations stand [REPO — merge rule 3].
4. **No privilege seeking.** A denied provider does not retry
   aggressively, does not prompt, does not attempt elevation or any
   workaround; bounded backoff only (§7). Escalation to the user is a
   UI concern fed by the document's explicit state.

## 4. Lifecycle

```
            construct                 probe (capability discovery)
  ┌─────────┐        ┌──────────────┐        ┌────────────────────┐
  │Created  ├───────►│Probing       ├───────►│Probed              │
  └─────────┘        └──────┬───────┘        │ ready|unavailable| │
                          │ probe error      │ permissionDenied   │
                          ▼                  └─────────┬──────────┘
                   (Probed: error state)               │ start()
                                                          ▼
                            ┌────────────────────────────────────┐
                            │Active ── observe() ⇄ events/poll    │
                            │  per-run: healthy|degraded|partial|│
                            │            unavailable             │
                            └─────────┬──────────────────────────┘
                                      │ shutdown()
                                      ▼
                            ┌──────────────────┐
                            │Stopped (terminal)│  resources released
                            └──────────────────┘
```

- **Construct** is cheap and side-effect-free (no I/O, no sockets).
- **Probe** performs capability discovery only: presence checks and
  allow-listed, read-only presence queries (document 06 §4). Probing
  never observes network entities and never mutates anything. Probing is
  idempotent and re-runnable (a desktop session may re-probe after a
  system change signal).
- **Active** is the only state in which observation happens.
- **Stopped** is terminal: sockets closed, D-Bus matches removed, tasks
  joined, resources released (§10). A stopped provider is discarded, not
  restarted; the owner constructs a fresh instance (keeps lifecycle
  reasoning trivial and testable).

Re-provisioning (service appears/disappears mid-session): the *engine*
may re-probe and swap provider instances at run boundaries (never
mid-observation); a swap is recorded in the next document's capability
metadata so provenance stays honest.

## 5. Capability declaration and discovery

Every provider declares, statically and per-environment:

```
CapabilityReport {
  provider: ProviderId,
  available: Available | Unavailable { reason } ,
  permission: Granted | Denied { mechanism, reason } | NotApplicable,
  supportedSections: [SectionKind],     // in principle
  eventSupport: None | ChangeFeed { kinds },   // §6
  fidelityNotes: [String],              // verbatim caveats (Degraded driver)
  implementationVersion: String,        // provider build/version string
}
```

- `probe()` produces a `CapabilityReport`; it is embedded in schema v4
  documents (document 04 §4) so every document states what *could* and
  *could not* have been observed — the foundation for honest "not
  observed" presentation (a section absent from a document may mean
  unsupported-by-this-environment, and the report says which).
- Sections declare support *in principle*; a run may still fail a
  supported section (health, §8). This preserves the baseline's
  distinction between `Unsupported` and `Failed`/`NotObserved` [REPO].
- `fidelityNotes` feeds the existing `Degraded` state (sysfs
  classification ambiguity is the historical example [REPO]).

## 6. Observation, refresh, and event-driven updates

- **On-demand observation**: `observe() -> ProviderObservation` — the
  existing contract [REPO]: per-section results, never panics, partial
  results preserved.
- **Event-driven updates**: a provider (or its crate) may additionally
  offer a `ChangeFeed` — an async stream of `Changed { kinds }` /
  `Resynchronized` signals with the existing netlink monitor's semantics
  (a gap means "changes were not observed; re-dump") [REPO]. The engine
  subscribes feeds, debounces bursts (150 ms [REPO]), and re-observes.
- **Poll fallback**: providers without feeds are polled by the engine on
  a configurable interval (recommended default 30 s; RD-08) — but only
  when the application runs in a mode that displays live data; one-shot
  CLI commands never poll. Poll-sourced observations carry their own
  timestamps; any consumer presents their *age*, never implying a live
  signal (tray constraint, document 01 §9).
- **Refresh semantics**: an `observe()` pass replaces that provider's
  contribution at the *next* document construction; in-flight documents
  are never mutated. The engine is the only caller of `observe()` (no
  concurrent double-observation of one provider instance).

## 7. Provider errors and recovery

- **Errors are values, not exceptions**: per-section `Failed { error }`
  [REPO], extended with `PermissionDenied { mechanism, detail }` (§3).
  Provider code never panics across the boundary (existing rule [REPO]).
- **Recovery**: transient failures are retried by the engine with
  bounded exponential backoff (recommended: 250 ms doubling to 8 s — the
  existing monitor backoff values [REPO]; RD-09), tracked in health
  metadata. Backoff never delays shutdown (§10 aborts immediately).
- **Permanent conditions** (service removed, permission revoked) surface
  as `unavailable`/`permissionDenied` capability states on the next
  re-probe; the engine schedules re-probes conservatively (recommended
  60 s; RD-10) rather than hammering a dead bus.
- **Error strings are structured-enough to classify** — the error message
  must name the failing resource (e.g. `dbus call
  org.freedesktop.NetworkManager:GetDevices: AccessDenied`) so logs and
  documents carry actionable detail without free-form parsing.

## 8. Provider health and last-successful-observation metadata

Each document's provider report (schema v4) extends the existing
`ProviderReport` [REPO] with health metadata:

```
ProviderHealth {
  state: ProviderState,            // existing six states [REPO]
  lastAttemptMs: Option<u64>,      // last observe() attempt end
  lastSuccessMs: Option<u64>,      // last run with ≥1 section observed
  consecutiveFailures: u32,
  lastError: Option<String>,       // verbatim, from the most recent failure
}
```

- Health describes **the provider**, never the network: a
  fully-available provider on a machine with one loopback interface is
  healthy; a dead provider on a richly-connected machine is dead. Views
  must not derive network-health claims from provider health (work order
  §3.5).
- `lastSuccessMs` gives consumers an honest freshness base: staleness is
  *derived* against a freshness policy (document 03 §10), never guessed.
- Attention lists (any view that lists "what needs attention") describe
  **evidence limitations** ("resolver configuration not observed:
  systemd-resolved unavailable") and never assert network faults
  ("DNS is down") without supporting evidence.

## 9. Cross-provider reconciliation

Reconciliation is the merge layer's job (document 03 §9 summarizes the
entity-level rules); the provider contract supplies its inputs:

1. **Provenance is preserved per item**: `sources` lists every provider
   that observed an item, priority-first [REPO]. Merging never rewrites
   a provider's individual observation (the engine retains per-provider
   observations until document construction).
2. **Precedence without evidence loss**: field conflicts resolve by
   priority (existing rule [REPO]) but are *recorded* as conflict
   entries (schema v4) — both values and their sources survive
   (document 03 §9). Precedence is a presentation choice; the evidence
   is the document's content.
3. **Duplicate observations unite** (same kernel fact via two sources)
   — the existing evidence-based deduplication under complete identity
   [REPO — WO-03-F AUD-02].
4. **Disagreement about *existence*** (one provider observes an entity
   another does not): the entity is present with its observer's
   provenance; the non-observing provider's capability report explains
   why it may not have seen it. Absence is asserted **only** by a
   successful, complete observation of the enclosing scope (document 03
   §8); a provider's silence is never interpreted as the entity's
   absence.
5. **No provider is authoritative over another's domain**: netlink wins
   *field conflicts* by priority, but a DNS section observed only by
   systemd-resolved is that section's evidence, full stop. Capability
   sections are disjoint by design where possible; overlaps (interfaces
   via netlink vs. NetworkManager devices) reconcile through rules 1–4.

## 10. Shutdown and resource ownership

- Each provider owns every resource it created (sockets, D-Bus
  connection/matches, spawned tasks, file handles) and releases them in
  `shutdown()`.
- `shutdown()` is async, idempotent, and fast; it must complete within
  the engine's grace period (document 01 §6.3). On expiry the engine
  drops the provider (tasks aborted) and logs which provider hung.
- Providers never share resources with each other (no cross-provider
  sockets); sharing a process-wide D-Bus *connection* is a Prompt 05
  implementation choice permitted only if the connection's ownership is
  explicitly delegated to one owner (the engine or the shell) with
  reference semantics the contract can still shut down deterministically
  (RD-11).
- No provider may leave tasks, timers, or matches running after
  `shutdown()` returns — the provider-contract test suite asserts it
  (document 07 §4).

## 11. Intended Linux observation sources (contract summary)

| Source | Role | Sections (target) | Event support |
|---|---|---|---|
| rtnetlink (`crates/netlink`) | primary kernel interface — existing [REPO] | interfaces, addresses v4/v6, routes v4/v6, (target: routing rules) | multicast monitor (existing) [REPO] |
| `/sys/class/net`, `/proc/net` (`crates/sysfs`) | fallback — existing [REPO] | interfaces, addresses v6, routes v4, routes v6 (main-table caveats documented [REPO]) | none (poll fallback) |
| NetworkManager D-Bus | optional service provider (OD-02) | active connections, device mapping, Wi-Fi details, VPN evidence (target set is Prompt 05's to finalize within this contract) | StateChanged-family signals (target) |
| systemd-resolved D-Bus | optional service provider (OD-02) | DNS resolver configuration (per-link + global) | none assumed (poll fallback) |
| systemd-networkd | optional service provider (OD-02) | observable state only; concrete sections are a Prompt 05 finding (WO-02 §4) | none assumed |
| other kernel interfaces | only where a specific observation requirement justifies them (work order §3.2); each addition names its requirement and its read-only proof | — | — |

Allow-listed D-Bus operation specifics: document 06 §4. No provider may
change network state, even temporarily, as a discovery technique (work
order §3.3) — structurally impossible through this contract (no mutation
surface) and audited at the implementation level (document 06 §5).

## 12. Contract decision summary

| Decision | Source | Status |
|---|---|---|
| Identity as stable strings, unknown preserved verbatim | E-1; `Other(...)` discipline [REPO] | RD-03 |
| Four-axis availability/permission/support/health model | OD-02; OD-10 permission probes; work order §3.5 | approved-derived |
| `permissionDenied` as a first-class section outcome | OD-10; R-03 | RD-04 |
| Probe (capability-only) separated from observe | work order §3.2; read-only probing | RD-05 |
| ChangeFeed abstraction; engine owns debounce/poll | existing monitor semantics [REPO]; poll approval (WO-02 ch. 8) | RD-06 |
| Backoff values 250 ms→8 s (retry), 60 s (re-probe), 30 s (poll) | existing monitor backoff [REPO]; new defaults | RD-08/09/10 |
| Provider-owned resources; terminal Stopped state | work order §3.2; testability | RD-07 |
| Health metadata in every document | work order §3.2, §3.5 | approved-derived |
