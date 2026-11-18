# Araiguma — development guide

## One-time setup

```sh
source scripts/env.sh   # every shell: toolchain + ephemeral build paths
./scripts/bootstrap.sh  # idempotent: toolchain, cargo fetch, frontend work copy
```

`bootstrap.sh` re-creates all ephemeral state after a sandbox recycle
(CARGO_TARGET_DIR, CARGO_HOME with the persistent crate-cache link, npm
cache, frontend work copy, Playwright browsers on demand).

## Everyday commands

| Task | Command |
|---|---|
| Format / lint / test Rust | `cargo fmt --all` · `cargo clippy --workspace --all-targets -- -D warnings` · `cargo test --workspace` |
| Build the CLI | `cargo build -p araiguma-cli` (`$CARGO_TARGET_DIR/debug/araiguma`) |
| Observe | `araiguma snapshot [--pretty]` · `araiguma watch` |
| Frontend type check | `scripts/frontend.sh check` |
| Frontend unit tests | `scripts/frontend.sh test` |
| Frontend build / e2e | `scripts/frontend.sh build` · `scripts/frontend.sh e2e` |
| Frontend dev server | `scripts/frontend.sh prepare` then run `npm run dev` inside `/tmp/araiguma-work/apps/frontend` (port 5199) |
| Live cross-validation | `scripts/live-check.sh` |
| Everything | `scripts/test-all.sh` |
| Storage report | `scripts/disk-usage.sh` / `scripts/check-durable-size.sh` |

Frontend tooling runs in an ephemeral work copy (`/tmp/araiguma-work`) that
mirrors the repository layout; `node_modules` never exists inside the
repository. Edit sources in the repository — `frontend.sh` refreshes the
copy on every invocation. The generated `package-lock.json` is copied back
and must stay committed.

## Changing the data model (type-sync contract)

The Rust model (`crates/core/src/model.rs`) is authoritative; the TypeScript
mirror is `apps/frontend/src/types.ts`. They are locked together by
`fixtures/snapshots/*.json`:

1. Rust tests parse every fixture with serde (`crates/core/tests/
   fixtures_roundtrip.rs`).
2. Frontend tests parse the same files through the validating loader
   (`apps/frontend/tests/snapshot.test.ts`).
3. Any model change therefore requires updating the fixtures in the same
   commit, and both suites fail loudly on drift.

Procedure: change the Rust model → update fixtures (and sysfs trees if
needed) → update `types.ts` to match the new serde shape → run
`cargo test -p araiguma-core` and `scripts/frontend.sh test` until both are
green. Keep serde renames in mind (camelCase fields, lowercase enums,
`Other(u8)` variants serialize as `{"other": n}`).

### Schema versions

Documents carry `schemaVersion`. Writers emit the current version (see
`SNAPSHOT_SCHEMA_VERSION` / `TOPOLOGY_SCHEMA_VERSION` in crates/core);
readers accept older versions only where they are unambiguous — a v1
snapshot is accepted only if every `macAddress` is a non-null string,
because v1 `null` could mean either "no hardware address" or "not
observed". When bumping a version: update the constant, document the
change in its doc comment and in `docs/architecture.md`, and update
fixtures + validators in the same commit.

### Frontend document validation

All document loading goes through the validated boundaries in
`apps/frontend/src/lib/` (`snapshot.ts`, `topology.ts` on shared
primitives in `validate.ts`). When the model changes, extend the
validators and the malformed-document tests (`tests/validation.test.ts`)
in the same commit — these loaders are the boundary WO-03's import
workflow relies on.

Regenerate sysfs/procfs fixture trees with `scripts/gen-sysfs-fixtures.sh`;
`scripts/test-all.sh` verifies regeneration is stable (no uncommitted
drift).

## Git workflow

- Single `main` branch, local only. **No remote is configured** (GitHub
  publication deferred until transfer to Emne's machine and independent
  audit).
- Repository-local identity only:

  ```sh
  git config user.name  "Emne"
  git config user.email "https://github.com/Emne-Eiji"
  ```

- Milestone recovery artifacts (bundle + archive + SHA-256 manifest) are
  written outside the repository into `../download/` by
  `scripts/milestone.sh <NAME>` — run it at every meaningful milestone; it
  refuses to run on a dirty tree or a wrong identity.

## Durable-storage rules (why things live where they do)

See `docs/environment-inventory.md`. The short version: the persistent
project area must stay below 950 MB or the sandbox's persistence layer may
drop files again. Build output, dependency extraction, caches, and
`node_modules` all live under `/tmp`; nothing regenerable is committed; the
size check is part of the gate.

## Troubleshooting

- **`cargo` not found / wrong toolchain**: `source scripts/env.sh` sets
  RUSTUP_HOME/CARGO_HOME/PATH; the toolchain lives in `../toolchains`.
- **Frontend tests can't find fixtures**: they import
  `../../../../fixtures/...`; run them via `scripts/frontend.sh` (the work
  copy preserves the layout).
- **Playwright "browser not installed"**: `scripts/frontend.sh e2e` installs
  the matching chromium build on demand.
- **Port 5199 busy**: dev/preview use strictPort and will fail loudly
  rather than silently moving; free the port.
- **netns tests fail with "unshare must be available"**: the environment
  must allow unprivileged user namespaces; this sandbox does.
- **netns route setup fails with "Nexthop has invalid gateway"**: this
  kernel installs connected routes only for addresses on UP interfaces —
  bring the test interface up before adding addresses/routes.
- **Budget exceeded** (`check-durable-size.sh` fails): inspect
  `scripts/disk-usage.sh`; the usual culprits are caches inside the
  persistent tree — everything regenerable belongs in /tmp.

## Reading the Rust test counts

The netns integration tests re-execute themselves under `unshare -Urn`
with `--exact`, so each child prints its own "test result" line with a
nonzero "filtered out" count. To count the workspace's tests
deterministically, sum only the parent harness summaries (lines without
a nonzero "filtered out") — `scripts/test-all.sh` does this
automatically, and the declared count
(`rg '#\[(tokio::)?test\]' crates -g '*.rs' | wc -l`) must equal it. A
mismatch means a child was orphaned or a test skipped: investigate, never
adjust the expectation.
