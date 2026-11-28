# WO-09 — Identity, repository presentation, and documentation refresh

**Status:** Implementation complete and validated; **uncommitted** pending Emne's
review and explicit publication authorization.
**Authority:** Emne (Product Owner). **Implementer:** Tidy.
**Branch:** `main`. **Baseline commit:** `b6da3d661740113c8025d3915832775b7cbcac0f`.
**Date:** 2026-11-28.

Evidence tags: **[EXEC]** executed in this work order and observed;
**[REPO]** observed repository/remote state; **[HIST]** historical result from a
prior work order, not re-executed here. No estimates are used as evidence.

## 1. Recovery branch deletion

Before deletion [REPO]: working tree clean; current branch `main`; local `main`
and `origin/main` both `b6da3d6`; `recovery/araiguma-main` both local and remote
`b171d30`; `main...recovery` was `1 0` (recovery fully contained in `main`);
WO-08 commit `b171d30` was an ancestor of `origin/main`.

Deletion [EXEC]:

```text
git branch -d recovery/araiguma-main
Deleted branch recovery/araiguma-main (was b171d30).

git push origin --delete recovery/araiguma-main
 - [deleted]         recovery/araiguma-main

git fetch origin --prune
(no remaining output)
```

After deletion [EXEC]: local branches are `main` (`b6da3d6`, tracking
`origin/main`) and `archive/main-pre-recovery-2026-11-21` (`08e58f3`). Remote
heads are exactly `main` (`b6da3d6`) and the archive (`08e58f3`). No force-push
and no history rewrite were used.

## 2. Archive branch preserved

The archive branch was never modified or deleted [EXEC]:

- local `archive/main-pre-recovery-2026-11-21` = `08e58f34d3ce33e74fb5d921dd74277022852448`
- `refs/heads/archive/main-pre-recovery-2026-11-21` on `origin` = the same SHA.

## 3. Baseline

[REPO] Repository root:
`/home/banana/Constellation/Idea Dev/opencode/Araiguma/araiguma`.
Branch `main`, HEAD `b6da3d661740113c8025d3915832775b7cbcac0f`, tree clean.
`origin` = `https://github.com/Emne-Eiji/araiguma.git`; default branch `main`;
repository public.

- README SHA-1: `6b95d557262453ab377491d5e59f8a1fdbf20a09`.
- GitHub description: empty.
- No `LICENSE`/`COPYING` file.
- Old artwork: `assets/icon/araiguma-icon.svg` (1024 viewBox, 5,633 B);
  `assets/icon/araiguma-512.png` and `apps/desktop/icons/icon.png` (512×512,
  35,860 B); `icon_32x32.png` (32×32, 1,377 B); `icon.ico` (5 frames:
  16/24/32/48/128, 22,255 B); `icon.svg` (5,633 B).
- Old screenshots: six JPEGs at 1062×910 (overview, topology, interfaces,
  routes, providers, documents) plus seven WO-08 PNGs at 1440×~1250 under
  `docs/design/wo-08-screenshots/`.

## 4. Visual directions explored

Three materially different directions were rendered as SVG and inspected at
actual size and at 5–14× nearest-neighbour on light (`#f5f6f8`) and dark
(`#0b0c0f`) backgrounds.

| Direction | Description | Finding |
|---|---|---|
| **A — face-only mark** | Compact raccoon face, no network motif | Weak at 16 px and on dark; carries no observation meaning |
| **B — mascot + network triangle** | Raccoon head inside a three-node triangle | Cluttered at small sizes; the mascot becomes subordinate to the decoration |
| **C — observation node** | Raccoon face whose head is a network node, with observed-link stubs and green end nodes | Strongest silhouette and meaning; refined in three variants |

Within direction C, three motifs were tested: **C2** three stubs (read as a
crosshair), **C3** a dashed orbit ring (noisy halo at small sizes), and **C6**
one link through the head (observed node → observer → observed node). **C6 was
selected.**

Why C6: it keeps a compact, balanced face; the single left-to-right link reads
as observation rather than control (no arrows, locks, shields, or outward
motion); the green end nodes carry the evidence accent; the cyan outline and
light muzzle hold contrast on both light and dark surfaces; and the silhouette
stays intact at 16 and 24 px. The same geometry is reused in the navigation-rail
mark, favicon, desktop icons, and README mark.

