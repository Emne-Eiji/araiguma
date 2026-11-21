# WO-04 Stage A — closeout: application shell, documents workspace, and tray-ready navigation

**Work order:** WO-04 Stage A (recovered repository history — 2026-11-20 commits; original WO-04 text not preserved — see §F)
**Project:** Araiguma
**Authority:** Emne — sole project owner and acceptance authority; this document reports findings and completes the missing governance record; it does not authorize implementation
**Status:** IMPLEMENTED — complete and functionally validated [REPO]; governance gaps listed in §F remain under review (they motivated this closeout, not this closeout's corrections)
**Closeout date:** 2026-11-21 (commits reviewed: `33d75d0` + `9808196`)
**Branch:** `recovery/araiguma-main`

## Status summary

| Item | Status |
|---|---|
| WO-04 Stage A implementation (commits `33d75d0` + `9808196`) | IMPLEMENTED — complete per commit messages; GUI present in tree [REPO] |
| Six-section application shell | PRESENT in HEAD: `App.svelte`, `NavRail`, `StatusBar`, `DataBadge`, `DetailsPanel`, `AppSidePanel`, `InterfacesView`, `OverviewView`, `RoutesView`, `RouteDetails`, `TopologyDetails` [REPO] |
| Documents & Import workspace | PRESENT: `DocumentsView`, `lib/documents.ts`, `lib/dataMode.svelte.ts`, `lib/import.ts` + validation boundary [REPO] |
| E2E shell spec | PRESENT: `apps/frontend/e2e/shell.spec.ts` (added in `9808196`) [REPO] |
| Governing closeout `wo-04-stage-a-closeout.md` | THIS DOCUMENT (the missing record) [REPO] |
| Missing archive artifacts | NOT RECOVERED — absence disclosed, not fabricated [REPO] |

## A. Commit chronology [HIST]

Two consecutive commits — one Stage A work package, on the main chain of the
recovered history (root `1a939dc`). The package immediately follows the WO-03-FC
acceptance checkpoint commit `52fffeff` (2026-11-19).

| Commit | Date (UTC) | Subject |
|---|---|---|
| `33d75d0` | 2026-11-20T20:02:47Z | `feat(gui): add scenario registry and navigation destination foundation (WO-04 Stage A)` |
| `9808196` | 2026-11-20T20:03:10Z | `feat(gui): establish application shell, documents workspace, and tray-ready navigation (WO-04 Stage A)` |

Relationship: `git rev-parse 9808196^` → `33d75d0` — the second commit is the
direct child of the first (parent–child, same day, same message tag). Five
further commits follow Stage A: `8f0451e`, `0dc962b`, `b23cbcc`, `b172450`,
`3c4954f` (2026-11-21; Prompt 03 spec + FC closeouts).

Commit `33d75d0` (+1,087/−29):

- `apps/frontend/lib/scenarios.ts` — scenario registry (five synthetic fixture pairs: workstation, incomplete-observations, server-tunnel, dummy-virtual, empty)
- `apps/frontend/lib/navigation.svelte.ts` — six stable sections + destination API (`navigateTo`/`navigate`)
- `apps/frontend/lib/documents.ts` — Documents & Import state derivation (imported-ok / missing / not-observed; rejection classification)
- `apps/frontend/lib/dataMode.svelte.ts` — active scenario + last import attempt + generation counter
- `apps/frontend/lib/fixtures.ts`, `lib/routes.ts`
- Unit tests: `tests/documents.test.ts`, `tests/navigation.test.ts`, `tests/scenarios.test.ts`
- `fixtures/README.md` documents all five snapshot fixtures
- Message: "32 new tests"

Commit `9808196` (+2,015/−389):

- `apps/frontend/src/App.svelte` — six-section shell + shared details panel
- New: `components/AppSidePanel.svelte`, `DetailsPanel.svelte` (updated), `DocumentsView.svelte`, `InterfacesView.svelte`, `NavRail.svelte` (rewritten), `OverviewView.svelte` (updated), `RouteDetails.svelte` (updated), `RoutesView.svelte` (updated), `StatusBar.svelte`, `TopologyDetails.svelte` (updated)
- `e2e/shell.spec.ts` — new shell spec (message: "plus the new shell spec (25 tests total)")

## B. Implementation summary [REPO]

The Stage A deliverable — the six-section application shell with
tray-compatible destinations — is present at HEAD:

- `apps/frontend/src/App.svelte` defines the six sections (overview, topology,
  interfaces, routes, providers, documents) with the shared details panel and
  bottom status area; navigation store (`lib/navigation.svelte.ts`) declares
  the stable section ids and destination API without an event bus (Tray
  foundation only).
- Data source (`lib/dataMode.svelte.ts`): mock scenario + imported documents,
  honest provenance badges (`MOCK DATA` / `IMPORTED OBSERVATION`), status bar
  with observation/import times (no invented timestamps).
- `apps/frontend/e2e/shell.spec.ts` is the e2e spec (added in `9808196`).

## C. Relationship assessment [HIST][INFER]

One Stage A work package, represented by **two sequential commits** — not two
separate work orders: same commit-message tag `(WO-04 Stage A)`, consecutive
on-chain commits (parent–child), same day, distinct scopes (library
foundations → shell).

## D. Evidence of completion [REPO][ENV]

- Both commit messages describe the complete deliverables; the Stage A tree
  contains the six-section shell, scenario registry, destination API, and e2e spec.
- GUI functional evidence (recorded during WO-LOCAL-ONBOARDING-01): all six
  views navigated in the in-app browser; no console errors observed.
- `e2e/shell.spec.ts` present at HEAD.
- Prior session validation: frontend suite green (168 vitest, 0 failed;
  svelte-check 0/0) — historical evidence from WO-LOCAL-ONBOARDING-01.

## E. Authorization and completion assessment [INFER]

- **Authorization before work:** no standalone WO-04 text or pre-work
  authorization record exists in this repository (`git ls-files | grep -i wo04`
  → empty) and in history (`git log --all --diff-filter=A -- '*wo04*'` → none).
  The Stage A *work* is attributed post-facto: the Prompt 03 closeout lists a
  "WO-04 Stage A accepted checkpoint" (§G) and the design docs
  (`01-backend-architecture.md:16–17`: "repository at baseline `9808196`
  already contains a working, tested observation pipeline (WO-01R through WO-04
  Stage A)") treat Stage A as accepted baseline.
- **Completion:** functionally complete — the GUI exists and is exercised
  (§D). Acceptance is recorded as the post-facto checkpoint reference, not as
  an independent closeout.

## F. Missing governance records

1. `docs/work-orders/wo-04-stage-a-closeout.md` — this document (the record
   that should have existed; see [DOC] gap report WO-GOV-RECON-01).
2. README status entry for Stage A — absent: the README §Status lists
   WO-01R/WO-02/WO-03 only.
3. Referenced archive artifacts — not present in the tree (§I).

## G. Relationship to Prompt 03 and 03-FC [DOC][REPO]

- `docs/design/prompt-03/README.md:5–6`: "verified baseline `main` @
  `98081962403f55e332fc8b13f5209594bb04ca2e` (WO-04 Stage A, 50 commits)".
- `docs/work-orders/prompt-03-closeout.md:30–35`: "Checkpoint selected: WO-04
  Stage A accepted checkpoint `araiguma-WO04-STAGEA-03` (the Stage A closeout's
  final row: shell HEAD)" — historical claim; the closeout record itself was
  never delivered (see §I).
- `docs/work-orders/wo-03-fc-closeout.md` does not name Stage A.

## H. Publication authorization evidence [DOC][REPO]

Stage A is unrelated to publication. The evidence for the later publication on
`recovery/araiguma-main` is the commit `08e58f3` message: "The complete
verified history is published on the `recovery/araiguma-main` branch of this
repository (WO-RECOVERY-03, authorized by Emne)" — and the remote README text.
Not repeated; nothing pushed under this WO.

## I. Missing archive artifacts [REPO]

`prompt-03-closeout.md:30–35` references, but the current tree contains **no**:

- the `download/` directory,
- `araiguma-WO04-STAGEA-03-repo.tar.gz`,
- `araiguma-WO04-STAGEA-03.bundle`.

Disclosed without fabrication; if Emne locates them elsewhere, they are
documented as "recovered elsewhere" in a follow-up.

## J. Validation limitations [UNKNOWN]

- No automated Stage A-specific gate exists; no independent author review of
  the Stage A work was performed during this WO.
- e2e for Stage A was not run (Playwright Chromium build 1255 missing on the
  investigation workstation).
- The original Stage A text (WO-04) is lost; this closeout reconstructs
  scope from commits and design-doc credits (see §E).

## K. Deviations from this closeout scope

None. This document closes Stage A governance (§F) and documents the work
(§B–E). No implementation changes, no publication operations, no branch
changes.
