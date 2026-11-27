# WO-08 final report — Topology & interface observability redesign

**Outcome: COMPLETE** — implemented and validated locally, with one documented
review deviation: the designated reviewer (GLM 5.3) was unavailable on this
account because of provider billing, so an independent review was performed by
an alternate model and every critical/important finding was addressed. No
commit, push, PR, merge, branch creation, or remote-ref change was performed.

---

## 1. Outcome

**COMPLETE.** The redesign is implemented (not proposed), the v4 contract and
every fidelity guarantee are preserved, and all checks pass locally. WO-08 is
left uncommitted in the working tree for Emne's acceptance. WO-06F remains
SHELVED and was not executed or reopened.

The only deviation from the letter of the work order is the identity of the
independent reviewer: GLM 5.3 could not be billed (see §10). An alternate
independent model performed the review instead.

## 2. Baseline

- Repository: `/home/banana/Constellation/Idea Dev/opencode/Araiguma/araiguma`
- Branch: `recovery/araiguma-main`
- HEAD: `2cdccc5a80355fa908bf0c473fd9bbe131ca4c67` (`fix(engine): validate initial provider snapshots`)
- Initial working tree: clean at WO-08 start; all WO-08 changes are uncommitted.
- No backend/Rust source file is modified.

## 3. Discovery findings

- The pre-WO-08 topology was a Cytoscape force layout: generic circles and
  lines with weak semantic differentiation; relationships were not explained.
- Navigation had six sections; DNS and connection data existed in the v4
  contract but had no dedicated views.
- The workstation fixture was schema v3 and carried no `dns`/`connections`
  collections, so the optional v4 evidence could not be demonstrated honestly.
- Provider health was shown as per-provider tables, not comparable across
  providers, and per-section outcome distinctions were hard to scan.
- Interface inspection was a flat grid; there was no ordered evidence chain
  from address → route → gateway → DNS → connection.
- Accessibility, keyboard navigation, `Esc` + focus return, and test hooks
  (`topology-canvas`, `__araigumaTopology`, `iface-card-*`, `provider-*`) were
  the contracts to preserve.

## 4. Three design directions considered

1. **Direction A — table-first coverage console.** Dense sortable tables for
   every fact; topology as node/edge tables; provider tables. Maximal density
   and trivially keyboard-navigable, but weak at "see the shape at a glance"
   and provider health is not comparable across providers. Not selected whole;
   its tables survive inside the selected design (relation ledger, DNS tables,
   provider matrix).
2. **Direction B — guided-investigation card flow.** A wizard: interface →
   addresses → routes → gateways → DNS → connections, with a triage card.
   Great onboarding and narrow-screen ergonomics, but slow for experts, hides
   the overview, and imposes a model opinion of investigation order. Not
   selected whole; the ordered evidence chain survives as the observed signal
   path and the overview's needs-attention list.
3. **Direction C (selected) — layered evidence-first observability console.**
   A deterministic fabric map + a state-board overview + a provider × section
   coverage matrix, sharing one evidence vocabulary.

## 5. Selected direction and rationale

Direction C best satisfies "network state, provider health, and
observed-vs-inferred data clear at a glance" while *increasing* honesty:
unknown stays unknown, `unsupported` is not absence, `Observed([])` is a valid
observation, and inferred edges are dashed and ledgered. It needs no new
dependency (the map is hand-rendered SVG; unused `cytoscape` was removed), and
it preserves navigation, keyboard behavior, and accessibility. Tables are kept
where density matters; the map and overview supply the at-a-glance layer that
Direction A lacks; the evidence chain supplies Direction B's clarity without
hiding data.

## 6. Information architecture and interaction design

- **Overview — state board.** Four evidence tiles (interfaces, default routes,
  DNS configuration, connections), each carrying an evidence marker (OBSERVED /
  OBSERVED EMPTY / NOT OBSERVED / UNSUPPORTED / UNAVAILABLE / FAILED /
  PERMISSION DENIED). When no snapshot exists, every snapshot-driven tile reads
  NOT OBSERVED with "no snapshot" — never `OBSERVED(0)`. Needs-attention list;
  observation-coverage panel; evidence key; current-document provenance;
  section links.
