# Prompt 03-FC closeout — specification consistency correction

**Work order:** Prompt 03-FC (Emne, 2026-11-21) — Specification
Consistency Correction. **Type:** documentation-only specification
correction, consistency validation, and recovery-artifact repair.
**Parent:** Prompt 03 (backend architecture and network data model).
**Implementation authorization:** NOT GRANTED — none requested, none
used. **Acceptance authority:** Emne. **Execution window:**
2026-11-21T06:52Z–07:10Z UTC (project date convention: UTC). **Status:**
corrections FC-01..FC-06 applied, validated, committed, checkpointed.
**This report is a delivery statement, not an acceptance claim.**

## A. Baseline and recovery

- **Repository root:** `/home/z/my-project/araiguma` (verified before
  every modifying Git operation; the platform accounting repository
  `/home/z/my-project/.git` was never touched — all operations used
  explicit `git -C /home/z/my-project/araiguma …`). [REPO]
- **Recovery source:** none required — the working tree survived from
  the Prompt 03 session (no platform recycle between Prompt 03 and
  03-FC). Baseline verified against artifacts before any change:
  HEAD `0dc962be5e40d858c205e4dadf16ff5dd7112cec`, branch `main`,
  52 commits, clean tree, no remote, `git fsck --full` showing only the
  two documented Stage A transient stash objects; PROMPT03-CLOSEOUT
  bundle clone tree-identical to the working repository at `0dc962b`
  (`git ls-tree -r HEAD` equal); PROMPT03-SPEC and PROMPT03-CLOSEOUT
  manifest checksums re-verified (`sha256sum -c`: all OK). [REPO]
- **Initial HEAD:** `0dc962b` (52 commits). **Final HEAD:** this
  closeout commit (the third FC commit; the two preceding are
  `b23cbcc`, `b172450`; the final hash and its recovery artifacts are
  recorded in `download/araiguma-PROMPT03-FC.sha256`). Branch `main`,
  55 commits at this commit, clean tree, **no remote configured**.
  [REPO]
- **fsck (final):** three dangling commits only — the two documented
  Stage A transient stash objects (`9ae9964b`, `f2c28317`) plus
  `c58c5dae`, the documented transient stash created to checkpoint
  commit `b23cbcc` with a clean tree (dropped immediately after; same
  pattern as the Stage A precedent). No corruption, no missing objects.
  [REPO]
- **FC commits (all documentation-only; `git diff --name-only
  0dc962b..HEAD` = 10 `.md` + 1 `.sh`):**
  1. `b23cbcc` — FC-01/02/03/04 specification corrections, 12
     cross-reference fixes, validation-script extension (checks 9–15).
  2. `b172450` — Prompt 03 closeout amendments (FC-01/FC-05 notes).
  3. This closeout commit.
- **Identity audit:** every FC commit authored and committed by
  `Emne <https://github.com/Emne-Eiji>` (`git log --format='%an <%ae> /
  %cn <%ce>'` — single unique identity). [REPO]
- **Independent clone comparison:** checkpoints FC1 (at `b23cbcc`) and
  FC2 (at `b172450`) each verified by the checkpoint procedure —
  checksums, `git bundle verify`, fresh clone (HEAD and commit count
  match), archive extraction (HEAD/count match, fsck clean, `ls-tree
  -r` identical to the repository). The final PROMPT03-FC checkpoint
  (this commit) is created and verified by the same procedure
  immediately after this commit; its results are recorded in the
  corrected recovery manifest. [REPO/TEST]

## B. Correction-by-correction results

### FC-01 — IPC subscription cancellation contract — **PASS**

- **Finding:** document 04 §9 declared three commands while the
  subscription lifecycle used `unsubscribe_watch { subscriptionId }` —
  an undeclared command (internal contract inconsistency). [DOC]