Accepted trade-off: at 16/24 px the tiny variant deliberately drops the brow
patches, in-mask network stubs, and fine mask detail to preserve the head,
ears, eyes, and muzzle. The standard tile retains the full mark for 32 px and
above.

## 5. Final artwork inventory

| File | Format | Dimensions / frames | Bytes |
|---|---|---|---|
| `assets/icon/araiguma-icon.svg` | SVG (bare mark) | 64 viewBox | 2,087 |
| `assets/icon/araiguma-app-icon.svg` | SVG (standard tile) | 64 viewBox, 80% mark | 2,048 |
| `assets/icon/araiguma-app-icon-tiny.svg` | SVG (tiny tile) | 64 viewBox, simplified | 1,222 |
| `assets/icon/araiguma-16.png` | PNG RGBA | 16×16 | 810 |
| `assets/icon/araiguma-24.png` | PNG RGBA | 24×24 | 1,422 |
| `assets/icon/araiguma-32.png` | PNG RGBA | 32×32 | 1,816 |
| `assets/icon/araiguma-48.png` | PNG RGBA | 48×48 | 3,087 |
| `assets/icon/araiguma-64.png` | PNG RGBA | 64×64 | 4,366 |
| `assets/icon/araiguma-128.png` | PNG RGBA | 128×128 | 9,315 |
| `assets/icon/araiguma-256.png` | PNG RGBA | 256×256 | 19,283 |
| `assets/icon/araiguma-512.png` | PNG RGBA | 512×512 | 39,856 |
| `apps/desktop/icons/icon.svg` | SVG | standard tile | 2,048 |
| `apps/desktop/icons/icon.png` | PNG RGBA | 512×512 | 39,856 |
| `apps/desktop/icons/icon_32x32.png` | PNG RGBA | 32×32 | 1,816 |
| `apps/desktop/icons/icon.ico` | ICO | 16,24,32,48,64,128,256 | 40,217 |
| `apps/frontend/public/araiguma-icon.svg` | SVG favicon | tiny tile | 1,222 |
| `assets/architecture/observation-pipeline.svg` | SVG diagram | 760×500 | 5,362 |

Raster policy: tiny master for 16/24 and the favicon; standard master for
32/48/64/128/256/512. PNGs are generated from the SVG masters with CairoSVG; the
ICO is assembled from PNG frames. The compact-tile master used during exploration
was removed after the tiny master replaced it, so no duplicate icon source
remains. The six stale JPEG screenshots were deleted.

## 6. Small-size inspection

[EXEC] Installed PNGs were normalized to 256×256 with nearest-neighbour and
inspected on light and dark surfaces:

- **16 px** — tiny master: rounded tile, cyan head outline, ears, two eyes, and
  muzzle remain distinguishable. The first compact-tile attempt was mushy at
  16 px and was replaced; this was the only artwork rejected during final
  inspection.
- **24 px** — tiny master reads clearly as a masked raccoon face; eye separation
  and ears are stable.
- **32 px** — standard master: full brow/mask detail returns; silhouette clear.
- **48 px / 64 px** — standard master: clean at actual size and when enlarged.
- **ICO** — seven frames verified by parsing the file: 16, 24, 32, 48, 64, 128,
  256. Frame 16/24 use the tiny tile; 32+ use the standard tile.
- **Light/dark** — the dark tile supplies its own surface; on GitHub dark the
  border defines the tile, and the cyan/green/white features remain visible.

## 7. Screenshots

[EXEC] Captured from the production frontend build served at
`http://127.0.0.1:5198/?araiguma-test-hooks`, with Playwright 1.65.1 Chromium,
device pixel ratio 1, viewport width 1440, and content-fitted heights. The
application used the checked-in synthetic demonstration scenarios — the
"Synthetic workstation" scenario for seven views and the "Degraded
observation" scenario for the Providers view, which is why that panel shows a
failed primary provider and an honest sysfs/procfs fallback. Every view
identifies the data as `MOCK DATA` in the status bar (and in the header badge
where the details panel does not cover it). No live host identifiers are
present. The old JPEGs and the WO-08 PNGs were not reused.

