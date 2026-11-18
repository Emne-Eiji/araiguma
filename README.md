# Araiguma

A Linux desktop network observability application. **Observe, do not
control.**

Araiguma helps users understand, inspect, and monitor their computer's
network environment without requiring specialist networking knowledge. It is
a strictly read-only observer: it never changes addresses, routes, DNS, VPN,
or firewall state, requires no privileges or daemons, and reports the
environment as it is — including what it *could not* observe.

> **Status:** WO-01R foundation reconstructed and validated; WO-02
> (topology, main-table route presentation, provider status) implemented
> and validated; WO-03 (document import, veth peers, full multipath,
> IPv6 procfs fallback) implemented and validated — see
> `docs/work-orders/wo-01r-wo-02-closeout.md` and
> `docs/work-orders/wo-03-closeout.md`. The original WO-01 repository was
> lost to a sandbox persistence incident on 2026-11-18 and was rebuilt
> under a fresh history — see
> `docs/incident-2026-11-18-repository-loss.md`.

## What exists today

- **Observation core** (`crates/core`): domain model for interfaces,
  addresses, and routes with explicit not-observed semantics and per-item
  provenance; provider abstraction with six-state health; merge layer
  (netlink primary, sysfs/procfs fills gaps, failures degrade without
  destroying observations); snapshot diffing.
- **rtnetlink provider** (`crates/netlink`): read-only dumps of links,
  addresses, and routes (all tables, full multipath hop sets),
  kernel-faithful translation, and a multicast change monitor with
  exponential-backoff reconnect and explicit resynchronization.
- **sysfs/procfs provider** (`crates/sysfs`): fallback observations with
  honestly-reported classification limits — `/sys/class/net`,
  `/proc/net/if_inet6`, `/proc/net/route`, and `/proc/net/ipv6_route`.
- **CLI** (`crates/cli`): `araiguma snapshot [--pretty]` and
  `araiguma watch` (NDJSON events).
- **Frontend** (`apps/frontend`): Svelte 5 + TypeScript + Vite; dark design
  system, 56 px icon rail, ≥96% central workspace, overlay details panels,
  persistent MOCK DATA badge. Views: interface overview, interactive
  topology (Cytoscape, evidence-driven), main-table routes with an explicit
  table filter and full multipath hop lists in the details, provider
  status. Synthetic fixtures render clearly-labeled by default; actual
  observation documents exported by the CLI import through the file input
  (validated at the document boundary, provenance shown, explicit
  non-live-connection notice, one click back to the labeled fixtures).
- **Topology projection** (`crates/core::topology` + `araiguma topology`):
  a pure, deterministic projection of a snapshot into nodes and edges that
  exist only with evidence — observed vs inferred relations distinguished,
  unknown relationships stay unknown, and no internet/upstream nodes are
  ever invented.

## Quick start

```sh
source scripts/env.sh
./scripts/bootstrap.sh        # toolchain, dependencies, budget check
./scripts/test-all.sh         # full validation gate
```

```sh
araiguma snapshot --pretty    # one merged observation (JSON)
araiguma watch                # live change events (NDJSON)
araiguma topology --input snapshot.json
                              # project a snapshot into a topology document
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
- Multipath routes are represented in full (every observed next hop with
  gateway, output interface, weight, per-hop flags — schema v3). The one
  remaining gap is explicit in the data: a kernel multipath attribute that
  cannot be decoded is shown as "not decodable", never flattened.
- WireGuard peer configuration is not observable unprivileged; never shown.
- MAC addresses are three-state observations (present / observed no
  hardware address / not observed, schema v2) — never conflated.

## Project authority

Product owner: **Emne** — sole authority over direction, scope, and
acceptance. The repository carries a local-only history, every commit by
`Emne <https://github.com/Emne-Eiji>`; no remote is configured. Publication
to `https://github.com/Emne-Eiji/araiguma` is deferred until the application
is transferred to Emne's local machine, independently audited, and validated
in a real environment.