- **Correction:** `unsubscribe_watch` added as the explicit **fourth**
  command (`{ "subscriptionId": string }` → `Cancelled {
  subscriptionId }`); document 04 §9 rewritten as a complete lifecycle
  contract: subscription identity (engine-generated unguessable UUID
  v4, session-scoped, never reused), creation, active delivery,
  cancellation semantics (idempotent for session-issued ids; typed
  `invalidRequest` naming unknown ids), terminal `ended` control
  envelope (exactly once; `seq` = highest delivered document-event seq;
  consumes no counter; nothing delivered after it; CLI has no `ended`
  — envelope-equality tests compare document events only), disconnect
  cleanup (server-side release regardless of delivery success),
  shutdown cleanup, and task/resource ownership (engine owns; no
  orphaned task permitted).
- **Files changed:** `04-document-and-ipc-contract.md` (§9, §12 row),
  `06-security-read-only-enforcement.md` (§5 four-command surface),
  `07-testing-strategy.md` (§6 lifecycle tests: cancellation, repeated
  cancellation, unknown ids, disconnect, shutdown, no-post-`ended`
  delivery, counter invariance), `08-decision-register.md` (OD-03 row),
  `docs/work-orders/prompt-03-closeout.md` (§D.6 amendment).
- **Verification:** validation check 9 (all four commands declared as
  table rows; "Four commands" in 04; "four IPC commands" in 06; "all
  four commands" in 07; no stray command names anywhere in the
  package) — PASS. Manual audit: every command referenced in any
  lifecycle or test text is declared in the §9 inventory. [TEST]
- **Remaining limitation:** command names remain candidates per the
  standing rule (names refined from WO-02 §2.3); no code exists — the
  contract is design-level. [DESIGN]

### FC-02 — D-Bus property access allow-list — **PASS**

- **Finding:** generic allowances for `Properties.Get`/`GetAll` (and
  `ObjectManager.GetManagedObjects`) could expose properties outside
  the observation scope. [DOC]
- **Correction:** document 06 §4 restructured to per-operation
  granularity — §4.1 primitives table with `Properties.Get` scoped to
  the exact (service, object-path-pattern, interface, property) rows;
  **`GetAll` prohibited for v1.0.0** (no candidate interface is
  fully in reviewed scope — e.g. NM Manager `GlobalDnsConfiguration`,
  Device `Udi`/`Vendor`/`Product`, resolved Manager `Transactions`;
  named `Get` only; amendment path stated); **`GetManagedObjects`
  prohibited** (unboundable); §4.2/§4.3 NetworkManager and
  systemd-resolved tables with explicit columns: object path (pattern),
  interface, member, operation, purpose/observation requirement, data
  classification; NM IP/DHCP config objects' inner properties excluded
  (correlation paths only; addresses are netlink-authoritative);
  §4.4 systemd-networkd allow-listed to **nothing** (deny-by-default
  applies to anticipation; Prompt 05 must amend through review);
  §4.5 semantics-not-names rule retained and extended; §4.6 enforcement
  (no free-form call surface; member+interface+path audit;
  `GetAll`/`GetManagedObjects` negative assertions). Failure/permission
  behavior stated once for all rows (permissionDenied/Failed are
  evidence limitations, never absence claims). Settings vs secrets
  treatment preserved (`GetSettings` allowed, `GetSecrets` prohibited).
- **Files changed:** `06-security-read-only-enforcement.md` (§4, §6
  check 5, §8 RD-25 row), `07-testing-strategy.md` (§4 negative tests:
  prohibited members, unlisted properties, `GetAll`,
  `GetManagedObjects`), `08-decision-register.md` (RD-25 row).
- **Verification:** validation check 11 — PASS. Manual audit: every
  member of the original allow-list is preserved in the restructured
  tables (none silently dropped); prohibitions (`GetSecrets`,
  `CheckConnectivity`, `RequestScan`, `FlushCaches`, `Resolve*`,
  `Properties.Set`, state-changing methods) all retained. [TEST]
- **Remaining limitation:** the allow-list is a reviewed design
  artifact; its mechanical source-of-truth file and audit script are
  implementation-time deliverables (RD-27). [DESIGN]

### FC-03 — Diagnostics privacy default — **PASS**

- **Finding:** the export retained the first three MAC octets by
  default — exposing hardware-vendor information. [DOC]