| File | View | Dimensions | Bytes |
|---|---|---|---|
| `assets/screenshots/overview.webp` | Overview | 1440×1253 | 66,518 |
| `assets/screenshots/topology.webp` | Topology | 1440×1031 | 125,418 |
| `assets/screenshots/interfaces.webp` | Interfaces + details signal path | 1440×1250 | 71,148 |
| `assets/screenshots/routes.webp` | Routes & Gateways | 1440×900 | 27,430 |
| `assets/screenshots/dns.webp` | DNS Configuration | 1440×954 | 39,584 |
| `assets/screenshots/connections.webp` | Connections | 1440×900 | 34,710 |
| `assets/screenshots/providers.webp` | Providers & Observation Health | 1440×900 | 52,340 |
| `assets/screenshots/documents.webp` | Documents & Import | 1440×923 | 54,116 |

Source PNGs totalled 1,195,448 B; published lossless WebPs total 471,264 B
(39.4%). The WebP conversion is lossless, so no visual quality was traded for
size. A 480×2506 narrow-window capture was also made and inspected but is not
published: it is not a required view and would make the README gallery
unreasonably tall. The original PNG captures remain in the session evidence
directory `/tmp/opencode/wo-09/captures/` and are not committed.

## 8. GitHub repository description

[EXEC] Before: empty string. After:

> A read-only Linux network observability desktop app for interfaces, routes,
> DNS, and connections, with evidence-based topology and explicit unknown
> states.

Changed with `gh repo edit Emne-Eiji/araiguma --description ...` and re-read with
`gh repo view`; the new value is confirmed on the public repository. The wording
claims only read-only observation, the observed areas, evidence-based topology,
and explicit unknown states; it does not claim control, packet inspection,
traffic analysis, a release, or production readiness. No GitHub release was
created.

## 9. README changes and factual corrections

The README was rewritten around the integrated WO-08 implementation:

- Replaced the giant-head mascot presentation with an 88 px application mark, a
  short positioning statement, and one hero screenshot; the gallery uses the
  remaining seven views.
- Replaced the six-section description with the current eight sections:
  Overview, Topology, Interfaces, Routes & Gateways, DNS Configuration,
  Connections, Providers & Observation Health, Documents & Import.
- Documented rtnetlink, sysfs/procfs, NetworkManager, and systemd-resolved as
  implemented providers (NetworkManager and resolved as optional,
  capability-dependent), and described the shared engine, CLI, Tauri 2 desktop,
  and Svelte 5 frontend.
- Added the evidence model (observed, observed empty, not observed, unsupported,
  unavailable, failed, permission denied, partial/degraded, inferred) and the
  data modes (LIVE, STALE, UNAVAILABLE, IMPORTED OBSERVATION, MOCK DATA), with
  explicit limits: configuration is not resolution, interface state is not
  internet access, connection objects are not traffic.
- Replaced the stale two-provider architecture text with the schema-v4
  four-provider diagram and the merge/ownership rules.
- Removed the old six-section/Cytoscape/old-test-count claims, the
  `recovery/araiguma-main` governance wording, and the roadmap entries that are
  now implemented; demoted the repository-loss incident to a historical link.
- Recorded current validation evidence (2026-11-28), including the successful
  native desktop release build, and clearly marked the full `test-all.sh` gate
  and the live host check as not run for this refresh.
- Added a current documentation index and a governance/license note: `main` is
  canonical, the archive branch is historical only, the recovery branch is
  retired, no release is published, and no license has been declared.

Also corrected `docs/development.md` §Git workflow to `main` and updated the
architecture diagram to schema v4, four providers, the shared engine, the CLI,
the Tauri/Svelte frontend, and eight sections.

## 10. Files changed

Modified: `README.md`, `docs/development.md`, `apps/frontend/index.html`,
`apps/frontend/src/components/NavRail.svelte`,
`assets/architecture/observation-pipeline.svg`, `assets/icon/araiguma-icon.svg`,
`assets/icon/araiguma-512.png`, and the four desktop icon files.

Deleted: six stale JPEG screenshots.

Added: two SVG icon masters (`araiguma-app-icon.svg` and the purpose-built
`araiguma-app-icon-tiny.svg`), eight generated PNG sizes, the favicon, eight
WebP screenshots, and this report. The bare `araiguma-icon.svg` master was
replaced in place.

