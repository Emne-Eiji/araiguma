# Araiguma

A Linux desktop network observability application. **Observe, do not control.**

Araiguma is a read-only observer of the local network environment. It never
modifies network configuration, and it never fabricates observations: a field
that could not be observed is reported as not observed, never guessed.

> **Status: reconstruction in progress (WO-01R).** The original WO-01
> repository was lost to a sandbox persistence incident on 2026-11-18 and is
> being reconstructed under a fresh Git history. See
> `docs/incident-2026-11-18-repository-loss.md` (once the documentation
> milestone lands) for the incident record.

## Current contents

This commit contains the workspace scaffolding and the durable-storage
tooling only: `scripts/env.sh` (environment), `scripts/bootstrap.sh`
(reproducible setup), `scripts/check-durable-size.sh` (storage budget),
`scripts/milestone.sh` (recovery artifacts), `scripts/disk-usage.sh`, and
`scripts/check-dbus-harness.sh` (capability probe). The observation model,
discovery providers, CLI, and frontend are reconstructed in the following
commits; this README is updated as they land.

## Development environment

```sh
source scripts/env.sh       # every shell: toolchain + ephemeral build paths
./scripts/bootstrap.sh      # idempotent: toolchain, deps, playwright, budget
./scripts/check-durable-size.sh
```

Rust toolchain is pinned by `rust-toolchain.toml` (1.100.0). Build output
(`CARGO_TARGET_DIR=/tmp/araiguma-target`), Cargo registry extraction, the npm
cache, and `node_modules` all live outside the persistent tree, so the
persistent footprint stays small enough for the sandbox persistence layer.

## Product authority

Product owner: Emne. The repository carries a local-only Git history with the
identity `Emne <https://github.com/Emne-Eiji>`; no remote is configured.
Publication to `https://github.com/Emne-Eiji/araiguma` is deferred until the
application is transferred to Emne's local machine and independently audited.
