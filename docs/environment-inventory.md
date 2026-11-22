# Araiguma — environment and dependency inventory

Reconstructed after the 2026-11-18 persistence incident; records what the
development environment is, what is installed where, and the durable-storage
rules that prevent a repeat. Re-verify numbers with
`scripts/disk-usage.sh` / `scripts/check-durable-size.sh` — they drift.

## Sandbox (verified 2026-11-19)

- Debian 13.4 (trixie), kernel 5.10.134 (Alibaba Cloud Linux host kernel), x86_64
- 2 vCPU, ~4.0 GiB RAM, no swap, ~9.9 GiB overlay root filesystem
- Unprivileged user; no root, no capabilities
- Unprivileged user namespaces work (`unshare -Urn`) — the basis of all
  network-namespace integration testing
- No systemd, no system D-Bus, no NetworkManager; a **private** D-Bus
  session bus works (harness check passes)
- Node.js 24.22.3, npm 11.19.1, git 2.47.3, iproute2 6.15.0, jq, curl
- Playwright chromium-headless-shell 1255 (for @playwright/test 1.65.1)

## Persistence model (the reason the storage rules exist)

- `/home/z/my-project` is the persistent project area. At session end its
  content is synced into the PolarFS store mounted at `/tmp/my-project`.
- The boot-time restore is **capped at 1024 MB**. On 2026-11-18 the store
  held 1180 MB (Rust toolchain 971 MB + npm cache 209 MB), the restore
  stopped at the cap, and the original WO-01 repository — never actually
  written to the store — was lost. See
  `docs/incident-2026-11-18-repository-loss.md`.
- Consequence: the persistent project footprint must stay **below 950 MB**
  (`scripts/check-durable-size.sh` enforces this; it excludes only the
  sandbox accounting repository's `.git`, which the store does not sync).

### Durable-storage rules

| Item | Location | Why |
|---|---|---|
| Source, docs, fixtures, `.git` | `/home/z/my-project/araiguma` (persistent) | they are the project |
| Rust build output | `CARGO_TARGET_DIR=/tmp/araiguma-target` | regenerable |
| Cargo registry extraction | `CARGO_HOME=/tmp/araiguma-cargo` | regenerable |
| Crate archive cache | persistent `toolchains/cargo/registry/cache`, reused via a symlink inside `/tmp/araiguma-cargo` | keeps warm rebuilds offline-capable without persisting extracted trees |
| npm cache | `/tmp/araiguma-npm-cache` (`npm_config_cache`) | regenerable |
| Frontend `node_modules` | only inside `/tmp/araiguma-work` (work copy; `scripts/frontend.sh`) | 99 MB of regenerable tooling must never enter the persistent tree |
| Playwright browsers | `~/.cache/ms-playwright` (ephemeral) | regenerable, ~640 MB |

**Deviation from the work order (documented):** the work order prescribed a
gitignored `node_modules` symlink to `/tmp/araiguma-node-modules`. This
sandbox's command policy forbids creating symlinks, and persisting a symlink
into the sync layer is risky for the store. `scripts/frontend.sh` achieves
the same guarantee — `node_modules` never exists inside the repository — by
running all frontend tooling in an ephemeral work copy that mirrors the repo
layout; the generated `package-lock.json` is copied back and committed.

## Toolchain inventory

| Tool | Version | Location |
|---|---|---|
| Rust toolchain | 1.100.0 (pinned via `rust-toolchain.toml`) | `../toolchains/rustup` (persistent, ~912 MB) |
| cargo / rustc shims | 1.100.0 | `../toolchains/cargo/bin` |
| rustfmt / clippy | 1.11.0 / 0.1.100 | same |
| svelte | 5.58.0 | committed lockfile |
| cytoscape | 3.35.0 | committed lockfile |
| @tauri-apps/api | 2.12.5 | committed lockfile |
| vite | 8.3.8 | committed lockfile |
| @sveltejs/vite-plugin-svelte | 7.3.1 | committed lockfile |
| typescript | 5.9.3 | committed lockfile |
| svelte-check | 4.7.8 | committed lockfile |
| vitest | 5.1.1 | committed lockfile |
| @playwright/test | 1.65.1 | committed lockfile |

Rust crate versions are exact-pinned in the workspace manifest and
`Cargo.lock`. The earlier WO-01R/WO-03 dependency set matched the crate
archive cache that survived the incident; the Tauri dependency tree added
by WO-05 must be revalidated separately before claiming a fully offline
warm-cache rebuild.

## npm security advisory status (re-evaluated 2026-11-22)

- `npm audit` over the complete frontend dependency tree, including
  development tooling: **0 vulnerabilities**.
- Runtime dependencies are Svelte, Cytoscape, and the Tauri JavaScript API.
  The remaining packages are build/test tooling.
- The 2026-11-19 `picomatch` development-tooling advisory was resolved by
  later pinned transitive releases; the current lockfile is clean.

## Build limitations in this environment

- The earlier Debian sandbox had no WebKitGTK development packages, so the
  Tauri shell was deferred there. On the current Pop!_OS workstation Emne
  installed the four Tauri build prerequisites; the native WO-05 Stage 1
  application has since been built and exercised successfully.
- No systemd / NetworkManager / systemd-resolved ⇒ no real service
  integration testing (mocks and a private-session D-Bus capability check
  only). Nothing in this repository claims such testing.
- Desktop notifications, system tray, `.deb` packaging: not attempted
  (outside current scope).

## Storage snapshot (2026-11-19, after M0 cleanup)

- Store: 919 MB (was 1180 MB before the authorized removal of the npm
  cache, rustup-init, and the store's registry/src copy).
- Persistent project area: ~924 MB total, of which the Araiguma repository
  is a few MB.
- Root filesystem headroom: several GB (build trees live in /tmp).