No Rust crate, `Cargo.toml`, `Cargo.lock`, data schema, provider, dependency, or
application behavior was changed. `git diff --name-only` contains no `crates/`,
`apps/desktop/src/`, `Cargo*`, or package changes. Frontend edits are limited to
the favicon link and the identity mark in the navigation rail.

## 11. Validation results

| Check | Result | Evidence |
|---|---|---|
| `cargo fmt --all -- --check` | PASS | [EXEC] |
| `cargo clippy --workspace --all-targets -- -D warnings` | PASS | [EXEC] |
| `cargo test --workspace` | **345 passed, 0 failed** | [EXEC] includes netns and desktop crate tests |
| `scripts/frontend.sh check` | **0 errors, 0 warnings** | [EXEC] |
| `scripts/frontend.sh test` | **14 files; 217 passed + 1 skipped (218)** | [EXEC] |
| `scripts/frontend.sh e2e` | **26/26 passed**, production build succeeds | [EXEC] |
| `cargo check -p araiguma-desktop --features custom-protocol` | PASS | [EXEC] against a temporary production `dist`, removed afterward |
| `cargo build --release -p araiguma-desktop --features custom-protocol` | **PASS** (2m34s) | [EXEC] production frontend bundle; temporary `dist` removed afterward |
| README local links/images | **29/29 resolve** | [EXEC] scripted extraction |
| Stale-content scan | only intentional “retired branch” mentions | [EXEC] |
| Icon dimensions / ICO frames | 8 PNG sizes correct; ICO 7 frames 16–256 | [EXEC] |
| `scripts/check-prompt03-docs.sh` | **PASS** (15 checks) | [EXEC] |
| `git diff --check` | clean | [EXEC] |
| `apps/frontend/package-lock.json` | unchanged after tooling runs | [EXEC] |

Not run for this refresh: `./scripts/test-all.sh` (npm audit, D-Bus harness,
live observation, storage budget, fresh-clone) and `./scripts/live-check.sh`.
The native desktop release build was run successfully, but the packaged
application was not installed or launched. Historical WO-08 results are not
presented as current evidence.

## 12. Independent review

The requested reviewer was **GLM 5.3 Max**, which could not be reached through
any available endpoint. The implementer is DeepSeek V4.1 Flash; the substitute
reviewer that actually ran is MiMo v2.5 (Token Harbor, free), a different model
family, so the review is independent of the implementer even though it is not
the requested Max tier.

Review attempts and their exact outcomes:

| # | Model (variant) | Outcome returned |
|---|---|---|
| 1 | `opencode/glm-5.3#max` | Insufficient balance (OpenCode Models pay-as-you-go credits) |
| 2 | `tokenharbor/glm-5.3#high` | No active plan on the account and wallet balance cannot cover the model |
| 3 | `nvidia/z-ai/glm-5.3-flash#thinking` | `ECONNRESET` (socket closed unexpectedly) — attempted twice, failed both times |
| 4 | `kilo/moonshotai/kimi-k3.1-pro` | Add credits, or switch to a free model |
| 5 | `orcarouter/z-ai/glm-5.3-flash-free#high` | Free-model capacity limited; retry later or add credits |

Since GLM 5.3 Max was not reachable and no funded substitute was available, the
GLM-5.3-Flash fallback was also attempted and failed (network/capacity above),
so the independent review was carried out with **MiMo v2.5** on a zero-cost
Token Harbor endpoint (`tokenharbor/mimo-v2.5:free`) — a different model family
from the DeepSeek implementer. The reviewer's verdict and dispositions follow.

### Substitute findings and dispositions

The substitute reviewer (MiMo v2.5, Token Harbor free; independent of the
DeepSeek implementer) returned **CHANGES REQUIRED** with one MATERIAL finding
and two MINOR notes; all were addressed.