- **Correction:** **full MAC redaction is the default** (all six
  octets, marker `<redacted>`); `privacy.mac_addresses = "redact"
  (default) | vendor-prefix | include"` — partial disclosure only as an
  explicit user-controlled option, recorded in the export's
  self-describing manifest; new §9.1 field-level privacy policy: 13
  categories (MAC, IPv4, IPv6, interface names, SSIDs, VPN
  names/identifiers, route destinations/gateways, DNS
  servers/search domains, hostnames, user/home paths, hardware
  identifiers, provider object paths, timestamps/metadata), each with
  inclusion-by-default, treatment (redacted/generalized/omitted/
  retained), diagnostic necessity, user control, and exposure; §3
  detail-string content rule (SSIDs, VPN names, hostnames, paths never
  enter free-form log strings); allowlist + pattern-pass architecture
  (each bounds a different surface); hard-exclusion list retained and
  extended; `privacy.ip_addresses` toggle added; redaction test corpus
  specified (MAC all modes, IPv4/IPv6 both modes, SSID-bearing,
  VPN-name-bearing, home-path, secret-bearing inputs +
  field-allowlist totality).
- **Files changed:** `05-logging-and-diagnostics.md` (§2 config, §3
  content rule, §9 + §9.1, §11 RD-24 row), `07-testing-strategy.md`
  (§5 redaction tests), `08-decision-register.md` (RD-24 row).
- **Verification:** validation checks 13 and 14 — PASS (default key
  present; "fully redacted" stated; obsolete `redact_macs_in_export`
  key absent; "first three octets" appears only in explicit-opt-in
  context; all six required categories addressed). [TEST]
- **Remaining limitation:** documentation claims are now exactly as
  enforceable as specified: the field allowlist and pattern pass exist
  as design; their unit tests are specified (document 07 §5) and land
  with the implementation. [DESIGN]

### FC-04 — RD-01..RD-30 decision governance — **PASS**

- **Finding/classification exercise:** all 30 recommendations reviewed
  for clarity and implementation impact. [DOC]
- **Correction:** document 08 §3.1 "Implementation disposition" added —
  per-RD table preserving the original recommendation, rationale,
  affected subsystem, implementation impact, dependencies,
  blocks-implementation, and disposition under an exact three-value
  vocabulary: **20 × REQUIRES EMNE DECISION** (RD-01..07, RD-12, RD-13,
  RD-15, RD-17..21, RD-23..27), **9 × NON-BLOCKING DESIGN GUIDANCE**
  (RD-08, RD-09, RD-10, RD-14, RD-16, RD-22, RD-28, RD-29, RD-30),
  **1 × DEFERRED TO A LATER PROMPT** (RD-11 → Prompt 05). Preamble
  states the approval status of every RD: **NOT APPROVED** — the
  disposition confers no approval. Specific-attention items (RD-01,
  RD-06, RD-08..10, RD-15, RD-18, RD-25..27) each state whether the
  spec merely recommends or implementation would require Emne's
  approval of the specific choice. **Schema-version note:** the
  existing rationale for v4 is documented — the repository's shipped
  series v1 → v2 → v3 (`SNAPSHOT_SCHEMA_VERSION = 3`; v2 = the WO-02
  correction MAC-ambiguity fix, v3 = the WO-03 additive
  `peerIfindex`/multipath bump, each recorded in
  `crates/core/src/snapshot.rs` and `docs/development.md`); v4 is the
  next additive bump extending v3 with the `meta` envelope; version
  left unchanged; no v1/v2/v3 compatibility obligations invented.
  OD-01/02/03/08/09/10 remain exactly the approved decisions; OD-04..07
  remain open (register §2 untouched).
- **Files changed:** `08-decision-register.md` (§3 RD-24/25 text
  updates, new §3.1, §7).
- **Verification:** validation check 12 — PASS (30/30 rows, exact
  vocabulary, none approved, blanket NOT-APPROVED statement present).
  Manual audit: no RD marked approved, no approval date invented,
  register §1/§2/§4/§5/§6 otherwise unchanged. [TEST]
- **Remaining limitation:** none identified. [DOC]

