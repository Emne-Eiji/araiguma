# WO-08 — Observability redesign: evidence-first topology + provider coverage

Araiguma already has an honest data model: a v4 `NetworkSnapshot`, a
deterministic Rust topology projection, and per-provider `SectionOutcome`
reports. The pre-WO-08 UI obscured it. The goal of this work order is not
new discovery; it is to make **network state, provider health, and
observed-vs-inferred data clear at a glance** without weakening any fidelity
guarantee.

This note records the three materially different directions explored and
why the selected direction was implemented. It is a design record, not a
new data source.

## Constraints (non-negotiable)

- **Preserve the v4 snapshot contract.** No backend change; no new D-Bus
  calls, permissions, probes, polling cadence, or lifecycle changes. No
  backend files were modified in WO-08.
- **Preserve provider ownership.** `systemdResolved` owns `dns`;
  `networkManager` owns `connections`; baseline providers
  (`netlink`/`sysfs`) mark those sections `unsupported`. Source provenance
  (`sources`, provider names, observation notes) stays visible.
- **Preserve outcome distinctions.** `NotObserved`, `Unsupported`,
  `Observed([])`, `Observed(n)`, `Failed`, `Unavailable`,
  `PermissionDenied` are never collapsed into "unavailable" or "none".
  `Observed([])` is a valid observation; an absent v4 collection is "not
  observed"; `unsupported` is not absence.
- **No fabrication.** No invented interfaces, routes, gateways,
  relationships, connection state, DNS resolution, reachability, or
  inferred topology edges. Only `Evidence::Inferred` edges already marked
  inferred by the Rust projection are rendered as inferred. The view adds
  no relations of its own.
- **No new identifiers.** The redesign adds no D-Bus paths, UUIDs, SSIDs,
  hardware addresses, connection names, remote endpoints, process info,
  credentials, or secrets, and introduces no new identifier-bearing field.
  Privacy-contract note (corrected at WO-08 publication): the MAC redaction in
  `docs/design/prompt-03/05-logging-and-diagnostics.md` §9.1 is a
  **diagnostics-export** policy — an allowlisted export whose document summary
  carries counts, never values, and whose free-form log/error strings are
  pattern-redacted by default. It is not a live-UI rule. The live UI sits
  outside that allowlist and, unchanged by WO-08, renders the model's honest
  three-state `MacObservation` only where it already did
  (`formatMacObservation`, in interface details and topology details). WO-08
  adds no MAC rendering site and neither relaxes the contract nor alters the
  model.
- **No new dependency for visuals.** No large UI framework was added. The
  topology is hand-rendered SVG; `cytoscape` is removed because it is no
  longer used.
- **Navigation, keyboard, responsiveness, accessibility preserved.**
  Sections, rail, shared details panel, `Esc` close + focus return, and
  cross-link focus management (WO-08 review I2) are maintained. The new map
  adds keyboard-operable nodes (`tabindex="0"`, `focus-visible` styling);
  WO-08 also adds a `prefers-reduced-motion: reduce` block so every new
  transition collapses to a single frame. `data-testid`s and the
  `__araigumaTopology` inspection hook contract (`topology-canvas`,
  `nodeCount`, `edgeCount`, `nodeLabels`, `selectNode`, gated on
  `?araiguma-test-hooks`, removed on unmount) are preserved.

## Direction A — Table-first coverage console

**Idea.** Replace force-layout topology with dense tables; every fact a
sortable row; topology becomes a table of nodes + a separate edge table.
Provider health is per-provider tables. DNS and connections are new table
sections.

**Strengths.** Maximal fact density; trivially keyboard-navigable;
screenshots and review easy; no layout risk.

**Weaknesses.** Weak at "see the shape of the network at a glance";
relationships are buried in a second table; topology's value (what is
attached to what) is lost; provider health is still per-provider, not
comparable across providers. It under-serves the core ask (network state +
provider health visible at a glance).

**Not selected** as the whole design; its tables survive inside the
selected direction (relation ledger, DNS tables, provider matrix).

## Direction B — Guided-investigation card flow

