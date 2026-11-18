# WO-02 implementation design

Reassessment and design for network topology, main routing-table
presentation, and provider status on top of the reconstructed WO-01R
foundation. Written after inspecting the actual reconstructed architecture
(the crates, model semantics, merge rules, and test infrastructure from
commits `1a939dc`…`9e462d1`), per WO-02 §7.1.

## 1. What WO-02 adds to the reconstructed foundation

The foundation already provides everything WO-02 consumes: the authoritative
`NetworkSnapshot` (interfaces with kind/master/parent, addresses, routes in
all tables with protocol/scope/type/metric/table, provider reports with
six-state health and per-section outcomes), provenance on every item, the
CLI, and the schema-locked fixture contract. WO-02 adds:

1. `crates/core::topology` — a pure, deterministic projection from a
   `NetworkSnapshot` to a `TopologyDocument` (nodes, edges, evidence).
2. `araiguma topology` — CLI projection of a snapshot file (or stdin) to a
   topology JSON document, pipe-friendly next to `araiguma snapshot`.
3. Frontend views: interactive topology (Cytoscape), main-table route
   presentation, provider status — all consuming the same documents.
4. Fixture expansion and the test matrix for all of the above.

No new discovery mechanism, no parallel model: topology is a **view** of the
observation model, computed where the model lives (Rust).

## 2. Topology design

### 2.1 Nodes

| Node | Created when | Meaning |
|---|---|---|
| Interface node | every observed interface (all kinds, including unrecognized `other`/`virtual` — never dropped) | the interface as observed |
| Gateway node | a route explicitly names a next-hop IP | the next hop as a routing fact; **not** a claim that the host is reachable, alive, or on the internet |

There are deliberately **no other node kinds**: no "internet" node, no
"cloud", no "upstream" node. A default route is evidence of a configured
next hop, not of connectivity; nothing in the observation model can prove
an upstream exists, so none is drawn.

Gateway nodes are keyed by IP address (per family). A gateway referenced by
several routes is one node with several edges.

### 2.2 Edges and evidence classes

Every edge carries an evidence class. Edges exist **only** with evidence:

| Edge | From → To | Evidence |
|---|---|---|
| `bridgeMember` | port interface → bridge interface | **observed** — netlink `IFLA_MASTER` or sysfs `master` symlink (`masterIfindex`) |
| `vlanParent` | vlan interface → parent interface | **observed** — netlink `IFLA_LINK` (`parentIfindex`) |
| `gatewayRoute` | interface → gateway | **observed** — a route names both this oif and this gateway (main table; table id recorded on the edge) |
| `gatewayInferred` | interface → gateway | **inferred** — a route names the gateway but no oif, and the gateway falls inside exactly one interface's subnet (reason recorded) |
| `tunnelUnderlay` | tunnel interface → underlay interface | **observed** — only where `parentIfindex` is present on a tunnel-kind interface (e.g. some kernels expose it); WireGuard underlays are **not** observable unprivileged → no edge, node annotation instead |

`observed` = a single kernel fact states the relationship. `inferred` =
derived from more than one observation, with the derivation recorded on the
edge; rendered differently (dashed) and labeled. Relationships that cannot
be established (VPN peer, upstream beyond a gateway, bridge↔gateway) are
**not edges** — they appear as explicit notes in the selected node's
details, phrased as unknown.

### 2.3 Node state and annotations

- Interface state chips reuse the existing honest semantics (admin-down
  wins; `unknown` operstate stays unknown — loopback/dummy behavior).
- Interfaces with a default route but **no gateway** get an annotation
  ("default route without a gateway — on-link next hop"), not a fake edge.
- Gateway nodes are annotated "reachability not tested — observation only";
  they never get an "up" chip.
- Unresolved route oifs (ifindex/name not in the interface list) produce an
  annotation on the route row, not a phantom node.

### 2.4 Determinism and testing

