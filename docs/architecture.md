# Araiguma — architecture

Araiguma is a Linux desktop network observability application. Its single
governing principle is **observe, do not control**: everything below is
read-only, and every value the system reports is either an observation or an
explicit "not observed" — never a guess.

## Workspace layout

```
araiguma/
├── crates/
│   ├── core/      domain model, provider abstraction, merge, snapshot diff
│   ├── sysfs/     sysfs/procfs fallback provider (injectable roots)
│   ├── netlink/   rtnetlink primary provider + multicast change monitor
│   └── cli/       `araiguma snapshot` / `araiguma watch`
├── apps/
│   └── frontend/  Svelte 5 + TypeScript + Vite UI
├── fixtures/      synthetic, clearly-labeled test data (schema-locked)
├── scripts/       environment, bootstrap, validation, recovery tooling
└── docs/          this documentation set
```

## Observation pipeline

```
        ┌──────────────────┐   ┌──────────────────┐
        │  rtnetlink dumps │   │  sysfs/procfs    │
        │  (primary,       │   │  (fallback,      │
        │   priority 0)    │   │   priority 1)    │
        └────────┬─────────┘   └────────┬─────────┘
                 │ ProviderObservation  │ ProviderObservation
                 │ (per-section results)│
                 └──────────┬───────────┘
                            ▼
                      merge (core)          netlink values win;
                            │               sysfs fills None fields;
                            ▼               duplicates merge, sources unite
                    NetworkSnapshot         provider reports preserved
                            │
        ┌───────────────────┼─────────────────────┐
        ▼                   ▼                     ▼
  `araiguma snapshot`  `araiguma watch`     frontend (today:
  (one JSON document)  (NDJSON: snapshot,   fixtures/mock; later:
                        changed, resync)    loads CLI documents)
```

The Rust model is authoritative. The frontend never discovers or reconstructs
network state; it renders `NetworkSnapshot` documents (today: labeled
synthetic fixtures; later: CLI output).

## Domain model semantics (crates/core)

- `Option<T>` on a field means **not observed by any provider**. It is never
  interpreted as a negative fact.
- The single documented exception: `Route::gateway == None` on an observed
  route means the kernel reported **no gateway** (an on-link route) — the
  kernel's own encoding of an observed absence.
- Every item carries `sources`: the providers that observed it, in priority
  order (primary first).
- Unmapped raw kernel values are preserved verbatim (`RouteProtocol::Other(
  u8)`, `RouteScope::Other(u8)`, `RouteType::Other(u8)`) — never collapsed.

## Provider abstraction

A provider is a read-only source of network state implementing
`DiscoveryProvider` (async). A run produces a `ProviderObservation` with a
per-section result (`Observed(items)` / `Failed(error)` / `Unsupported` /
`NotObserved`) for interfaces, IPv4/IPv6 addresses, and IPv4/IPv6 routes.

Provider states (six, derived from section results):

| State | Meaning |
|---|---|
| `available` | every supported section observed, no fidelity limits |
| `degraded` | observed, but with known fidelity limits (sysfs classification ambiguity) |
| `partial` | some supported sections observed, some failed at runtime |
| `unavailable` | every supported section failed this run |
| `notSupported` | the provider has no supported sections in this environment |
| `notYetObserved` | has not run yet |

A failing provider never discards other providers' observations: the merge
keeps whatever the remaining sources observed and preserves every failure
report in `snapshot.providers`.

Future providers (NetworkManager D-Bus, systemd-resolved D-Bus,
systemd-networkd) implement the same trait at their own priority; nothing in
the pipeline assumes a specific provider set.

## Merge rules

1. Providers ordered by priority (netlink 0, sysfs 1).
2. Field-wise: the highest-priority provider's `Some` values win;
   lower-priority providers only fill fields that are still `None`.
3. Cross-source duplicates (same kernel fact seen by netlink and procfs)
   deduplicate into one item with a united `sources` list. Route identity
   normalizes the output interface (ifindex ↔ name) through the merged
   interface list and treats procfs's implied main table (254) as equal to
   netlink's explicit 254.
4. Output-interface enrichment: merged routes carry both ifindex and name
   when either is resolvable.

## Monitoring (crates/netlink)

`Monitor` subscribes to the link / IPv4+IPv6 address / IPv4+IPv6 route
multicast groups (equivalent to `ip monitor`) via a dedicated netlink
connection. Relevant notifications are classified (`newLink`, `newAddress`,
`newRoute`, `delLink`, ...); ACKs and unrelated messages are filtered.

If the connection dies, the monitor reconnects with exponential backoff
(250 ms doubling to 8 s) and emits `Signal::Resynchronized`, which tells
consumers that changes during the gap were **not observed** and state must be
re-dumped. The watch CLI turns that into a `resynchronized` envelope with a
full fresh snapshot; ordinary bursts are debounced (150 ms) and emitted as
`changed` envelopes carrying pure snapshot diffs.

## Testing architecture

| Layer | What | Where |
|---|---|---|
| Unit | model serde, merge precedence, dedup, provider states, diff, kernel-format parsing, netlink message translation (constructed messages, no sockets) | `crates/*/tests`, `crates/netlink/src/translate.rs` |
| Fixtures | synthetic sysfs/procfs trees + snapshot JSON documents, schema-locked from Rust **and** TypeScript | `fixtures/` |
| Integration | real kernel behavior inside transient unprivileged network namespaces (`unshare -Urn` self-re-exec): dumps, classification, local-table presence, monitor lifecycle, resync, merge precedence | `crates/netlink/tests/netns.rs` |
| Live cross-validation | the real binary against iproute2 JSON output in a netns, plus a watch smoke | `scripts/live-check.sh` |
| Frontend unit | formatters, state semantics, fixture contract, parse validation | `apps/frontend/tests` (vitest) |
| Browser | smoke, design verification (dark theme, 56px rail, ≥96% workspace, badge), panel-overlays-workspace | `apps/frontend/e2e` (Playwright, production build) |
| Gate | all of the above + fmt/clippy + budget + identity audit | `scripts/test-all.sh` |

Network namespace tests create and destroy interfaces **only** inside
`unshare -Urn` child processes; the host namespace is never modified. `ip`
is used for test setup and independent verification only — the application
itself never shells out.

## Frontend

Svelte 5 (runes) + TypeScript + Vite. Dark charcoal design system, 56 px
icon-led navigation rail, central workspace at ≥96% of viewport width, and a
contextual details panel that overlays (never shrinks) the workspace. While
synthetic data is displayed, a persistent `MOCK DATA` badge is visible.

The TypeScript model in `apps/frontend/src/types.ts` mirrors the Rust serde
output. The sync contract: `fixtures/snapshots/*.json` are parsed by Rust
tests (authoritative model) and by the frontend tests (mirror) — any drift
fails the suite. See `docs/development.md` for the change procedure.

## Future integration points (not built in WO-01R)

- Tauri 2 desktop shell (blocked in this sandbox: no WebKitGTK, no root).
- NetworkManager / systemd-resolved D-Bus providers (trait slots exist; the
  sandbox has no system bus — a private-session harness capability check is
  part of the gate).
- Loading real observations in the frontend (same `NetworkSnapshot` JSON the
  CLI emits).
