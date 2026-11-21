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
- Documented exceptions where a missing value is itself an observation:
  - `Route::gateway == None` on an observed **single-path** route means the
    kernel reported **no gateway** (an on-link route).
  - `Route::next_hops == Multi(hops)` (WO-03 M3): the route-level
    `gateway`/`output_interface` are `None` **by design** — every hop
    carries its own; multiple hops are never flattened into one gateway.
    `Undecoded(reason)` keeps a kernel multipath attribute that could not
    be decoded as an explicit marker instead of dropping it.
  - `Interface::mac_address` is the three-state `MacObservation` (schema
    v2): `Present(addr)` (serialized as the plain address string),
    `Absent` (observed: the interface has **no** hardware address, e.g.
    wireguard), or `NotObserved`. A v1 `null` was ambiguous between the
    latter two states and is rejected on read rather than silently
    assigned a meaning.
- Every item carries `sources`: the providers that observed it, in priority
  order (primary first).
- Unmapped raw kernel values are preserved verbatim (`RouteProtocol::Other(
  u8)`, `RouteScope::Other(u8)`, `RouteType::Other(u8)`) — never collapsed.

### Document schema versions

| Version | Introduced | Change |
|---|---|---|
| 1 | WO-01R | original snapshot/topology documents |
| 2 | WO-02 correction | three-state `MacObservation` everywhere a MAC appears; topology edge kinds `bondMember` and `masterMember` (a master relationship's flavor comes from the master's own kind observation, never an assumption) |
| 3 | WO-03 | veth `peerIfindex` on interfaces + `vethPeer` topology edge (M2); route `nextHops` — `'single'` / `{ multi: [...] }` / `{ undecoded: reason }` (M3). All additions are serde-defaulted, so v2 documents parse unchanged |

Writers emit the current version; readers accept v1 only where it is
unambiguous (a v1 document whose every `macAddress` is a non-null string).
Unknown versions are rejected with the supported range stated.

## Provider abstraction

A provider is a read-only source of network state implementing
`DiscoveryProvider` (async). A run produces a `ProviderObservation` with a
per-section result (`Observed(items)` / `Failed(error)` / `Unsupported` /
`NotObserved`) for interfaces, IPv4/IPv6 addresses, and IPv4/IPv6 routes.

Provider states (six, derived from section results — `NotObserved` is never
conflated with `Unsupported`):

| State | Meaning |
|---|---|
| `available` | every supported section observed, no fidelity limits |
| `degraded` | observed, but with known fidelity limits (sysfs classification ambiguity) |
| `partial` | some sections observed, and at least one supported section **failed or was never attempted** — the run is incomplete either way |
| `unavailable` | no section observed and at least one failure |
| `notSupported` | no supported sections in this environment at all |
| `notYetObserved` | nothing attempted-and-supported yet (an unattempted section is not evidence of missing functionality) |

Aggregation rules, in order: nothing attempted-and-supported → `NotYetObserved`
when some section is unattempted, else `NotSupported`; failures with no
observations → `Unavailable`; observations alongside any failed or unattempted
supported section → `Partial`; a complete run → `Degraded`/`Available`.
Unsupported-in-principle sections never lower a state.

A failing provider never discards other providers' observations: the merge
keeps whatever the remaining sources observed and preserves every failure
report in `snapshot.providers`.

Future providers (NetworkManager D-Bus, systemd-resolved D-Bus,
systemd-networkd) implement the same trait at their own priority; nothing in
the pipeline assumes a specific provider set.

The sysfs provider reads `/sys/class/net` (interfaces), `/proc/net/if_inet6`
(IPv6 addresses), `/proc/net/route` (main-table IPv4 routes), and — since
WO-03 M4 — `/proc/net/ipv6_route` (IPv6 routes). That file spans multiple
routing tables without a table column, so its routes carry `table: None`
with a note; a missing file is a `Failed` section, never an observed "no
IPv6 routes" (absence of the file is not evidence about the route table).

## Merge rules

1. Providers ordered by priority (netlink 0, sysfs 1).
2. Field-wise: the highest-priority provider's `Some` values win;
   lower-priority providers only fill fields that are still `None`. For the
   v2 MAC observation the analogous rule is: a higher-priority `Present` or
   `Absent` stands; `NotObserved` is the only gap a lower-priority provider
   may fill.
3. Cross-source duplicates (same kernel fact seen by netlink and procfs)
   deduplicate into one item with a united `sources` list. Route identity
   (redesigned by WO-03-F / AUD-02 — the previous "next-hop-set overlap"
   rule collapsed distinct ECMP routes the kernel holds under one
   superficial key, verified live with `ip route append`) is two-level
   and evidence-based:
   - Two netlink observations are the same kernel route only when every
     observed discriminator agrees — family, destination, table, metric,
     protocol, scope, type, TOS (`rtm_tos`), flags — AND the complete
     next-hop representations are equal (gateway, via, output interface,
     weight, hop flags). Overlapping hop sets prove nothing. Identical
     `Undecoded` markers deduplicate; a partial single-path view never
     writes route-level gateway/oif onto a multipath observation.
   - A procfs row reconciles with a netlink route only when that route is
     the UNIQUE unused match on family + destination + metric (+ the
     row's table requirement: v4 rows imply main, v6 rows expose no
     table) with the row's first hop contained in the route's hop set —
     `/proc/net/route` shows only the first hop of a multipath route
     (verified live). Multiple candidate matches are never resolved by
     iteration order: the row stays an independent observation with an
     explicit ambiguity note (AUD-05). Identical duplicate table-less
     rows (e.g. the kernel's twice-printed IPv6 unreachable-default
     sentinel) are never silently deduplicated — the file cannot prove
     they are one route, so the multiplicity is preserved and annotated
     (AUD-03).
