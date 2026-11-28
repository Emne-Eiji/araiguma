# Araiguma — development guide

## One-time setup

```sh
source scripts/env.sh   # every shell: toolchain + ephemeral build paths
./scripts/bootstrap.sh  # idempotent: toolchain, cargo fetch, fixtures, frontend work copy
```

`bootstrap.sh` re-creates all ephemeral state after a sandbox recycle
(CARGO_TARGET_DIR, CARGO_HOME with the persistent crate-cache link, npm
cache, frontend work copy, Playwright browsers on demand) **and generates
the sysfs/procfs fixture trees** (WO-03-F / AUD-04): several fixtures are
EMPTY directories (`wlan0/wireless`, `br0/bridge`, the empty-observation
`class/net`) that Git cannot track, so a fresh clone relies on the
bootstrap to create them — generation is deterministic, idempotent, and
verified after running. There is exactly ONE documented setup path; no
manual fixture preparation is ever needed.

Fresh-clone validation (WO-03-FC / AUD-04 closure) proves the full
property: a genuinely fresh checkout of an exact commit passes the
COMPLETE documented workflow, not just a subset of the tests.

* `scripts/fresh-clone-prepare.sh <dest> [<commit>]` — the preparation
  half: isolated clone from the exact commit (tracked content only,
  empty-directory fixtures verified absent before bootstrap), documented
  bootstrap, fixture generation and byte-stability check.
* `scripts/check-fresh-clone.sh` — the validation half: prepares a fresh
  clone under `/tmp/araiguma-fresh-clone-area` and runs the clone's own
  `scripts/test-all.sh` inside it (fmt, clippy, complete workspace tests
  including isolated-netns integration tests, svelte-check, Vitest,
  production build, Playwright e2e, npm audit, D-Bus harness probe, live
  observation with iproute2 cross-validation, live CLI documents through
  the frontend validators, git identity audit, shell syntax, fixture
  stability, storage budget) with fully isolated build/work/log
  locations. It is the last step of `test-all.sh`; it never recurses
  (the nested fresh-clone step skips itself with an explicit recursion
  guard), and the primary checkout's earlier steps never depend on it.

## Everyday commands

| Task | Command |
|---|---|
| Format / lint / test Rust | `cargo fmt --all` · `cargo clippy --workspace --all-targets -- -D warnings` · `cargo test --workspace` |
| Build the CLI | `cargo build -p araiguma-cli` (`$CARGO_TARGET_DIR/debug/araiguma`) |
| Build the native desktop | `npm --prefix apps/frontend run build` then `cargo build --release -p araiguma-desktop --features custom-protocol` |
| Observe | `araiguma snapshot [--pretty]` · `araiguma watch` |
| Frontend type check | `scripts/frontend.sh check` |
| Frontend unit tests | `scripts/frontend.sh test` |
| Frontend build / e2e | `scripts/frontend.sh build` · `scripts/frontend.sh e2e` |
| Frontend dev server | `scripts/frontend.sh prepare` then run `npm run dev` inside `/tmp/araiguma-work/apps/frontend` (port 5199) |
| Live cross-validation | `scripts/live-check.sh` |
| Everything | `scripts/test-all.sh` |
| Storage report | `scripts/disk-usage.sh` / `scripts/check-durable-size.sh` |

The desktop's `custom-protocol` feature embeds the production frontend
bundle. Without that feature a native binary uses Tauri's development URL and
shows a connection-refused page instead of the application.

Frontend tooling runs in an ephemeral work copy (`/tmp/araiguma-work` by
default; `ARIGUMA_FRONTEND_WORK` moves it, as the fresh-clone validation
does) that mirrors the repository layout; `node_modules` never exists
inside the repository. Edit sources in the repository — `frontend.sh`
refreshes the copy on every invocation. The generated
`package-lock.json` is copied back and must stay committed.

## Optional D-Bus foundation tests

`crates/provider-dbus` is the constrained, read-only D-Bus foundation for the
optional NetworkManager and systemd-resolved providers. It is pinned to
`zbus = 6.5.0` with only `tokio` and `proxy`; see `docs/architecture.md` for
the permitted method, property, and signal surface.

The tests never use the host system bus. Each test starts a private
`dbus-daemon` from a temporary config with no service directories, exports
its address as `DBUS_SYSTEM_BUS_ADDRESS`, runs raw mock services, and records
the actual traffic with a separate `BecomeMonitor` connection. Tests are
serialized inside the test binary because the bus address is process-wide.
This requires `dbus-daemon` on `PATH`; `scripts/check-dbus-harness.sh` records
whether the sandbox can run private buses. If the daemon is missing the
tests fail explicitly rather than silently skipping.

```sh
cargo test -p araiguma-provider-dbus
```