### FC-05 — Recovery-package manifest discrepancy — **PASS**

- **Finding:** the Prompt 03 delivery ZIP lacked
  `araiguma-PROMPT03-SPEC.bundle` although the SPEC manifest referenced
  it (the closeout bundle was present and cloneable). [DOC — work
  order §8.1]
- **Investigation:** the SPEC bundle **exists in the workspace**,
  created 2026-11-21T06:18:49Z during the Prompt 03 execution window
  (its manifest header, `baseline-verification.txt`, and the Prompt 03
  closeout §G all record its creation). Verified this session:
  checksum matches `araiguma-PROMPT03-SPEC.sha256` [OK]; `git bundle
  verify` okay (complete history, `refs/heads/main` = `8f0451e`);
  fresh clone succeeds (51 commits); **clone tree identical to the
  repository at `8f0451e`** (`git ls-tree -r` equal). Conclusion: work
  order §8.2 explanation **(2) — generated but omitted from the
  delivery ZIP**. The exact packaging step that dropped it is not
  observable from inside the sandbox [UNKNOWN]; the artifact itself is
  proven intact. No regeneration was needed; nothing was fabricated.
- **Repair:** original artifacts, manifests, and checksums preserved
  untouched (all re-verified OK). The Prompt 03 closeout §G carries an
  explicit [FC-05] amendment documenting the omission and why the
  PROMPT03-CLOSEOUT bundle (complete 52-commit history incl. the
  specification commit) kept recovery intact. A **corrected recovery
  manifest** (`download/araiguma-PROMPT03-FC-recovery-manifest.md`)
  now accurately describes every recovery artifact that exists —
  original, corrected, and final — with verified checksums, roles, and
  the packaging-omission explanation. The final PROMPT03-FC bundle
  remains independently cloneable (verified by the checkpoint
  procedure).
- **Verification:** all 15 `.sha256` manifests in `download/` verified
  (`sha256sum -c`: every listed artifact OK — including every original
  checkpoint); all 13 bundles `git bundle verify` okay with complete
  histories. [TEST]
- **Remaining limitation:** the contents of the delivered ZIP cannot be
  re-inspected from inside the sandbox [UNKNOWN]; the correction
  guarantees the *current and final* manifests describe exactly what
  exists.

### FC-06 — Storage budget and recovery safety — **PASS (with standing warning)**

- Section F reports the measurements. Headroom was sufficient for this
  correction; the standing WARN-zone condition (within 50 MB of the
  950 MB limit) persists and is reported, not hidden.

## C. Decision governance (unchanged by this correction)

- **Approved decisions consumed (exactly these six, unchanged):**
  OD-01 (Tauri 2 shell), OD-02 (optional capability-probed
  NetworkManager/systemd-resolved providers, graceful degradation),
  OD-03 (Tauri IPC live snapshots/events; file import retained; one
  JSON document contract), OD-08 (XDG locations), OD-09 (Prompt 03
  logging/rotation/diagnostics design), OD-10 (Ubuntu 24.04 validation
  checkpoint after Prompt 03, before Tauri implementation). [DOC]
- **Still open (unchanged):** OD-04 (D-Bus crate choice, UD-01),
  OD-05 (distribution format, UD-02), OD-06 (update mechanism, UD-03),
  OD-07 (tray timing/scope, UD-04). [DOC]
- **RD-01..RD-30: all remain unapproved recommendations** — §3.1
  classifies implementation disposition only; no RD was promoted,
  merged, or reclassified; no approval date exists. [DOC]
- **Assumptions remain assumptions:** A-1..A-6 (document 08 §4),
  each with basis and validation path; none asserted as fact. [DOC]

## D. Cross-document consistency audit (§10 of the work order)