- **Topology — deterministic layered fabric map.** No force layout. Interfaces
  are placed in dependency layers (member/child right of the master/parent),
  gateways in a final layer; only projected nodes/edges are drawn. Observed vs
  inferred evidence is encoded in line style and repeated in a relation ledger
  (kind + evidence + detail + sources). A service strip shows DNS/connection
  observations from the current snapshot, separated from the topology document,
  with explicit absence and a different-observation-time warning.
- **Interfaces + details — scannable board and evidence chain.** Cards show
  state, kind, addresses, route/gateway/link-DNS/connection counts, structural
  relations, and sources. The shared details panel adds an **observed signal
  path**: addresses → routes (incl. multipath) → gateways → link DNS →
  connections, with absent optional sections labeled; DNS/connection cross-links
  close the panel, navigate, and move focus to the new workspace landmark.
- **DNS + Connections — first-class evidence views.** Explicit observed /
  observed-empty / not-observed / unsupported / unavailable / permission-denied
  / failed states; provider report; no name-resolution or traffic-flow claim;
  provenance shown; no prohibited identifiers.
- **Providers — provider × section coverage matrix.** Rows = providers, columns
  = the seven v4 sections (+ unknown reported kinds); cells use the full outcome
  vocabulary (short visible code + accessible label + tooltip); provider state,
  ownership ("owned by systemd-resolved"), verbatim notes, vocabulary key, and
  provider-state counts. A provider with no section entry shows "not reported",
  never success.

## 7. Implementation summary and file list

Tracked files changed (21):

- `apps/frontend/src/App.svelte` — routing for 8 sections, shared panel,
  `onopenSection` cross-link + workspace focus management (I2).
- `apps/frontend/src/app.css` — `--text-muted` → `#8b95a5` (WCAG AA);
  `prefers-reduced-motion: reduce` block (I1).
- `apps/frontend/src/components/OverviewView.svelte` — state board; honest
  no-snapshot tiles (C1).
- `apps/frontend/src/components/TopologyView.svelte` — deterministic SVG fabric
  map, relation ledger, service strip, keyboard nodes; inferred-edge contrast.
- `apps/frontend/src/components/InterfacesView.svelte` — section header, state
  summary, honest DNS/connections chips (M2).
- `apps/frontend/src/components/InterfaceCard.svelte` — at-a-glance card.
- `apps/frontend/src/components/DetailsPanel.svelte` — identity + observed
  signal path + cross-links.
- `apps/frontend/src/components/ProvidersView.svelte` — coverage matrix;
  ownership display names (M3).
- `apps/frontend/src/lib/navigation.svelte.ts` — eight sections (`dns`,
  `connections` added).
- `apps/frontend/src/components/NavRail.svelte` — eight-section rail; DNS and
  Connections icons and section comment updated (added at publication to
  reconcile the inventory with Git status; it was omitted from the original
  §7 list).
- `apps/frontend/src/lib/topology.ts` — stale "cytoscape ids" comment (M6).
- `apps/frontend/package.json`, `apps/frontend/package-lock.json` — remove
  unused `cytoscape`; no new dependency.
- `fixtures/snapshots/workstation.json` — synthetic schema v3 → v4 (3 DNS
  observations, 5 connection objects; baseline providers mark `dns`/
  `connections` unsupported; `systemdResolved` owns DNS, `networkManager` owns
  connections). Network portion unchanged; topology golden byte-identical.
- Tests: `tests/import.test.ts`, `tests/navigation.test.ts`,
  `tests/scenarios.test.ts`, `tests/snapshot.test.ts`, `e2e/features.spec.ts`,
  `e2e/shell.spec.ts`, `e2e/smoke.spec.ts`.

New (untracked) files:

- `apps/frontend/src/lib/observability.ts` — shared evidence derivations.
- `apps/frontend/src/components/DnsView.svelte`,
  `apps/frontend/src/components/ConnectionsView.svelte`.
- `apps/frontend/tests/observability.test.ts`.
- `docs/design/wo-08-observability-redesign.md` — design record.
- `docs/design/wo-08-screenshots/` — seven genuine captures; see §8.
- `docs/design/wo-08-final-report.md` — this report.

## 8. Screenshots from the running application