`build_topology(&NetworkSnapshot) -> TopologyDocument` is pure and
deterministic: same snapshot, same document (stable ordering throughout).
Unit tests assert nodes/edges/evidence per fixture scenario, including
negative assertions (e.g. the workstation fixture produces **no** edge from
`wg0` to any gateway; `dummy0`'s connected route creates no gateway node).

## 3. Main routing-table presentation

- Columns: destination (formatted, `default` highlighted), gateway
  (`on-link` when the kernel reported none — an observation, not a gap),
  output interface (name, resolved via the snapshot), family chip, protocol,
  scope, type, metric, table, flags.
- **Default view = main table (254) only.** Other tables present in the
  snapshot are offered behind an explicit table selector showing counts
  (e.g. `local (255) — 3 routes`); selecting them adds them — nothing is
  ever deleted or merged across tables. The active filter is always
  visible.
- Multiple default routes are all shown, ordered by metric, visually
  emphasized; no "the" default is invented.
- Missing fields render as `—` with a "not observed" tooltip; the
  incomplete-observations fixture proves the degraded path renders
  honestly.
- Long prefixes and long interface names are handled by CSS (truncation
  with full value on hover).
- Linkage: selecting a route highlights its oif and gateway nodes when the
  topology view is opened (highlight only — nothing implies control).

## 4. Provider status

- A dedicated view lists each provider: state chip (six states, consistent
  colors), per-section outcomes with counts or errors, and notes verbatim.
- The section table explains *why* something is missing: failed (with the
  error), unsupported (no source), observed-with-limits (degraded).
- Views whose data depends on a failed provider show a non-blocking banner
  naming the provider and its state; a provider failure never blanks or
  crashes the UI (the frontend renders whatever the snapshot contains, and
  the snapshot preserves every surviving observation).
- The provider list is derived entirely from `snapshot.providers` — no
  invented health facts.

## 5. Frontend/backend consistency (type-sync decision)

**Decision: defer code generation; keep the golden-fixture contract.**
Rationale: the model surface is small (~20 types), already mirrored once,
and locked from both sides by `fixtures/snapshots/*.json` plus (new)
`fixtures/topology/*.json` — Rust tests parse every fixture with the
authoritative serde model; frontend tests parse the same files through the
TypeScript mirror and the validating loaders. Introducing a generator
(ts-rs or similar) would add a build-time dependency and a derive-macro
surface for a duplication the fixtures already police. Precise sync
requirements if this is revisited: every serde rename (camelCase fields,
lowercase enums, `Other(u8)` → `{"other": n}`, internally-tagged section
outcomes) must round-trip identically in both languages; the change
procedure stays the one in `docs/development.md`.

Topology documents follow the same contract: `fixtures/topology/*.json` are
generated by the Rust projection (script) and parsed by both suites.

## 6. Cytoscape.js assessment

- Candidate: `cytoscape` **3.35.0**, MIT license, no peer dependencies,
  ~5.8 MB unpacked (~400 kB bundled into the app build). One new runtime
  dependency; no layout extensions required (built-in `cose` force-directed
  layout is sufficient at our node counts; `fcose`/`dagre` would be extra
  dependencies for no present need).
- Integration: one Svelte component owns an imperative Cytoscape instance
  (`onMount` create / `$effect` update / destroy cleanup); Svelte state
  holds only selection and layout parameters. Elements are styled by data
  (kind/state/evidence classes); selection routes to the shared details
  panel.
- Assessment result: **adopted** as planned.

## 7. Test plan (extends `scripts/test-all.sh`; nothing weakened)

| Area | Tests |
|---|---|
| Topology projection (Rust) | per-fixture assertions incl. negative cases: workstation (multi-default, wg on-link, bridge isolated, no internet node), server-tunnel (missing gateway, tunnel default), dummy-virtual (veth pair, vlan parent, bridge members), empty (no nodes — valid), incomplete (sysfs-only classification caveats surface) |
| Golden documents | `fixtures/topology/*.json` generated by the Rust projection via script; committed; stability checked like the sysfs fixtures |
| CLI | `araiguma topology` round-trip: snapshot fixture in → topology golden out (byte-comparable modulo timestamps) |
| Frontend unit (vitest) | topology document loader + parse validation; route filtering (main-table default, explicit other tables, multiple defaults); provider-state display mapping; missing-field rendering |
| Browser (Playwright) | topology canvas renders N nodes with legend and evidence styling; node selection opens details; routes table renders with main-table filter default and table selector; provider view lists both providers with correct states; mock badge still visible everywhere |

## 8. Scope boundaries (unchanged)

Excluded and not attempted: Tauri/desktop packaging, NetworkManager /
systemd-resolved / systemd-networkd integration, persistence, traffic
accounting, notifications, tray, any network control, privileged anything,
GitHub publication. Sandbox tests make no claim about real-service
compatibility.