**Idea.** A step-by-step inspection UI: pick an interface → see addresses →
see routes → see gateways → see DNS → see connections, as a wizard. A
"what's wrong?" triage card guides the user. Provider health is folded
into a checklist.

**Strengths.** Great onboarding; teaches the evidence chain; excellent for
narrow screens.

**Weaknesses.** Slow for experts; hides overview at a glance; sequential
hiding conflicts with "keep humans able to see"; triage order is the
model's opinion, not observed data. Provider health across providers is
not a comparison.

**Not selected** as the whole design; the ordered evidence chain survives
as the shared **observed signal path** inside interface details, and the
overview's **needs-attention** list keeps the triage idea without hiding
data.

## Direction C (selected) — Layered evidence-first observability console

**Idea.** Combine a deterministic **fabric map**, a **state-board
overview**, and a **provider coverage matrix**, all sharing one honest
evidence vocabulary.

- **Overview = state board.** Four at-a-glance tiles (interfaces, default
  routes + gateways, DNS configuration, connections) each carrying an
  evidence marker (OBSERVED / OBSERVED EMPTY / NOT OBSERVED / UNSUPPORTED
  / UNAVAILABLE / FAILED / PERMISSION DENIED); a "needs attention" list
  (down/unknown interfaces, on-link defaults, optional-section gaps,
  failed/unavailable/denied provider sections, missing documents); an
  observation-coverage panel; an evidence key (observed, observed empty,
  inferred, not observed, unsupported); current-document provenance;
  section links. No synthetic numbers; missing docs stay explicit.
- **Topology = deterministic layered fabric map.** Pure-SVG, no force
  layout. Interfaces are placed in dependency layers (a member/child sits
  to the right of the master/parent it attaches to); gateways are in a
  final layer; rows are ordered by `ifindex`/address. Only projected
  nodes/edges are rendered; observed vs inferred evidence is shown in
  line style and in the **relation ledger** (every edge listed with
  kind + evidence + detail). A **service strip** shows DNS/connection
  observations from the current snapshot, clearly separated from the
  topology document, with explicit absence and different-observation-time
  handling. Keyboard-operable nodes; `topology-canvas` +
  `__araigumaTopology` preserved.
- **Interfaces = scannable inspection board.** Cards show state, kind,
  addresses, routes, gateways, link-DNS, connection counts, structural
  relations, and provenance. Shared details panel adds an **observed
  signal path**: addresses → routes (incl. multipath) → gateways → link
  DNS → connections, with absent optional sections labeled; DNS/connection
  cross-links close the panel and navigate to the dedicated views.
- **DNS + Connections = first-class evidence views.** New `dns` and
  `connections` sections (v4 optional collections). Explicit observed /
  observed-empty / not-observed / unsupported / unavailable /
  permission-denied / failed states; provider report shown; no claim of
  name resolution; connection objects are not traffic telemetry; source
  provenance shown; no identifiers beyond what the model carries.
- **Providers = provider × section coverage matrix.** Rows = providers,
  columns = the seven v4 sections (+ any unknown reported kinds); cells
  use full outcome vocabulary with short visible code + accessible label +
  tooltip; provider state; ownership; verbatim notes; outcome-vocabulary
  key; provider-state counts. A provider without a report entry shows
  "not reported", not success; nothing is inferred.

**Shared library.** `apps/frontend/src/lib/observability.ts` centralizes
pure derivations so no two views disagree on counts or outcome semantics:
`optionalSectionAvailability`, `dnsAvailability`, `connectionsAvailability`,
`gatewayObservations`, `routesForInterface`, `gatewaysForInterface`,
`routeGateways`, `isDefaultRoute`, `dnsForInterface`,
`connectionsForInterface`, `outcomeSummary`, `problemSections`,
`providerStateCounts`, `providerDisplayName`, `sectionOwner`,
`SECTION_ORDER`, etc. Unit-tested in `tests/observability.test.ts`.

**Why selected.** It satisfies "network state, provider health, and
observed-vs-inferred data clear at a glance" without adding a dependency,
without changing data fidelity, and by making honesty *more* visible:
unknown stays unknown, unsupported is not absent, observed-empty is not
missing, inferred is dashed and ledgered. Tables are kept inside the
console where density is needed; the map and overview supply the
at-a-glance layer that Direction A lacks; the evidence chain supplies
Direction B's clarity without hiding data. It keeps responsiveness,
keyboard, and accessibility intact.

