# 04 — Shared JSON document and IPC contract

Prompt 03 deliverable: the one JSON document contract (OD-03) shared by
the CLI, file import, and Tauri IPC — envelope, identity, timing,
validation, atomic replacement, schema policy — and the IPC surface with
its event-continuity semantics and runtime ownership.

Sources: OD-03 (one document contract; Tauri IPC live snapshots/events;
file import retained), OD-01 (Tauri 2 shell), the existing document
model and version policy [REPO — schema v1–v3, `crates/core/src/snapshot.rs`,
`docs/development.md`], the existing watch envelope contract [REPO —
`crates/cli/src/lib.rs`], WO-02 §2.3 (thin-command principle), E-3
(live-document store), work order §3.6–3.7.

## 1. The single contract (binding)

There is exactly one interchange format: the **snapshot document** (and
its pure projection, the **topology document**). The CLI prints it, the
import path validates it, IPC serves it. No layer defines a second,
competing representation of the same facts; no side channels carry
network facts (work order §3.6; OD-03).

- CLI `snapshot` → one snapshot document (JSON).
- CLI `watch` → NDJSON envelopes whose payloads are the same documents
  and their diffs [REPO — existing envelope contract].
- File import → the same documents, validated fully [REPO].
- IPC commands return the same documents; IPC events are the watch
  envelopes (§7).
- Topology is always the backend's pure projection of a snapshot
  [REPO — `build_topology`]; the frontend never re-implements projection.

## 2. Document envelope (schema v4)

Schema v4 extends the current v3 snapshot document additively, following
the repository's established policy (additions serde-defaulted so older
documents parse unchanged — the v2/v3 precedent [REPO]):

```json
{
  "schemaVersion": 4,
  "timestampMs": 1732180000000,
  "meta": {
    "documentId": "0f4c…-uuid-v4",
    "producer": { "name": "araiguma", "version": "1.0.0", "channel": "cli" },
    "synthetic": false,
    "collection": {
      "sessionId": "b18d…-uuid-v4",
      "collectionIndex": 7,
      "startedMs": 1732179999871,
      "completedMs": 1732180000000
    }
  },
  "providers": [ { "provider": "netlink", "state": "available",
      "capabilities": { … document 02 §5 … },
      "health": { … document 02 §8 … },
      "sections": [ … existing per-section outcomes … ],
      "notes": [ … ] } ],
  "interfaces": [ … v3 shape, plus optional per-item "conflicts": [ … ] … ],
  "addresses":  [ … v3 shape … ],
  "routes":     [ … v3 shape … ],
  "routingRules": [ … document 03 §6.2 … ],
  "dns":          [ … document 03 §8 … ]
}
```

Envelope rules:

- `schemaVersion` (u32): present and checked first — existing behavior
  [REPO].
