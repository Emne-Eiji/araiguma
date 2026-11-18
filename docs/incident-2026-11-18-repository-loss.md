# Incident record — repository loss on 2026-11-18

Status: closed. The original WO-01 repository is unrecoverable; this file
records what happened, the evidence, and what changed so it cannot happen
the same way again.

## What happened

Between the end of the WO-01 execution session (~12:33 UTC, 2026-11-18) and
the start of the next session (15:29 UTC), the Araiguma repository at
`/home/z/my-project/araiguma` — source code, tests, documentation, and its
complete Git history — was lost. The repository does not exist anywhere in
the sandbox afterwards; forensic search (recursive listing of `/home/z`,
`/tmp`, the store mount, the parent accounting repository, and a
filesystem-wide search for `araiguma*`) found no copy.

## Root cause (evidence-backed)

- The sandbox's persistence layer syncs `/home/z/my-project` into a PolarFS
  store mounted at `/tmp/my-project` at session end. The boot-time restore
  from that store is **capped at 1024 MB**.
- At the WO-01 session end the store held **1180 MB**: the Rust toolchain
  installed during WO-01 (971 MB / 4067 entries) plus the npm cache
  (209 MB). The store's own manifest (4517 entries, written 12:34 UTC)
  contains exactly one `araiguma` entry: an **empty directory** (mtime
  12:25:32). Not a single Araiguma file was ever written to the store.
- The next boot's restore log: `copied 800/4513 file(s), 1143MB; stopped:
  total cap 1024MB reached`.
- Conclusion (high confidence): the sync's capacity budget was consumed by
  the toolchain and cache; the repository — which with `target/`,
  `node_modules/`, and `.git` would have added roughly another gigabyte —
  never got in. The cap is the root cause either way.

Contributing factors: WO-01 had stored the toolchain and npm cache inside
the persisted directory (believed safe at the time; the cap was not
discoverable from inside the sandbox), and the WO-01 closeout did not
export a recovery artifact (no git bundle in `download/`), so the loss was
total.

## What was lost (permanently)

- All WO-01 source code (four crates + frontend), tests, `Cargo.lock`,
  `package-lock.json`, and all documentation.
- The complete Git history: **11 commits** per the last handoff summary
  (`ff779c7`…`1896426`). A separate report line had mentioned a count of
  10; with the repository gone, the 11-vs-10 discrepancy can no longer be
  reconciled against data and is recorded here as **permanently
  unresolved**. No reconciliation is invented.

## What survived

- Rust toolchain 1.100.0 (verified healthy).
- The exact WO-01 crate dependency set in the local registry archive cache.
- The synthetic sysfs/procfs fixture generator script and the frontend
  design-verification script (recovered from the store).
- The complete WO-01 work-order text (1101 lines, via `upload/`).
- WO-00 deliverables and the incident report itself.

## Response (WO-01R, authorized 2026-11-19)

1. **Authorized cleanup (M0)** — removed from both the working directory
   and the store: the npm cache (210 MB working / 209 MB store),
   `rustup-init` (21 MB), and the store's extracted `registry/src`
   (33 MB). Store: 1180 MB → **919 MB**, back under the 1024 MB cap.
   The crate archive cache (5.1 MB, 44 crates) was verified intact.
2. **Durable-storage discipline** — build output, registry extraction, npm
   cache, and frontend `node_modules` all moved out of the persistent tree
   (see `docs/environment-inventory.md`); an automated sub-950 MB budget
   check is part of the validation gate. One documented deviation from the
   prescribed mechanism: this sandbox forbids creating symlinks from the
   command layer, so the prescribed `node_modules` symlink was replaced by
   an ephemeral work-copy runner (`scripts/frontend.sh`) with the same
   guarantee.
3. **Recovery artifacts at milestones** — `scripts/milestone.sh` writes a
   git bundle, a source archive, and a SHA-256 manifest into the persistent
   `download/` directory at every meaningful milestone, so a future
   persistence failure can never again be a total loss.
4. **Reconstruction (WO-01R)** — the foundation was rebuilt to the original
   WO-01 requirements under a fresh Git history, every commit by
   `Emne <https://github.com/Emne-Eiji>`, starting from
   `chore: scaffold workspace and durable-storage layout`. The code is a
   reimplementation: equivalent architecture and behavior, different text.

## Corrective actions for the future

- Never store regenerable bulk (build output, caches, extracted sources,
  `node_modules`) inside the persistent project area.
- Run `scripts/check-durable-size.sh` before ending any work session.
- Write milestone recovery artifacts before stopping.
- Re-verify store health at the start of each session
  (`du -sm /tmp/my-project`).

The original WO-02 premise that this history could be rewritten to the
approved identity is void; WO-02 proceeds on the reconstructed history,
which carries the approved identity from its first commit.