The suite asserts preflight ordering, `NO_AUTO_START` on every application
call, absence of `GetAll`/`Set`/`Introspect`/settings/secrets/enumeration,
approved reply signatures, exact signal rules and sender pinning, bounded
timeouts and error categories, failure without activation when a service is
absent or disappears, and connection shutdown. Since WO-06C-C1 it also
asserts the real active-connection interface
(`org.freedesktop.NetworkManager.Connection.Active`) against an independently
written literal on the wire, and rejects the legacy
`org.freedesktop.NetworkManager.ActiveConnection` interface before the wire.

## NetworkManager provider tests

`crates/provider-nm` implements the optional NetworkManager provider on the
accepted D-Bus foundation. Its tests reuse the private-bus harness and add a
path-aware mock plus a fixture sysfs tree, so each active connection and
device can return different approved values without touching the host.

```sh
cargo test -p araiguma-provider-nm
```

The suite covers construction, available and absent services, active
connection discovery, device/interface mapping, VPN/state, empty and
duplicate results, approved signals, partial and unresolved reads, permission
denial, timeout, malformed replies, stale-data prevention, v4 serialization,
and diagnostic privacy. Since WO-06C-C1 it also proves the provider reads
active connections through the real NetworkManager interface
(`org.freedesktop.NetworkManager.Connection.Active`, verified against an
independently written literal on the wire), never the legacy
`org.freedesktop.NetworkManager.ActiveConnection` name.

## systemd-resolved provider tests

`crates/provider-resolved` implements the optional systemd-resolved provider
on the accepted D-Bus foundation. Its tests reuse the private-bus harness and
add a path-aware mock for the three approved manager properties.

```sh
cargo test -p araiguma-provider-resolved
```

The suite covers global and per-link DNS scopes, IPv4/IPv6 addresses, search
and routing domains, `ResolvConfMode`, empty versus partial observations,
malformed tuples and modes, permission/timeout failures, service
disappearance, v4 serialization, privacy, and wire-level proof that no
`GetLink`, resolution method, setter, `GetAll`, or signal subscription is
used.

## Engine optional-provider lifecycle tests

`crates/engine` owns the provider registry and optional-provider lifecycle.
The tests use deterministic scripted providers and cover registry ordering,
duplicate rejection, baseline-only opening snapshots, stable
`NotYetObserved` reports, optional success/empty/failure replacement,
disappearance/recovery, baseline isolation, event delivery, and shutdown.

`crates/engine/tests/startup_validation.rs` covers the startup boundary
(WO-07): the opening document validates as a v4 provider report, immediate
topology projection succeeds before any optional observation, the snapshot
and topology paths agree on ownership, the first optional refresh keeps the
document valid, and the distinct outcomes (`NotObserved`, `Unsupported`,
`Observed([])`, a non-empty observation, `Unavailable`, `Failed`) stay
distinct. The placeholder reports the owned section as `NotObserved` and
every non-owned section as `Unsupported`.

```sh
cargo test -p araiguma-engine
```

The CLI watch loop and desktop driver own the optional refresh timer; the
engine itself spawns no background task. The default cadence is the
authorized 30 s optional poll interval.

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
`Other(u8)` variants serialize as `{"other": n}`). The identity enums
`ProviderId`, `ObservationSource`, and `SectionKind` use `Other(String)`:
unknown non-empty identities serialize as the raw string (for example
`"futureProvider"`), never as a tagged object.

### Schema versions

Documents carry `schemaVersion`. Writers emit the current snapshot version
(v4) and the current topology version (v3) — see `SNAPSHOT_SCHEMA_VERSION`
and `TOPOLOGY_SCHEMA_VERSION` in crates/core. Readers accept older
versions only where they are unambiguous — a v1 snapshot is accepted only
if every `macAddress` is a non-null string, because v1 `null` could mean
either "no hardware address" or "not observed". v2 and v3 snapshots parse
under the v4 model; v4's optional `dns` and `connections` fields are
absent in older documents, which means "not observed". A present v4
`dns` or `connections` field must be an array; explicit `null` is
rejected. DNS `mode` is an optional token: unknown non-empty values are
preserved; empty or non-token values (whitespace, control characters, or
punctuation other than `.`, `_`, `-`, `/`) are rejected.
When bumping a version: update the constant, document the change in its
doc comment and in `docs/architecture.md`, and update fixtures +
validators in the same commit.

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

- Current development branch: **`main`**, tracking **`origin/main`** on the
  GitHub remote `origin` (`https://github.com/Emne-Eiji/araiguma.git`). It is
  the canonical branch.
- The pre-recovery two-commit history is preserved on
  `archive/main-pre-recovery-2026-11-21` for historical reference only; it is
  not an active development branch. The former `recovery/araiguma-main` branch
  has been retired.
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
  RUSTUP_HOME/CARGO_HOME/PATH; the intended toolchain lives in
  `../toolchains`. On this workstation that sibling toolchain is incomplete;
  use the complete `~/.rustup` toolchain by exporting
  `RUSTUP_HOME="$HOME/.rustup"` and `ARIGUMA_CARGO_HOME="$HOME/.cargo"`
  (and `ARIGUMA_TOOLCHAINS="$HOME/.cargo"` for scripts that source
  `scripts/env.sh`).
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