- `timestampMs`: the observation **completion** time (epoch ms, UTC) —
  retained in place for v3 compatibility; the authoritative timing is
  `meta.collection` (`startedMs`/`completedMs`; per-provider completion
  times live in each provider report's health metadata).
- `meta` is **required in v4 documents written by the application** and
  fully defaulted on read so v3 documents parse (a v3 document's meta is
  reconstructed as unknown-origin: absent `documentId` ⇒ the document is
  identified only by its content and timestamp — presented honestly as
  such).
- **Document identity**: `meta.documentId` is a UUID v4 generated per
  document emission. Two emissions of the same observation are distinct
  documents (identity is per-document, not per-content). Topology
  documents carry their own `meta` with
  `producer.channel = "projection"` and `sourceDocumentId` naming the
  projected snapshot.
- **Collection/session identity**: `meta.collection` (sessionId +
  collectionIndex) identifies the observation run that produced the
  document — the correlation key for logs and diagnostics (document 05).
- **Producer channel**: one of `cli`, `desktop`, `projection`,
  `test-fixture`. Fixtures, mocks, demos, and any synthetic observation
  MUST set `synthetic: true` **and** `producer.channel = "test-fixture"`;
  the validators reject the combination `channel: "test-fixture"` with
  `synthetic: false` (an unmistakable label — work order §3.6; existing
  MOCK DATA badge discipline [REPO], now structural).
- **Observation time vs. import time are separate facts.** The document
  carries only observation timing. Import time is a fact about the
  importing session and is recorded **by the importer, in session state,
  next to the document** — never written into the document (importing
  must not mutate the artifact; the existing UI already displays the two
  distinctly [REPO — WO-03 M1, Stage A]). Every presentation of an
  imported document shows its original observation time and its import
  time as distinct labeled facts.
- **Provider/capability metadata**: the capability report (document 02
  §5) and health metadata (§8) travel inside every document, so any
  consumer can always answer "what could/should have been observed and
  what happened" without a second channel (work order §3.6).
- **Errors and incomplete coverage**: represented inside the document —
  per-section outcomes, provider states, conflict entries, unresolved
  markers, fidelity notes. A document is never "an error"; failures are
  facts within it (existing principle [REPO]).

## 3. Serialization and deserialization behavior

- serde JSON, camelCase fields, lowercase enum values, `Other(..)`-style
  passthrough for unmapped values — the established conventions [REPO].
- Ordering is deterministic (items sorted by their stable keys; sections
  in declaration order; keys in declaration order) so equivalent
  documents compare byte-equal modulo identity/timing — the existing
  golden-fixture discipline [REPO].
- Documents are self-contained (no external references, no $ref).
- Watch output is NDJSON: one envelope per line, flushed per envelope
  (existing behavior [REPO]).
- Payload size is not a v1.0.0 concern (workstation-scale snapshots are
  tens of kB; changed envelopes carry diffs — WO-02 §2.3 [ASSESS]);
  binary protocols are explicitly rejected for v1.0.0 (RD-16).

## 4. Schema compatibility and migration policy

The existing policy, extended [REPO — `docs/development.md`]:

1. Writers emit the current version (v4) only.
2. Readers accept the current version and older versions **where
   unambiguous**: v3 and v2 parse under the v4 model unchanged (every
   v4 addition is defaulted); v1 is accepted only when every
   `macAddress` is a non-null string (the v1 ambiguity rule [REPO]).
3. Unknown versions are rejected with the supported range stated
   (existing behavior [REPO]).
4. Version bumps are additive-first: a new version must state, field by
   field, its defaulted/required status and its acceptance rule for the
   previous version, and update fixtures + both validators (Rust and
   TypeScript) in the same change — the existing same-commit procedure
   [REPO — `docs/development.md`].
5. Removals or semantic changes to existing fields require a version
   bump AND an explicit migration note in the schema history; they are
   avoided unless a correctness defect demands one (the v2 MAC change
   is the precedent [REPO]).

## 5. Validation and rejection behavior

- **Full-document validation at every boundary** — the existing frontend
  discipline [REPO — `lib/validate.ts` and loaders; WO-02 correction §8]
  becomes the universal rule: CLI topology input, file import, IPC
  responses, and IPC-request-carried documents are all validated
  completely (structure, types, numeric ranges, family consistency,
  duplicate identifiers, dangling references, provenance non-emptiness
  [REPO — AUD-06], synthetic labeling, envelope/meta shape) before use.
- Rejections carry a **path-qualified reason** (`$.routes[2].gateway:
  …`) — existing behavior [REPO]; nothing is ever partially accepted.
- An invalid document is rejected whole; the previous valid view stands
  (§6). Validation is pure and total: it never panics, never observes,
  never repairs. Where a document violates a MUST of this package
  (e.g. unlabeled synthetic data), the rejection message names the rule.

## 6. Atomic validation and replacement

- **Import**: parse → validate fully → only then replace the current
  imported view; a rejected document leaves the current view untouched
  (existing behavior [REPO — WO-03-F AUD-09], now a binding contract).
- **Live store**: IPC documents enter the live store through the same
  validate-then-swap (E-3): a malformed live document (bug, corruption)
  must not corrupt or blank the current valid view; it is rejected,
  logged as a diagnostics event (document 05 §7), and the previous
  document remains with its observation time (an honest stale view
  beats a fabricated fresh one).
- **Construction**: the engine constructs each document in memory and
  publishes it atomically (single assignment); consumers never see a
  half-built document.
- Imported documents and live documents occupy **separate, explicitly
  labeled states** (imported | live | synthetic); no presentation path
  renders an imported document as live observation (existing status-bar
  discipline [REPO]; binding).

## 7. Watch / IPC event contract

The event envelopes extend the existing CLI watch contract [REPO] with a
sequence number (continuity, §8):

```json
{"event":"snapshot",      "seq":41, "snapshot":{…document…}}
{"event":"changed",       "seq":42, "notifiedBy":["newLink","newRoute"], "changes":[…diff events…]}
{"event":"resynchronized","seq":43, "changes":[…], "snapshot":{…full fresh document…}}
```

- Semantics are unchanged from the CLI contract [REPO]: `snapshot` is
  the initial state; `changed` is a debounced pure diff; `resynchronized`
  means a monitoring gap occurred — changes during the gap were **not
  observed** and the fresh snapshot is authoritative again.
- `seq` is a u64, monotonic per engine session, gap-free in-order
  delivery within a session (envelopes are delivered in seq order).
- `changes` items are the existing typed diff events [REPO —
  `NetworkEvent`], extended additively for v4 entities (rules, DNS)
  under the same identity discipline.

## 8. Event continuity and resynchronization (binding rules)

1. **Gap detection**: a consumer that observes a seq discontinuity (or
   an envelope arriving after a resync it cannot reconcile) must treat
   its state as discontinuous.
2. **Recovery is always snapshot-first**: the consumer requests a fresh
   full document (`getSnapshot`) and continues from it — never silently
   continues on incomplete state (work order §3.7). The `snapshot`
   envelope that opens every new subscription (existing first-envelope
   rule [REPO]) serves the same role on (re)subscribe.
3. **Resync means unknown-in-between**: after `resynchronized`, the
   consumer must not interpret the fresh snapshot as evidence that
   nothing changed during the gap (R-10); freshness restarts at the
   resync envelope.
4. **No lossy compaction**: `changed` envelopes are pure diffs of full
   documents; consumers may always re-derive from a snapshot.

## 9. IPC surface (OD-03; candidate names from WO-02 §2.3, refined)

Four commands (thin-command principle — each maps to one engine call;
no IPC verb invents semantics the engine lacks; a complete subscription
lifecycle requires both a create and a cancel verb — Prompt 03-FC FC-01):

| Command | Request | Response | Semantics |
|---|---|---|---|
| `get_snapshot` | `{}` | `SnapshotDocument` | the engine's current validated document (§6) |
| `get_topology` | `{ "source": "current" }` or `{ "snapshot": SnapshotDocument }` | `TopologyDocument` | pure projection of the current live document, or of a caller-supplied (validated) snapshot — keeping projection authority in the backend |
| `subscribe_watch` | `{}` | `WatchHandle { subscriptionId }` | starts event delivery (§7); the first delivered event is always a `snapshot` envelope |
| `unsubscribe_watch` | `{ "subscriptionId": string }` | `Cancelled { subscriptionId }` | terminates the subscription (lifecycle below); idempotent for subscriptions this session issued |

Subscription lifecycle — the complete contract (every state, transition,
and cleanup path is declared; nothing is implicit):

**Identity.** `subscriptionId` is an unguessable string (UUID v4)
generated by the engine per subscription. It names exactly one
subscription within one engine session; it is never reused and is never
valid across process restarts.

**Creation.** `subscribe_watch` → the engine registers the subscription,
returns `WatchHandle { subscriptionId }`, and delivers the initial
`snapshot` envelope as that subscription's first event. Multiple
subscriptions per frontend are permitted and independent.

**Active delivery.** Document-event envelopes (§7) flow to the
subscription as IPC events (channel `araiguma://watch`, named by
subscriptionId), in seq order until termination.

**Cancellation.** `unsubscribe_watch { subscriptionId }`:

- Returns `Cancelled { subscriptionId }` when accepted — for an **active**
  subscription, or for one **already terminated within this session**
  (idempotent: repeated cancellation succeeds without error and never
  emits a second `ended` envelope).
- A syntactically well-formed `subscriptionId` this session never issued
  is an error: `invalidRequest` with the reason naming the unknown id
  (a wrong id is a client defect or a stale id from a previous process —
  surfaced, not masked).
- Termination takes effect at the next envelope boundary: the terminal
  `ended` envelope is delivered, and no document-event envelope generated
  afterwards is delivered to that subscription.

**`ended` envelope (terminal, exactly once per subscription):**

```json
{"event":"ended","subscriptionId":"…","seq":41}
```

A control envelope, not a document event: `seq` is the **highest
document-event seq delivered on that subscription** (its reconciliation
position; the initial `snapshot` envelope counts), the envelope itself
consumes no seq from the engine counter, and nothing is delivered after
it. The CLI watch contract has no `ended` envelope (CLI termination is
process-level signal handling), so the CLI/IPC envelope-equality tests
(RD-30) compare document-event envelopes only.

**Disconnect cleanup.** App/window close cancels that frontend's
subscriptions implicitly: the transport-level disconnect is detected,
each subscription is terminated server-side, and the subscription state,
delivery task, and channel resources are released (a best-effort `ended`
delivery is attempted; release does not depend on its success). No
subscription and no delivery task survives its frontend — asserted by
tests (document 07 §6).

**Shutdown cleanup.** Engine shutdown terminates every active
subscription (each receives `ended`), then the next command returns
`shutdownInProgress` (document 01 §6.3).

**Ownership.** The engine owns subscription state and delivery tasks; the
desktop shell owns only the transport binding (channel emission per
§10). Every delivery task has the engine as its named owner and
is joined or aborted at shutdown per document 01 §8 rule 4 — no orphaned
subscription task is permitted by this lifecycle.

Reconnection: IPC transports do not survive process restarts; a
reconnecting frontend resubscribes (first envelope = snapshot) and may
compare `meta.documentId`/`collectionIndex` to detect what it missed.
Within one session, seq gaps recover per §8.

Error model (typed, small — observation failures are NOT IPC errors,
they live inside documents):

```
IpcError { code: "invalidRequest" | "shutdownInProgress" | "internalError",
           message: string, detail?: string }
```

`invalidRequest` covers malformed parameters, documents rejected at
the boundary (§5) with the path-qualified reason in `detail`, and
syntactically well-formed subscription ids this session never issued
(§9 lifecycle); `internalError` is never used to smuggle network facts.

## 10. Transport isolation

The core domain and engine know nothing of Tauri, IPC channels, or JSON
wire details beyond their own serde types. Command handlers live in
`apps/desktop` as transport-neutral functions (request type → response
type) bound to Tauri at the edge (document 01 §7.3) — so the entire IPC
logic is testable headless with plain function calls (document 07 §6).
The frontend consumes documents through the same validating loaders as
file import (E-3): one contract, one boundary, three transports.

## 11. Tokio runtime ownership (summary; binding rules in document 01 §8)

The embedding binary owns the runtime (desktop: Tauri's Tokio-based
async runtime per OD-01; CLI: `#[tokio::main]`; tests:
`#[tokio::test]`). Library crates never construct runtimes, never nest
them, never block an async executor (blocking work goes to
`spawn_blocking`), and every spawned task has a named owner responsible
for its cancellation at shutdown. A static audit test enforces the
library-crate rules (document 07 §8).

## 12. Contract decision summary

| Decision | Source | Status |
|---|---|---|
| One document contract for CLI/import/IPC; no side channels | OD-03 | approved (binding) |
| Four-command IPC surface; `unsubscribe_watch` explicit, idempotent, with terminal `ended` envelope and full cleanup semantics | OD-03 (a subscription lifecycle requires explicit cancellation); Prompt 03-FC §4 (FC-01) | approved-derived |
| v4 envelope: additive `meta` (identity/producer/synthetic/collection) | work order §3.6; v2/v3 additive precedent [REPO] | RD-15 |
| Import time recorded by importer in session state, never in the document | work order §3.6 (separate concepts); artifact immutability | RD-17 |
| Envelope `seq` + snapshot-first gap recovery | work order §3.7; R-10 | RD-18 |
| `get_topology` also projects caller-supplied snapshots (backend stays projection authority) | work order §3.7; existing projection authority [REPO] | RD-19 |
| Typed 3-code IPC error model; observation failures live in documents | work order §3.7; existing principle [REPO] | approved-derived |
| Binary transports rejected for v1.0.0 | WO-02 §2.3 sizing | RD-16 |
| Validation at every boundary; atomic replacement everywhere | work order §3.6; AUD-09 [REPO] | approved-derived (binding) |
