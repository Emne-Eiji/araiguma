# Prompt 03 closeout — backend architecture and network data model

**Work order:** Prompt 03 (Emne, 2026-11-21) — backend architecture and
network data model specification. **Type:** engineering design and
specification; implementation explicitly not authorized. **Acceptance
authority:** Emne. **Execution window:** 2026-11-21T05:56Z–06:19Z UTC
(all timestamps UTC, per the work order's project-date rule).
**Status:** specification package complete, validated, committed,
checkpointed. **This report is a delivery statement, not an acceptance
claim.**

> **Amended by Prompt 03-FC (2026-11-21).** An independent review
> identified limited corrections, applied by the Prompt 03-FC
> consistency-correction pass and marked **[FC-xx]** where they touch
> statements below: FC-01 corrected the IPC surface to four commands
> (§D.6); FC-05 documented a delivery-packaging omission of the
> PROMPT03-SPEC bundle (§G). The original text of this report is
> preserved verbatim in commit `0dc962b` and the
> `araiguma-PROMPT03-CLOSEOUT` bundle; amendments are explicit, never
> silent replacements.

## A. Repository baseline — before and after

**Before work (recovery verification, 2026-11-21T06:05:58Z [REPO]):**

The last reported sandbox state had an empty project working directory
(platform-side recycle), as the work order anticipated. Recovery
procedure executed per §7:

- Checkpoint selected: **WO-04 Stage A accepted checkpoint
  `araiguma-WO04-STAGEA-03`** (the Stage A closeout's final row: shell
  HEAD). Manifest checksums verified **before** extraction:
  `araiguma-WO04-STAGEA-03-repo.tar.gz` OK,
  `araiguma-WO04-STAGEA-03.bundle` OK. [REPO — manifest
  `download/araiguma-WO04-STAGEA-03.sha256`]
- Archive extracted to exactly `/home/z/my-project/araiguma`; verified:
  HEAD `98081962403f55e332fc8b13f5209594bb04ca2e`; branch `main`;
  50 commits; working tree clean (porcelain empty); **no remote
  configured**; `git fsck --full` reports only the two documented
  transient Stage A stash objects (`9ae9964b`, `f2c28317` — Stage A
  closeout §I.5, expected); independent clone from the verified bundle
  yields the identical tree hash `c4b53467…` and identical
  `git ls-files -s` index — tracked content equality proven.
  [REPO — recorded in
  `download/araiguma-P03-recovery/baseline-verification.txt`]
- The platform accounting repository `/home/z/my-project/.git` was never
  touched; all Git operations used explicit
  `git -C /home/z/my-project/araiguma …` or an in-repository cwd.

**After work:** HEAD `8f0451ebbd33e0dcc8f8b8e1a400f313f6da23f0`
(51 commits), `main`, clean, no remote, identity audit: all commits
authored and committed by `Emne <https://github.com/Emne-Eiji>`
(verified by `milestone.sh` at the PROMPT03-SPEC checkpoint). [REPO]

## B. Exact files created or modified

Created (commit `8f0451e`, 11 files, +2,251 lines): [REPO]

- `docs/design/prompt-03/README.md` — package index, standing rules,
  reading order
- `docs/design/prompt-03/01-backend-architecture.md` — layers, crates,
  dependency direction, observation engine, lifecycle, runtime
  ownership, integrations, tray constraints, accounting extension
  points
- `docs/design/prompt-03/02-provider-contract.md` — identity,
  capability/permission/availability axes, lifecycle, observation and
  events, errors, health, reconciliation, shutdown
- `docs/design/prompt-03/03-data-model-and-evidence.md` — entity model,
  ten-state evidence vocabulary, absence semantics, conflicts,
  staleness, unresolved rules, DNS/rules/VPN entities, status
  vocabulary reservation
- `docs/design/prompt-03/04-document-and-ipc-contract.md` — one
  contract (CLI/import/IPC), schema v4 envelope, validation and atomic
  replacement, IPC surface, event continuity, runtime ownership
- `docs/design/prompt-03/05-logging-and-diagnostics.md` — OD-08/OD-09
  design: locations, config, JSONL logging, rotation, session registry,
  export redaction, unwritable-path behavior
- `docs/design/prompt-03/06-security-read-only-enforcement.md` —
  prohibitions table, five enforcement layers, concrete D-Bus
  allow-list, Tauri capabilities, audit gate design
- `docs/design/prompt-03/07-testing-strategy.md` — two-track model,
  every required test category, state-comparison approach, OD-10
  checkpoint, validation matrix
- `docs/design/prompt-03/08-decision-register.md` — approved /
  recommended / unresolved / assumptions / future work with full
  traceability
- `scripts/check-prompt03-docs.sh` — documentation validation
  (structure, links, register consistency, placeholders)
- Modified: `docs/architecture.md` (+12 lines) — a "Design trajectory"
  section pointing to the package; the document still describes only
  the current implementation, clearly separated from the target design

Created outside the repository (workspace/evidence, not commits):
`download/araiguma-P03-recovery/baseline-verification.txt`; the
milestone checkpoint artifacts of §G. This closeout report
(`docs/work-orders/prompt-03-closeout.md`) is the second commit.

**Untouched:** every Rust crate, `apps/frontend`, `fixtures/`,
`Cargo.lock`, `Cargo.toml`, `rust-toolchain.toml`, all other scripts,
all other documentation. [REPO — `git show --stat 8f0451e` above]

## C. Architecture summary [DESIGN]

Four layers with enforced dependency direction: presentation
(`apps/frontend`, documents only) ← application (`apps/desktop`, the
only Tauri-dependent crate, OD-01) ← observation & domain (`core`,
`netlink`, `sysfs`, new `engine`; future dbus provider crate) — with
the JSON document contract as the single cross-cutting interchange
(OD-03). The Rust backend stays authoritative for observations,
attribution, events, and accounting; the frontend renders and never
invents. A new `crates/engine` centralizes the orchestration the CLI
`watch` loop owns today, so CLI and desktop shell are interchangeable
consumers; providers are injected trait implementations, capability-
probed, permission-aware, and degrading gracefully when absent or
denied (OD-02). Runtime ownership is unambiguous: the embedding binary
owns the Tokio runtime; library crates never construct one. The
read-only invariant is enforced in five layers, anchored by structural
restrictions and a semantics-verified D-Bus allow-list. Logging,
diagnostics, and session state live at the OD-08 XDG locations with
bounded retention and allowlist-based redaction (OD-09).

## D. Major data model and contract decisions [DESIGN]

1. **Evidence vocabulary (ten states)** with a four-way absence
   distinction: observed-absent requires a successful, scope-complete
   observation; absence-of-evidence, collection failure, and
   unsupported never become negative facts. Unknown stays unknown.
2. **Schema v4 envelope** (additive, per the v2/v3 precedent):
   document identity (`meta.documentId`), producer channel, a mandatory
   synthetic label for fixture/mock data, collection/session identity,
   observation timing; per-provider capability and health metadata in
   every document.
3. **Observation time and import time are separate facts** — the
   document carries only observation timing; the importer records
   import time in session state; the artifact is never mutated.
4. **Conflict recording**: field and existence conflicts retain every
   assertion and its provider; priority resolves presentation only.
5. **New entities**: routing-policy rules (raw, no inference), DNS
   configuration observations (no resolution probing, no "DNS works"
   claims), VPN evidence as classified facts (never peer edges);
   veth peers and all cross-namespace references stay `unresolved`
   (never inferred from naming or weak evidence).
6. **One contract, three transports**: CLI stdout, file import, and IPC
   deliver identical documents; watch envelopes gain a monotonic `seq`
   with snapshot-first gap recovery; the IPC command surface of
   document 04 §9 — **[FC-01]** amended: four commands (`get_snapshot`,
   `get_topology`, `subscribe_watch`, `unsubscribe_watch`) with a typed
   three-code error model in which observation failures live inside
   documents, never as transport errors.
7. **Provider contract**: identity as stable strings with verbatim
   unknown-id passthrough; capability probing separated from
   observation; `permissionDenied` as a first-class outcome; change-
   feed abstraction with engine-owned debounce and poll fallback;
   provider-owned resources with terminal shutdown.
8. **Read-only enforcement**: prohibited-operation table with named
   enforcement layers; restricted D-Bus proxies over an allow-list that
   verifies operation *semantics* (not names — `CheckConnectivity`,
   `RequestScan`, resolved query methods are prohibited despite
   observational names); Tauri capabilities `core:default` only;
   an eight-check static audit gate; before/after state-comparison
   invariant tests in netns (Track 1) and on the real host (Track 2).

## E. Traceability to approved decisions [DOC]

OD-01 → 01 §2/§8, 06 §5, 07 §9. OD-02 → 02 §1/§3, 03 §1, 07 §8.
OD-03 → 04 (entire), 07 §6. OD-08 → 05 §1. OD-09 → 05 (entire).
OD-10 → 07 §8–9. The full map, including per-section sources, is
`08-decision-register.md` §6. OD-04..OD-07 remain open (register §2,
UD-01..04) with dependencies stated and recommendations clearly
labeled as recommendations — none was treated as approved.

## F. Documentation validation results [TEST]

`scripts/check-prompt03-docs.sh` — **RESULT: PASS** (executed
2026-11-21T06:16Z at `8f0451e`'s tree): package completeness (9/9);
relative-link resolution; RD register cross-reference consistency
(every RD-01..30 used in documents 01–07 is defined; numbering
complete, no gaps); decision-id vocabulary check (only OD-01..OD-10
referenced, none invented); placeholder scan clean; deliverable
headers; balanced code fences; architecture.md pointer present.
`bash -n` on the script itself: clean. No other claims of test
execution are made; no application tests were run because no
application code changed.

## G. Git commits, recovery artifacts, and SHA-256 verification

Commits (both documentation-only, `main`): [REPO]

1. `8f0451ebd33e0dcc8f8b8e1a400f313f6da23f0` — the specification
   package + validation script + architecture pointer (§B).
2. The closeout commit (this file) — recorded in the PROMPT03-CLOSEOUT
   checkpoint created immediately after it.

Recovery checkpoints (archive + bundle + SHA-256 manifest, per the
standing convention; earlier artifacts untouched): [REPO]

| Checkpoint | Commit | Artifacts | Verification |
|---|---|---|---|
| PROMPT03-SPEC | `8f0451e` (51 commits) | `download/araiguma-PROMPT03-SPEC.{tar.gz,bundle,.sha256}` | created by `scripts/milestone.sh` (clean tree, identity audit, fsck, bundle verify, budget OK — 942 MB working / 934 MB store against 950 MB); manifest re-verified `sha256sum -c`: bundle OK, archive OK |
| PROMPT03-CLOSEOUT | this closeout commit | `download/araiguma-PROMPT03-CLOSEOUT.{tar.gz,bundle,.sha256}` | same procedure, executed immediately after this commit |

PROMPT03-SPEC SHA-256 (from the verified manifest): bundle
`5d0060eb9e5e92b00c2a4b2bca6f76513c1ac842f107430194f3d23267e2c489`;
archive `bbfe940e7a3b277c5c9b742fe0833b450b0d313964710f9dc96c861264249c5d`.

> **[FC-05] Packaging omission, corrected by Prompt 03-FC:** the
> PROMPT03-SPEC artifacts above were created and verified in the
> workspace during execution (checksums in the manifest, bundle
> clone-verified at `8f0451e`, 51 commits) but were **omitted from the
> delivery ZIP** sent for review — a packaging discrepancy, not a
> history defect. The PROMPT03-CLOSEOUT bundle (the complete 52-commit
> history, including the specification commit) was present and
> independently cloneable, so recovery was never at risk. The Prompt
> 03-FC closeout report and its corrected recovery manifest document
> the full artifact inventory.

**Durable-storage note [ENV]:** the budget check passes but is within
the 50 MB warning band on both measurements (R-04). No persistent
additions were made beyond the specification text and checkpoint
artifacts; the tight margin is recorded here so the next work order
starts from a known number.

## H. Known limitations and tests deferred to Ubuntu 24.04 [DESIGN]

- The entire specification is unimplemented and unvalidated against
  running code by design; every statement is design-level.
- Track 2 (OD-10) items are specified (07 §9) and deferred: thin Tauri
  slice, .deb lifecycle, live NetworkManager/systemd-resolved probes
  including permission behavior, real-host read-only comparison.
- Assumptions A-1..A-6 (08 §4) carry their validation paths; none is
  silently relied upon by code.
- The sandbox cannot execute D-Bus provider tests against real
  services (no system bus) [ENV — WO-02 §6.1]; Track 1 coverage is the
  private-session mock harness only.

## I. Deviations from scope

None. No source code, dependency, application configuration, remote,
GitHub access, credential use, or implementation work was performed or
authorized; Prompt 04/05/06/07 scopes and WO-TRAY-01 were not started;
the six approved decisions were implemented as design only and never
reopened; OD-04..OD-07 were preserved as open. The one judgment call:
the deliverable set is a structured nine-document package (a README
index plus eight focused documents) rather than a single monolithic
file, which the work order explicitly permits and which the repository's
`docs/design/` convention supports; the README is the coherence map.

## J. Stop condition

The Prompt 03 specification package and this closeout report are
complete. **STOP. Prompt 04 and implementation are not begun.** The
deliverables await Emne's independent review; Emne alone decides
acceptance and authorizes the next stage.

*Prepared by the implementation agent, 2026-11-21 (UTC). Evidence
labels used throughout: [REPO] repository-verified, [DOC]
document-verified, [ENV] environment-verified, [TEST]
executed-test-verified, [DESIGN] design-level statement (no runtime
claim), [UNKNOWN] none remaining for this work order.*