| # | Class | Finding | Disposition |
|---|---|---|---|
| R1 | **MATERIAL** | The README claimed *every* screenshot used the "Synthetic workstation" scenario, but `assets/screenshots/providers.webp` shows the **"Degraded observation"** scenario (status bar), which is why it depicts a failed primary provider. Same claim in this report §7. | **Fixed.** README hero caption, Screenshots intro, the Providers caption, and report §7 now name both scenarios explicitly. Re-verified by reading the status bar of all eight captures: seven use "Synthetic workstation", only Providers uses "Degraded observation". |
| R2 | MINOR | README listed "Partial / degraded" in the item **outcome** table, but `degraded`/`partial` are **provider states**, not per-item outcomes (`apps/frontend/src/lib/observability.ts:9-11` vs the six provider states). | **Fixed.** Removed the row from the outcome table; the provider-states sentence now covers `degraded` (limited fidelity) and `partial` (incomplete run). |
| R3 | MINOR | `assets/icon/araiguma-icon.svg` (bare mark) and `apps/frontend/public/araiguma-icon.svg` (tiny tile) are different artwork sharing a filename. | **Accepted, documented.** The bare mark is the canonical repo mark; the public copy must be named `araiguma-icon.svg` for the `/araiguma-icon.svg` URL. Different directories and purposes; recorded as a trade-off. |

The reviewer explicitly confirmed as passing: mascot restraint (head ≈41/64 of
the viewBox, no control cues), observation reading of the node/link motif,
16/24 legibility of the tiny tile, 32+ legibility of the standard tile, the
seven-frame ICO, icon/Tauri/frontend consistency, README facts (eight sections
and labels, four providers and priorities, schema v4, evidence vocabulary,
five data modes, CLI commands, dependencies, test counts), all-local-link
integrity, lossless 1440 px WebP screenshots with MOCK DATA labeling and no
live identifiers, the schema-v4 architecture diagram, GitHub-description
accuracy, and full scope compliance (no Cargo/package/lock/schema/provider/UI
changes; stop condition respected). Recorded AESTHETIC trade-offs (the network
stubs fall below ~32 px; the dark tile's border is subtle on dark backgrounds)
were not treated as failures; both are already documented in §4 and §6.

### Second, corroborating review

A second independent free-model run (MiMo v2.6, Token Harbor free) was executed
in parallel and returned **APPROVE WITH MINOR FIXES** with **no MATERIAL
findings**. Its single MINOR item was report-only: §9 had listed the native
desktop release build among the checks "not run", contradicting §11 and §13
which record that build as PASS. §9 was corrected (the report now states the
build ran; only `test-all.sh` and the live check were not run). The second
reviewer independently re-confirmed the identity, icon/ICO, README facts
(eight sections, four providers, schema v4, evidence vocabulary, five data
modes, CLI, prerequisites, license/governance wording), screenshot scenario
labels, architecture diagram, scope compliance, and the GitHub description;
its two aesthetic notes (link stubs below ~32 px; shared favicon filename) are
the already-documented trade-offs A1/A2.

**Combined verdict:** the only MATERIAL finding across both reviews (the
screenshot-scenario mislabel, R1) is fixed; the remaining items are MINOR and
were fixed or accepted-and-documented. No material issue remains in any WO-09
deliverable.

## 13. Remaining limitations and unresolved issues

- **WO-06F** (real Ubuntu 24.04 host checkpoint) remains outstanding; live host
  compatibility is not claimed.
- Typed IPC errors and the full Document-04 subscription lifecycle remain
  deferred.
- Packaging, tray, and notifications are not implemented; no release exists.
- **No license is declared.** This needs an explicit Emne decision; WO-09 does
  not authorize choosing one.
- Screenshots use labeled synthetic data, not a live host. This is deliberate:
  live captures risk exposing real identifiers and would vary with the machine.
- The full `test-all.sh` gate and live check were not executed for this
  documentation refresh. The native desktop release build succeeded, but the
  packaged application was not installed or launched.
- The 16/24 px tiny icon drops fine facial and network detail; this is an
  intentional legibility trade-off, recorded in §4 and §6.
- The GitHub description is updated externally; the repository implementation
  remains uncommitted and unpublished pending Emne's authorization.

## 14. Final Git state

[EXEC] HEAD remains `b6da3d661740113c8025d3915832775b7cbcac0f`; no WO-09 commit
exists. The working tree contains the changes listed in §10 (modified, deleted,
and untracked files) and this report. `main` still points at the baseline commit;
nothing was committed, pushed, merged, or published. The archive branch remains
at `08e58f3`; the recovery branch remains deleted.

## 15. Scope statement

No application functionality, provider behavior, data schema, evidence
semantics, architecture, dependency set, packaging, tray, telemetry, or UI
section/navigation was changed. The only frontend edits reference the new
identity assets (favicon and navigation-rail mark). The architecture SVG and
README/development-guide edits are documentation/presentation changes. The work
order's stop condition is respected.