| # | Audited contract | Result |
|---|---|---|
| 1 | README index and amendment record match the package contents | agree [TEST check 1 + manual] |
| 2 | Backend architecture and crate boundaries (doc 01) | consistent; 8 stale section references fixed |
| 3 | Provider contract and lifecycle (doc 02) | consistent; 2 references fixed |
| 4 | Network data model (doc 03) | consistent; 1 reference fixed |
| 5 | Evidence and uncertainty semantics (doc 03 §1) | ten-state vocabulary used consistently across all documents (occurrence scan) [TEST] |
| 6 | Shared JSON document schema (doc 04) | schema v4 consistent everywhere [TEST check 15] |
| 7 | IPC command and event contracts | four commands agree across documents 04/06/07/08; no stray command names [TEST check 9] |
| 8 | Subscription creation/cancellation/shutdown behavior | doc 04 §9 lifecycle = doc 07 §6 tests = doc 01 §6.3 shutdown; `ended` semantics identical everywhere [manual] |
| 9 | Tokio runtime and task ownership | doc 01 §8 rules = doc 04 §11 summary [manual] |
| 10 | Event sequencing and resynchronization | doc 04 §7–8 = doc 07 §7 = doc 01 §9 (seq, snapshot-first, R-10) [manual] |
| 11 | Schema-v4 references | agree [TEST check 15] |
| 12 | Import and IPC use the same contract | doc 04 §1/§10 unchanged, consistent [manual] |
| 13 | Observation time vs import time distinct | doc 04 §2 (RD-17) intact [manual] |
| 14 | Evidence-state terminology | consistent [TEST occurrence scan] |
| 15 | Provider health not conflated with network health | docs 02 §1/§8, 03 §11 intact [manual] |
| 16 | D-Bus restrictions agree across architecture/provider/security | doc 01 §6.2/§7.3, doc 02 §4/§11 all point to the restructured doc 06 §4 [TEST check 10 + manual] |
| 17 | Diagnostics privacy statements match the export field allow-list | doc 05 §9/§9.1 internally consistent; doc 06 §4.2 privacy-noted pointers resolve to §9.1 [TEST checks 13/14] |
| 18 | Approved decisions remain exactly the approved decisions | [TEST check 4] |
| 19 | OD-04..OD-07 remain open | [manual, register §2] |
| 20 | RD-01..RD-30 remain unapproved | [TEST check 12] |
| 21 | Assumptions distinguishable from facts and decisions | register §4 unchanged [manual] |
| 22 | Future tray compatibility remains architectural only | docs 01 §9, 03 §11 unchanged; only negative-scope mentions of tray internals [TEST/manual scan] |
| 23 | No tray implementation or notification lifecycle introduced | scan clean [TEST] |
| 24 | Ubuntu 24.04 validation remains assigned to OD-10 | doc 07 §9 unchanged [manual] |
| 25 | Cross-document section references resolve | all "document NN §N" references resolve against actual headings [TEST check 10 — this check caught and drove the 12 fixes] |

Additional findings fixed during the audit (concrete inconsistencies,
authorized by §10): twelve wrong cross-document section references
(doc 01 ×8, doc 02 ×2, doc 03 ×1, plus one self-introduced reference
corrected before commit) and the command-count statements listed under
FC-01. No prose was rewritten for style.

## E. Validation (executed and recorded — nothing claimed that was not run)

**Automated documentation validation [TEST]:**

1. `bash -n scripts/check-prompt03-docs.sh` — OK (syntax).
2. `scripts/check-prompt03-docs.sh` (extended, checks 1–15) — **RESULT:
   PASS** at `b23cbcc` (after the corrections) and re-run **PASS** at
   `b172450` (final content tree): package completeness 9/9; relative
   links resolve; RD cross-references consistent; OD vocabulary
   correct; no placeholders; deliverable headers; fences balanced;
   architecture.md pointer; **new:** IPC command inventory complete and
   consistent; all cross-document section references resolve; D-Bus
   allow-list scoping statements present; RD disposition 30/30 with
   exact vocabulary and none approved; full-MAC-redaction default with
   obsolete key absent and partial disclosure only as explicit opt-in;
   all FC-03 privacy categories addressed; schema v4 consistent.
3. The original 8-check script also passed on the corrected documents
   before the extension was added (transitional run).

**Artifact-integrity verification [TEST]:**