4. Output-interface enrichment: merged routes carry both ifindex and name
   when either is resolvable — including every multipath hop.

### Merged route order

Routes are ordered for stable display: address family groups (IPv4 before
IPv6), then routing-table groups (ascending table id; a not-observed table
groups with main, matching the merge identity); within a family+table,
observed destinations by ascending prefix length — **default routes first**,
then progressively more-specific prefixes — with destination-not-observed
routes after every observed destination; ties break deterministically by
metric ascending (a not-observed metric sorts last — no preference is
invented) and then gateway (on-link before a named gateway). Multiple
default routes always remain visible.

## Monitoring (crates/netlink)

`Monitor` subscribes to the link / IPv4+IPv6 address / IPv4+IPv6 route
multicast groups (equivalent to `ip monitor`) via a dedicated netlink
connection. Relevant notifications are classified (`newLink`, `newAddress`,
`newRoute`, `delLink`, ...); ACKs and unrelated messages are filtered.

If the connection dies, the monitor reconnects with exponential backoff
(250 ms doubling to 8 s) and emits `Signal::Resynchronized`, which tells
consumers that changes during the gap were **not observed** and state must be
re-dumped. The same signal is emitted when the kernel reports a
receive-buffer overrun (`ENOBUFS` → `NLMSG_OVERRUN`): dropped notifications
are unobserved, and the stream never claims to be current. The watch CLI
turns that into a `resynchronized` envelope with a full fresh snapshot;
ordinary bursts are debounced (150 ms) and emitted as `changed` envelopes
carrying pure snapshot diffs.

## Topology projection (crates/core::topology, WO-02)

`build_topology(&NetworkSnapshot) -> TopologyDocument` is a pure,
deterministic projection. Nodes exist for every observed interface (all
kinds preserved) and for gateways that a route explicitly names; there are
deliberately no internet/upstream nodes. Edges require evidence:

| Edge | Evidence |
|---|---|
| `bridgeMember` | kernel master statement **and the master's own kind observed as a bridge** |
| `bondMember` | kernel master statement **and the master's own kind observed as a bond** |
| `masterMember` | kernel master statement where the master's type could not be established — presented neutrally, never assumed to be a bridge |
| `linkParent` | kernel parent statement (IFLA_LINK: vlan parent, tunnel underlay) |
| `gatewayRoute` | a main-table route names both oif and gateway (destinations aggregated); a multipath route (WO-03 M3) contributes EVERY hop as the same evidence class — one edge per (interface, gateway) pair, with the kernel weight in the edge detail |
| `gatewayInferred` | gateway without oif falls inside exactly one interface's subnet (reason recorded) |

A master or parent ifindex that names a missing interface produces an
annotation on the member node, never a dangling edge; self-referential
master/parent indexes are invalid references and are never drawn.

Observed vs inferred evidence classes are carried on every edge and rendered
differently (solid vs dashed). Unknown relationships (VPN peers, upstream
beyond a gateway, ambiguous inference) surface as node annotations — never
as fabricated edges. `araiguma topology` projects a snapshot document
(file or stdin) into this format; `fixtures/topology/*.json` are committed
goldens generated by that projection and parsed by both the Rust and the
frontend test suites.

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

Document loading goes through fully-validated boundaries (`lib/snapshot.ts`,
`lib/topology.ts` on top of `lib/validate.ts`): every nested structure —
interfaces, addresses, routes, provider outcomes, node ids, edges, evidence —
is checked, including numeric ranges, family consistency, duplicate node
identifiers, and dangling edge references. Invalid documents are rejected
with a path-qualified reason and never partially rendered.

Route rows are keyboard-operable (Enter/Space select, visible focus,
aria labels); selecting a route opens a details overlay exposing flags,
observation notes (including representation limitations), and provenance.
A small production test hook exists on the topology view only when the page
is opened with the explicit `?araiguma-test-hooks` parameter (or in dev
mode), and is removed when the view unmounts.

## Future integration points (not built in WO-01R)

- Tauri 2 desktop shell (blocked in this sandbox: no WebKitGTK, no root).
- NetworkManager / systemd-resolved D-Bus providers (trait slots exist; the
  sandbox has no system bus — a private-session harness capability check is
  part of the gate).
- Loading real observations in the frontend (same `NetworkSnapshot` JSON the
  CLI emits).

## Design trajectory (Prompt 03 specification — not yet implemented)

The backend architecture and network data model that will govern the
implementation stages are specified in
[`docs/design/prompt-03/`](design/prompt-03/README.md) (approved decisions
OD-01/02/03/08/09/10 of the accepted WO-02 feasibility assessment,
translated into crate boundaries, a provider contract, evidence semantics, a
schema-v4 document/IPC contract, logging/diagnostics design, read-only
enforcement, and the test strategy). This document continues to describe the
**current** implementation; where the two differ, the spec package is the
target and nothing in it has been built yet.