## Fixture change (honesty, not new capability)

`fixtures/snapshots/workstation.json` is upgraded **synthetic** schema
v3 → v4 (no change to interfaces/addresses/routes): adds 3 DNS
observations (global + `eth0` + `wlp3s0` link scope) and 5
NetworkManager connection objects; baseline providers mark `dns` and
`connections` `unsupported`; `systemdResolved` (DNS owner) and
`networkManager` (connections owner) are added with only their owned
sections `observed` and everything else `unsupported`. The v4 network
portion is unchanged; the Rust-projected
`fixtures/topology/workstation.json` golden remains byte-identical
(9 nodes / 5 edges). Still synthetic; not an observation.

## Implementation map

- `apps/frontend/src/lib/observability.ts` (new)
- `apps/frontend/src/components/OverviewView.svelte` (state board)
- `apps/frontend/src/components/TopologyView.svelte` (deterministic SVG fabric map + ledger + service strip; no Cytoscape)
- `apps/frontend/src/components/InterfacesView.svelte`, `InterfaceCard.svelte`
- `apps/frontend/src/components/DetailsPanel.svelte` (observed signal path + DNS/connection cross-links)
- `apps/frontend/src/components/DnsView.svelte`, `ConnectionsView.svelte` (new)
- `apps/frontend/src/components/ProvidersView.svelte` (coverage matrix)
- `apps/frontend/src/lib/navigation.svelte.ts`, `NavRail.svelte`, `App.svelte` (8 sections, routing, `onopenSection`, cross-link focus management)
- `apps/frontend/src/app.css` (contrast token + `prefers-reduced-motion` support)
- `fixtures/snapshots/workstation.json` (schema v4 upgrade)
- `apps/frontend/package.json` + lock (remove unused `cytoscape`)
- tests: `tests/observability.test.ts`, updated `navigation`, `snapshot`, `scenarios`, `import`, `e2e/shell`, `e2e/features`, `e2e/smoke`

## Independent-review response

The selected design was implemented, then reviewed independently. The reviewer
found two critical issues — an overview that claimed OBSERVED when no snapshot
existed, and duplicate `{#each}` keys that could crash production on documents
the merge contract permits — plus three important issues (an overstated
reduced-motion/keyboard claim, focus lost after cross-links, and a provenance
sentence that asserted a provider report when none existed). All critical and
important findings were fixed, and the cheap honesty/accessibility minors were
fixed; the remainder are documented in `wo-08-final-report.md` §10. See also
`wo-08-final-report.md` for the reviewer-availability limitation (the designated
GLM 5.3 reviewer was billing-blocked; an alternate independent model reviewed).

## Limitations

- No live data verification beyond the existing engine path; mock/imported
  documents are labeled. Screenshots are from the synthetic workstation
  scenario unless noted.
- Provider states/notes come from providers; Araiguma does not probe
  reachability, resolve names, or open connections to verify them.
- `ConnectionState` `{other}` and unknown enums are preserved, not mapped
  to a known state. Unknown optional collections remain "not observed".
- The map is deterministic for a given document but not a network
  simulator; edge `detail` strings are projected verbatim.

## Publication correction (WO-08)

While reconciling WO-08 for publication, the "no prohibited identifiers"
constraint above originally read "… MACs (beyond what the model already renders
honestly, see `formatMacObservation`) …", which mischaracterized the privacy
contract. The binding MAC policy lives in §9.1 of
`docs/design/prompt-03/05-logging-and-diagnostics.md` and governs the
diagnostics **export** (a field allowlist plus pattern redaction of free-form
log/error strings), not live-UI rendering. The wording was corrected in place
to describe that distinction accurately. The implementation was not changed:
WO-08 adds no MAC rendering site, and the live UI already rendered the model's
three-state `MacObservation` honestly before WO-08 (`formatMacObservation`, in
interface details and topology details). No data prohibited by the contract is
exposed, and no prohibition was silently relaxed.