4. `sha256sum -c` on **all 15** `.sha256` manifests in `download/` —
   every listed artifact OK (originals untouched and re-verified:
   FINAL, WO01R, WO02×2, WO03, WO03F, WO03FC, STAGEA-01-defective,
   STAGEA-02, STAGEA-03, PROMPT03-SPEC, PROMPT03-CLOSEOUT, icons; new:
   FC1, FC2).
5. `git bundle verify` on **all 13** bundles — all okay, complete
   histories, expected HEADs (…`9808196`, `52fffeff`, `8f0451e`,
   `0dc962b`, `b23cbcc`, `b172450`).
6. Fresh clones of PROMPT03-SPEC (`8f0451e`, 51 commits),
   PROMPT03-CLOSEOUT (`0dc962b`, 52), FC1 (`b23cbcc`, 53), FC2
   (`b172450`, 54) — HEAD and commit count match; FC1/FC2 clone trees
   identical to the repository (checkpoint procedure);
   PROMPT03-SPEC clone tree identical to the repository at `8f0451e`
   (the FC-05 proof).
7. Checkpoint artifact verification (FC1, FC2): archive extracts, HEAD
   and count match, fsck clean on the extracted `.git`, tracked
   content identical.

**Environment measurements [ENV]:** see §F.

**Manual specification review [DOC]:** full re-read of all nine package
documents plus both closeout reports against the §10 checklist
(results in §D).

**Not performed in the sandbox (cannot be):** any application test (no
application code changed — nothing to run); D-Bus behavior against
real services (no system bus [ENV — WO-02 §6.1]); the OD-10 Ubuntu
24.04 checkpoint (Track 2, unchanged). No application test is
represented as newly passing because documentation changed.

## F. Storage and recovery safety (FC-06)

Method: `scripts/check-durable-size.sh` (working footprint =
`/home/z/my-project` minus the platform accounting `.git`; persistent
store = `/tmp/my-project` FUSE mount; limit 950 MB, warn band 50 MB)
and `du`/`df` for the breakdown. All figures measured, not estimated
[ENV].

| Measurement | Before work | After FC1 checkpoint | After FC2 checkpoint |
|---|---|---|---|
| Working footprint | 943 MB | 945 MB | 947 MB |
| Persistent store | 935 MB | 935 MB | 935 MB |
| Rootfs (`df /`) | 9.9G total, 2.5G used, 6.9G avail | — | unchanged |

Breakdown at closeout (du): toolchains 921 MB (rustup 624 + cargo 298;
the 14 cargo/bin proxy binaries are separate 21 MB inodes — the
Stage 0-authorized hard-link optimization was **undone by the platform
recycle**, third occurrence), download ≈19 MB (all recovery artifacts),
araiguma repository ≈4 MB (`.git` 1.7 MB), skills 3.3 MB, upload
1.1 MB, tool-results 0.6 MB, workspace scripts 0.26 MB; platform
accounting `.git` 263 MB (excluded from the footprint by definition;
not store-synced).

- **The final PROMPT03-FC checkpoint** (created immediately after this
  commit, ≈1.8 MB) brings the working footprint to ≈949 MB — still
  under the 950 MB limit but deep in the WARN band. Exact final
  figures, per-artifact sizes, and checksums are recorded in
  `download/araiguma-PROMPT03-FC.sha256` and the corrected recovery
  manifest. [ENV]
- **Nothing was deleted.** No recovery artifact, checkpoint, manifest,
  log, Git object, or evidence file was removed, truncated, or
  overwritten; no history was rewritten; originals all re-verified
  (§E items 4–5). Temporary working material (verification clones)
  lived in `/tmp` — outside both budgets — and is removed at the end of
  the session. [ENV]
- **Proposed (NOT executed) headroom action for Emne's decision:**
  re-applying the Stage 0-authorized, previously validated hard-link
  optimization to `toolchains/cargo/bin` (14 identical 21 MB proxy
  binaries → hard links) would reduce the working footprint by
  ≈265 MB (921 → ≈656 MB toolchains). Caveats measured on 2026-11-20:
  the optimization did **not** propagate to the persistent store (the
  store kept its per-file accounting), and the next platform recycle
  undid it in the working tree as well — it buys working-footprint and
  redundancy headroom, not durable-store headroom, until the store
  behavior is understood. Per the 2026-11-20 incident report §5.3 this
  re-application requires Emne's explicit confirmation.
