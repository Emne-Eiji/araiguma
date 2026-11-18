# Araiguma

A Linux desktop network observability application. **Observe, do not
control.**

Araiguma helps users understand, inspect, and monitor their computer's
network environment without requiring specialist networking knowledge. It is
a strictly read-only observer: it never changes addresses, routes, DNS, VPN,
or firewall state, requires no privileges or daemons, and reports the
environment as it is — including what it *could not* observe.

> **Status:** WO-01R foundation reconstructed and validated (see below).
> Network topology, main-table route presentation, and provider status
> follow under WO-02. The original WO-01 repository was lost to a sandbox
> persistence incident on 2026-11-18 and was rebuilt under a fresh history —
> see `docs/incident-2026-11-18-repository-loss.md`.

## What exists today

- **Observation core** (`crates/core`): domain model for interfaces,
  addresses, and routes with explicit not-observed semantics and per-item
  provenance; provider abstraction with six-state health; merge layer
  (netlink primary, sysfs/procfs fills gaps, failures degrade without
  destroying observations); snapshot diffing.
- **rtnetlink provider** (`crates/netlink`): read-only dumps of links,
  addresses, and routes (all tables), kernel-faithful translation, and a
  multicast change monitor with exponential-backoff reconnect and explicit
  resynchronization.
- **sysfs/procfs provider** (`crates/sysfs`): fallback observations with
  honestly-reported classification limits.
- **CLI** (`crates/cli`): `araiguma snapshot [--pretty]` and
  `araiguma watch` (NDJSON events).
- **Frontend** (`apps/frontend`): Svelte 5 + TypeScript + Vite; dark design
  system, 56 px icon rail, ≥96% central workspace, overlay details panel,
  persistent MOCK DATA badge. Today it renders clearly-labeled synthetic
  fixtures that are schema-locked against the Rust model from both sides.

## Quick start

```sh
source scripts/env.sh
./scripts/bootstrap.sh        # toolchain, dependencies, budget check
./scripts/test-all.sh         # full validation gate
```

```sh
araiguma snapshot --pretty    # one merged observation (JSON)
araiguma watch                # live change events (NDJSON)
```

## Principles

1. **Observe, do not control.** No network mutation anywhere, ever.
2. **Never fabricate.** `None` means "not observed", never "no" — the one
   documented exception is route gateways, where the kernel itself encodes
   "no next hop". Unknown stays unknown; caveats are carried as notes.
3. **The backend is authoritative.** The frontend renders documents; it
   never discovers or reconstructs network state.
4. **Provider failures degrade; they never destroy.** Every observation
   keeps its provenance, and failures are visible, not hidden.
5. **Mock data is labeled.** Synthetic data always shows a MOCK DATA badge.

## Documentation

- `docs/architecture.md` — pipeline, model semantics, testing architecture
- `docs/discovery-notes.md` — what each source provides; kernel behaviors
- `docs/development.md` — setup, commands, model-change procedure, git flow
- `docs/environment-inventory.md` — toolchain, storage budget, advisories
- `docs/incident-2026-11-18-repository-loss.md` — why this history is fresh
- `docs/work-orders/` — the original WO-01 requirements and the WO-01R
  authorization, preserved verbatim

## Known limitations

- The desktop shell (Tauri) cannot run in this sandbox (no WebKitGTK, no
  root); the frontend targets a browser for now.
- NetworkManager / systemd-resolved integration is not implemented; the
  sandbox has no system bus. Nothing claims such testing.
- Multipath routes show only their first next hop (noted per route).
- WireGuard peer configuration is not observable unprivileged; never shown.

## Project authority

Product owner: **Emne** — sole authority over direction, scope, and
acceptance. The repository carries a local-only history, every commit by
`Emne <https://github.com/Emne-Eiji>`; no remote is configured. Publication
to `https://github.com/Emne-Eiji/araiguma` is deferred until the application
is transferred to Emne's local machine, independently audited, and validated
in a real environment.