Genuine captures from the production build served at
`http://127.0.0.1:5198/?araiguma-test-hooks`; synthetic scenarios clearly
labeled MOCK DATA. All seven were visually inspected in this session.

1. `01-overview.png` — state board, full content (tiles, needs-attention,
   coverage, evidence key, documents, sections).
2. `02-interface-inspection.png` — interface board + details panel with the
   five-step observed signal path and cross-links.
3. `03-dns-inspection.png` — global + per-link scopes, servers/ports, empty
   domains marked "none present in this observation".
4. `04-connections-inspection.png` — connection objects, states, devices,
   VPN chip, no prohibited identifiers.
5. `05-provider-health-unavailable.png` — `incomplete-observations`: netlink
   UNAVAILABLE with `failed` cells, sysfs DEGRADED with counts and `n/a`,
   "not reported" for optional sections.
6. `06-narrow-window.png` — 480 px layout, all sections stacked, no overflow.
7. `07-topology-fabric-map.png` — deterministic layered map, legend, service
   observations, and the 5-row evidenced-relations ledger.

The workspace scrolls internally; captures size the viewport to the workspace
content height so tall views are not cropped (addressing the review's M8).

## 9. Test, check, and build results

| Check | Result |
|---|---|
| `scripts/frontend.sh check` (svelte-check) | **0 errors / 0 warnings** |
| `scripts/frontend.sh test` (vitest) | **14 files; 217 passed + 1 skipped (218)** |
| `scripts/frontend.sh build` | **success; 172 modules; 239 kB JS + 60 kB CSS** |
| `scripts/frontend.sh e2e` (Playwright, production build) | **26/26 passed** |
| `cargo test -p araiguma-core` | **all suites pass** (no backend source touched) |
| Topology stability | `araiguma topology --input fixtures/snapshots/workstation.json --pretty` is **byte-identical** to `fixtures/topology/workstation.json` (9 nodes / 5 edges) |
| Lighthouse (production, overview) | **accessibility 1.0**, best-practices 1.0; remaining items are SEO-only (`meta-description`, `robots.txt`) |
| Contrast audit | all theme text tokens ≥ 4.5:1 on all backgrounds; state colors used as text pass; inferred edge stroke now ≥ 3:1 |
| `git diff --check` | **clean** |
| package-lock | helper-script churn restored; only the intentional `cytoscape` removal remains |

## 10. Independent review and disposition

**Designated reviewer — GLM 5.3 — could not run.** Two attempts failed on
provider billing:
- `opencode/glm-5.3` → "Insufficient balance … OpenCode Models credits."
- `tokenharbor/glm-5.3` → "No active plan was found on this account … wallet
  balance can't cover this model."

No top-up is possible from the implementation session, so this is recorded as
a limitation, not a success.

**Fallback independent review** was performed by `tokenharbor/mimo-v2.6-flash:free`
(a different model family). It could view the screenshots, independently re-ran
the full suite and re-derived the topology golden, and returned findings.

### Critical findings (fixed)

- **C1 — Overview fabricated OBSERVED/OBSERVED EMPTY with no snapshot.** The
  interfaces and default-routes tiles hardcoded an observed badge while live
  mode had no pair yet or imported mode had only a topology. **Fixed**: both
  tiles are gated on `snapshot !== null` and show NOT OBSERVED / "no snapshot".
  Added e2e coverage (`overview stays honest with only a topology document`).
- **C2 — Duplicate `{#each}` keys could crash production.** The Rust merge
  deliberately preserves duplicate connections/DNS observations, and the new
  details panel keyed connections by `type+state+vpn`, throwing
  `each_key_duplicate` (white screen) on contract-permitted documents. **Fixed**
  across all new/extended lists: DetailsPanel (connections, routes via
  `routeKey`, addresses, gateways, notes), InterfaceCard, DnsView,
  ConnectionsView, ProvidersView, OverviewView — index-suffixed keys.

### Important findings (fixed)

- **I1 — "reduced motion / roving keyboard operability kept" overstated.**
  **Fixed**: added a `prefers-reduced-motion: reduce` block to `app.css`; the
  design doc now states accurately that keyboard-operable map nodes were added
  and reduced-motion support is provided.
- **I2 — Focus lost after DNS/Connections cross-links.** **Fixed**: the
  workspace `<main>` is a focus landmark (`tabindex="-1"`) and cross-link
  navigation moves focus to it after the section switch.
- **I3 — Provenance sentence claimed a provider report even when none
  existed.** **Fixed**: `optionalSectionAvailability` now distinguishes
  "Supplied by a provider report" from "Carried by this document; no provider
  report names this section."

### Minor findings (fixed)

- **M1** state-specific empty-state test ids (`dns-{state}`,
  `connections-{state}`) instead of collapsing every failure to `*-unavailable`.
- **M2** InterfacesView DNS chip no longer counts global observations under a
  "Link DNS" label; distinguishes observed-empty.
- **M3** ProvidersView ownership note uses display names ("systemd-resolved").
- **M5** inferred-edge stroke lightened to `#4f7ba6` (≥ 3:1 on the map).
- **M6** stale "cytoscape ids" comment corrected.
- **M7** e2e now asserts `__araigumaTopology.nodeLabels` (names + uniqueness).
- **M8** added the fabric-map screenshot and full-content captures.
- **M9** empty domain lists read "none present in this observation".

### Minor findings (accepted, documented)

- **M4** failure `error` text remains in the cell's accessible label/tooltip
  (aria-label + title); compact matrix cells are intentional.
- **M10** cosmetic: short "Needs attention" panel leaves vertical slack; the
  details panel overlays (does not shrink) the workspace by design, asserted by
  `design.spec.ts`. A scrim would be a further polish item.
- **M11** regenerated `wo-08-status.txt` now includes every untracked file.

### Publication reconciliation (WO-08 publication)

- **Inventory.** `apps/frontend/src/components/NavRail.svelte` (eight-section
  rail; DNS/Connections icons) was modified by WO-08 but omitted from the
  original §7 list; it is now enumerated. The tracked count is 21; untracked
  publication files are 13 (4 source/test, the design record, this report, and
  seven screenshots).
- **MAC-address wording corrected (documentation only).** The design record's
  "no prohibited identifiers" constraint mischaracterized the privacy contract.
  The binding MAC policy (`docs/design/prompt-03/05-logging-and-diagnostics.md`
  §9.1) governs the diagnostics **export** (field allowlist + pattern redaction
  of free-form strings), not live-UI rendering. The wording was corrected in
  place and the correction is recorded in the design record. The implementation
  was not changed: WO-08 adds no MAC rendering site, and the live UI already
  rendered the model's honest three-state `MacObservation` via
  `formatMacObservation`. No contract-prohibited data is exposed and no policy
  was silently relaxed.

## 11. Remaining defects and limitations

- No live-network verification; mock/imported/live rendering stays labeled. No
  new probes, D-Bus calls, permissions, polling, or lifecycle changes.
- No fixture in the tree contains an `Evidence::Inferred` edge, so the
  dashed/ledgered inferred rendering path is covered by code and legend but not
  by a golden document (the projection itself is unchanged and golden-stable).
- Unknown `ConnectionState {other}` and unknown enums are preserved, not mapped.
- Absent optional collections remain "not observed"; `Observed([])` remains a
  valid empty observation.
- Screenshots are synthetic scenarios (labeled MOCK DATA), not a real machine.
- The designated GLM 5.3 review could not be obtained (billing); the fallback
  review is independent but is not the named reviewer.

## 12. Complete diff summary

- `git diff --stat`: **21 tracked files, +3139 / −565**.
- Untracked: `DnsView.svelte`, `ConnectionsView.svelte`, `observability.ts`,
  `observability.test.ts`, the design record, this report, and seven
  screenshots.
- Full patch: `/tmp/opencode/wo-08-diff.patch`; status:
  `/tmp/opencode/wo-08-status.txt`.

## 13. Final local Git status and publication confirmation

- Branch `recovery/araiguma-main`, HEAD unchanged at
  `2cdccc5a80355fa908bf0c473fd9bbe131ca4c67`.
- All WO-08 changes are present and **uncommitted**.
- `git diff --check` is clean.
- **No commit, push, PR, merge, branch creation, or remote-ref change was
  performed.** WO-06F remains SHELVED.