- **Suitability for the next stage:** the environment can absorb
  another documentation work order of this size (≈2 MB). It is **not**
  comfortable for a build-heavy stage: a cargo/Tauri build's `target/`
  inside the repository would count against the 950 MB working
  footprint and could exceed it; the OD-10 checkpoint will need either
  a build-output location outside `/home/z/my-project` or an explicit
  storage decision by Emne. This correction completing is NOT by
  itself a safety claim for the next stage.
- **Store reliability note (third occurrence):** the platform store's
  `araiguma/` entry is empty — the nested git repository again failed
  to sync (documented 2026-11-18, 2026-11-20, and now 2026-11-21),
  while plain-file trees (download/) sync promptly (observed: FC
  artifacts appeared in the store view immediately). The
  archive+bundle+manifest checkpoint convention after every meaningful
  commit remains the load-bearing mitigation and was followed for all
  three FC commits. [ENV]

## G. Scope compliance (explicit confirmation)

- **No source code changed** — `git diff --name-only 0dc962b..HEAD`:
  10 `.md` files + `scripts/check-prompt03-docs.sh` (documentation and
  documentation-validation tooling only; the script extension is
  strictly the specified checks). No Rust, Svelte, TypeScript, or
  Tauri file touched. [REPO]
- **No dependencies changed** — no manifest, lockfile, or
  `rust-toolchain.toml` change. [REPO]
- **No application configuration changed.** [REPO]
- **No implementation work performed** — no provider, IPC, D-Bus,
  logging/diagnostics, or domain-model code; the corrections are
  specification text and test specifications only. [REPO]
- **No Prompt 04/05/06/07 work performed; no WO-TRAY-01 work
  performed** — tray compatibility remains architectural constraints
  only. [DOC]
- **No remote configured; no push, no GitHub access, no CI, no
  publication, no credential request or use.** [REPO]
- **No adoption of RD-01..RD-30** — all remain unapproved (§C). [DOC]
- **No new product decisions** — the four-command surface and the
  corrected privacy defaults derive from OD-03/OD-09's approved scope
  plus the FC work order's explicit directives, cited as such; the
  underlying designs remain RD-referenced where they were before. [DOC]
- **The accepted Stage A application behavior is untouched** — the FC
  commits contain no change to any file outside `docs/` and
  `scripts/check-prompt03-docs.sh`. [REPO]

## H. Outstanding issues (not minimized)

1. **Storage WARN band persists** (§F): ≈949/950 MB working after the
   final checkpoint. The proposed hard-link re-application and the
   next stage's build-output strategy both await Emne's decision.
2. **Platform store does not sync the nested git repository** (three
   occurrences). Every future session must expect an empty
   `araiguma/` working tree and recover from the verified download/
   artifacts (the convention held: recovery via Stage A-03 artifacts
   worked on 2026-11-21).
3. **Prompt 03 remains pending Emne's final acceptance** — this
   correction does not claim it. The FC-05 delivered-ZIP contents
   could not be re-inspected from inside the sandbox [UNKNOWN]; the
   corrected manifests describe the artifacts that exist now.
4. **Design-level only:** every corrected contract (subscription
   lifecycle, D-Bus scoping, redaction behavior) is specification;
   nothing is implemented or runtime-validated — implementation is not
   authorized.
5. **`araiguma-FINAL.sha256` lists its tarball twice** (a pre-existing
   duplicate line in an original artifact; both entries verify OK).
   Left untouched — original artifacts are preserved as historical
   records.

*Prepared by the implementation agent, 2026-11-21 (UTC). Evidence
labels: [REPO] repository-verified, [DOC] document-verified, [ENV]
environment-measured, [TEST] executed-validation, [DESIGN]
design-level statement (no runtime claim), [UNKNOWN] not
establishable from inside the sandbox. No acceptance is claimed on
Emne's behalf.*
