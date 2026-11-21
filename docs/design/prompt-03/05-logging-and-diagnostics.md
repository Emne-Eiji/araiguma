# 05 — Logging, diagnostics, and session state design

Prompt 03 deliverable (OD-09 assigns this design to Prompt 03; OD-08
fixes the locations): log levels and structured fields, timestamp
conventions, rotation and bounded storage, startup/shutdown and
provider-failure logging, diagnostic identifiers and correlation,
diagnostics export with redaction and privacy safeguards, and behavior
when directories are unavailable or unwritable.

This is a specification. No logging or diagnostics code is implemented or
authorized by this document (work order §3.8). The existing CLI ships a
minimal `ARAIGUMA_LOG=debug` stderr tracer [REPO — usage text]; this
design supersedes it as the target for implementation prompts.

Sources: OD-08, OD-09, work order §3.8; XDG Base Directory
specification (state directory `~/.local/state`); the sandbox's
durable-storage discipline (950 MB persistent budget — the constraint
that already shapes repository practice [REPO —
`docs/environment-inventory.md`, `scripts/check-durable-size.sh`]).

## 1. Locations (OD-08 — binding)

| Content | Path | Created by |
|---|---|---|
| Configuration | `~/.config/araiguma/config.toml` | user / defaults |
| Logs | `~/.local/state/araiguma/logs/araiguma.log` (+ rotations) | application |
| Session registry | `~/.local/state/araiguma/sessions.jsonl` (+ rotations) | application |
| Diagnostics exports | `~/.local/state/araiguma/diagnostics/araiguma-diagnostics-<UTC timestamp>.jsonl` | application, on explicit user action |

Rules:

- Paths follow XDG semantics: `$XDG_CONFIG_HOME`/`$XDG_STATE_HOME`
  override the `~` defaults when set (standard behavior; documented in
  `--help`). The fixed subpaths `araiguma/…` are OD-08's decision.
- Directories are created on demand with mode 0700 (state may contain
  observation-derived data; conservative default), files 0600.
- Nothing is ever written outside these locations (plus the user-chosen
  export destination). No `/tmp` spill, no cache directory, no
  `~/.cache` (there is nothing to cache — no regenerable state worth
  persisting beyond logs).
- Upgrade-safety (OD-08 rationale): the directories are owned by the
  user, never by the package; `.deb` upgrades/removals must not touch
  them (R-09 mitigation; document 07 §9 tracks the local validation).

## 2. Configuration file (`config.toml`)

One file, TOML (human-editable, comment-friendly, standard in the Rust
ecosystem and inspectable with Python 3.11+ `tomllib` — RD-20):

```toml
[logging]
level = "info"            # trace | debug | info | warn | error
retention_mb = 50         # total bound across rotations (§5)

[observation]
poll_interval_secs = 30   # poll fallback (document 02 §6)
reprobe_interval_secs = 60
shutdown_grace_secs = 5

[freshness]               # staleness policy (document 03 §10)
default_secs = 60
per_section = { }         # e.g. { dns = 300 }

[privacy]                 # diagnostics-export policy (§9.1)
mac_addresses = "redact"  # redact (default) | vendor-prefix | include
ip_addresses  = "include" # include (default) | redact
```

- Absent file ⇒ all defaults; **no warning** (a default-config run is
  normal, not an anomaly).
- Malformed file ⇒ defaults + a prominent `warn`-level log record
  naming the file and the parse error + a diagnostics marker. The
  application must never crash on its own configuration (work order
  §3.8), and the warning must never be silent (the operator learns the
  config was ignored).
- Unknown keys are ignored with a `debug` record (forward tolerance);
  unknown *values* for known enums are a malformed-value warning.
- A `config` command surface to print the effective configuration is a
  Prompt 04/implementation decision (not specified here beyond the
  file's contract).

## 3. Log levels and structured fields

Format: **JSON Lines** (one JSON object per line) — machine-parseable,
greppable, and trivially embeddable in diagnostics export (RD-21). A
plain `--log-format=text` CLI escape hatch for humans reading a
terminal is acceptable; the file format is always JSONL.

Levels (standard Rust `log`/`tracing` semantics): `trace`, `debug`,
`info`, `warn`, `error`. Level policy:

| Level | Content |
|---|---|
| `error` | data loss risks: log write failures, engine shutdown aborts, document publication failures |
| `warn` | degraded operation: provider failures, permission denials, config ignored, stale-session detection |
| `info` | lifecycle: startup, shutdown, session summary, provider state transitions (first failure and recovery) |
| `debug` | per-observation progress, provider section outcomes, debounce/resync decisions |
| `trace` | verbose internals (never enabled by default; never contains payload bodies beyond identifiers) |

Structured fields (the fixed vocabulary — the redaction allowlist of §7
is defined over it):

| Field | Type | Always? | Notes |
|---|---|---|---|
| `ts` | RFC 3339 UTC string, millisecond precision | yes | §4 |
| `level` | enum | yes | |
| `session_id` | UUID v4 | yes | correlation root (document 01 §6.1) |
| `collection_id` | string `"<session>:<index>"` | when in an observation | document 04 §2 |
| `provider` | provider id string | provider-scoped records | document 02 §2 |
| `event` | stable event name | yes | e.g. `startup`, `provider_probe`, `resync`, `shutdown` |
| `detail` | free-form string | when applicable | the human message |
| `error_kind` | classified error kind | error/warn records | e.g. `dbus-denied`, `netlink-socket`, `fs-permission` |
| `seq` | envelope seq | watch/IPC records | |

Free-form `detail` strings are for humans; machine consumers key on
`event` + `error_kind` (the classification contract of document 02 §7).
No field outside this vocabulary is written without extending this
specification (review gate; document 06 §5).

**Detail-string content rule (binding; the foundation of §9's privacy
posture):** `detail` (and every other free-form string — error,
reason, and note fields) never embeds SSIDs or other wireless network
names, VPN connection names, hostnames, or user/home-directory paths.
Classification and correlation use interface names, provider ids,
section kinds, and stable identifiers, which suffice; a representative
corpus asserts the rule in the redaction tests (document 07 §5).

## 4. Timestamp and timezone conventions

- All log timestamps are **UTC, RFC 3339 with milliseconds**
  (`2026-11-21T14:03:09.512Z`). No local time is ever written (the
  project-wide UTC rule; work order header). Consumers who want local
  display convert at the presentation layer only.
- Observation timestamps in documents remain epoch milliseconds
  (existing contract [REPO]); logs use RFC 3339 strings (readability
  first for logs, compactness first for documents — both are UTC and
  trivially interconvertible).

## 5. Rotation policy and bounded storage

- Size-based rotation: a log file is rotated at **5 MiB** to
  `araiguma.log.1` … `.N` (compressed rotations are unnecessary at this
  scale; simplicity wins).
- Bound: total log storage ≤ `retention_mb` (default **50 MiB**);
  on breach, the oldest rotations are deleted first; the *current*
  file is never deleted to satisfy the bound.
- `sessions.jsonl` rotates identically at 1 MiB (bound: 5 MiB).
- Diagnostics exports (§6) count against their own user-visible
  location; a startup check warns when the whole state directory
  exceeds 100 MiB (misconfiguration symptom; nothing is auto-deleted
  there — exports are user artifacts).
- The application's own footprint is bounded by construction; the
  sandbox durable-budget discipline continues to apply to the
  repository, not the user's machine (document 07 §2 reiterates the
  Track split).

## 6. Session registry and correlation

`sessions.jsonl` — one append-only JSON record per session:

```json
{"sessionId":"…","startedMs":…,"endedMs":null,"binary":"araiguma","version":"1.0.0","unclean":false}
```

- On startup: append a record with `endedMs: null`. On clean shutdown:
  update by appending a completion record `{"sessionId":"…","endedMs":…,
  "collections":n,"envelopes":m}` (append-only; readers take the last
  record per session).
- A next-startup scan marks a session with no completion record
  `unclean: true` (crash evidence — document 01 §6.4) and logs a
  `warn` (once).
- Correlation: every log record and every document's
  `meta.collection.sessionId` share the session id — a diagnostics
  consumer can reconstruct the timeline from logs + documents alone.

## 7. Startup, shutdown, provider, and watch logging events

Mandatory lifecycle records (event names are the stable contract):

- `startup` (info): binary, version, session id, config path used,
  XDG paths in effect, pid.
- `config_status` (warn only when ignored; info when loaded).
- `provider_probe` (info): one per provider — capability summary
  (available/permission/sections) from document 02 §5.
- `provider_state` (info on first failure and on recovery; debug
  otherwise): provider id, old→new state, error_kind.
- `observation` (debug): collection id, per-provider section outcomes,
  duration.
- `resync` (info): reason (disconnect | overrun | manual), gap bound.
- `shutdown` (info): reason (signal | window-close | error), duration,
  sessions-completed counters; `shutdown_abort` (error) names any
  component that exceeded the grace period.
- `log_write_failure` (error): emitted to **stderr** when the log sink
  itself fails (§8).

## 8. Behavior when directories are unavailable or unwritable

Read-only home, missing permissions, full disk, read-only state path:

1. The application **starts and runs** — observation is not gated on
   logging (degrade, never block; OD-02's spirit applied to I/O).
2. Logging falls back to **stderr** (JSONL, unchanged format), with a
   single `log_write_failure` error record explaining the path and the
   OS error, re-attempted at most once per hour (no log-failure loops).
3. Session registry and diagnostics export report the same condition
   explicitly: export fails with a user-facing error naming the path
   and OS reason (never a silent no-op, never a crash).
4. No alternate location is silently invented (no `/tmp` fallback —
   that would hide the condition from the user).

## 9. Diagnostics export

**User-initiated only.** Nothing is ever transmitted anywhere
automatically (work order §3.8; no telemetry exists anywhere in the
product — document 06 §2). Export produces a single
`araiguma-diagnostics-<UTC>.jsonl` file at a destination the user
chose (default: the state diagnostics directory).

Contents — exactly (include/exclude is the contract):

| Included | Excluded |
|---|---|
| environment summary: OS, kernel, package version, binary channel, architecture | any credentials, tokens, or private keys (none exist in the app; the exclusion is audited) |
| effective configuration **with secrets-free fields only** (the schema has no secret fields — asserted) | file contents other than the log itself |
| session registry records (§6) | full document bodies (sizes; only summaries below) |
| the log file contents (bounded per §5) | network payloads, captured traffic (nothing is captured, ever) |
| capability reports of all providers (document 02 §5) | contents of `~/.ssh`, browser profiles, unrelated paths (never read) |
| the most recent document **summary**: counts per section, provider states, timestamps, document/collection ids, conflict counts | MAC addresses — **fully redacted by default** (§9.1): replaced by the marker `<redacted>`, all six octets; `vendor-prefix` (first three octets) and `include` are explicit user disclosures via `privacy.mac_addresses`, never defaults |
| IPC/watch statistics: envelope counts, resync events | |
| `config.toml` **as written by the user** (it has no secret fields by schema) | |

### 9.1 Field-level privacy policy (binding)

The export schema is an **allowlist**: a field is written only if the
table above names it and §3's vocabulary defines it — a field not on
the list never enters the export (structural privacy). Additionally,
the free-form string content that the allowlisted fields carry (log
`detail`, error/reason strings) passes a **pattern-redaction pass** at
export construction for the value categories marked redacted or
generalized below. Neither mechanism alone is sufficient — the
allowlist bounds which fields exist; the pattern pass bounds what the
retained free-form strings contain — and both are tested (document 07
§5). Redacted values are replaced by the literal marker `<redacted>`;
generalized paths use `~/…` form.

Per-category policy (each category states: included by default?
treatment? diagnostic necessity? user control? exposure?):

| Category | Reaches the export via | Default treatment | Why needed for diagnostics | User control | What it can reveal |
|---|---|---|---|---|---|
| MAC addresses | free-form log/error strings only (the document summary carries counts, never values) | **fully redacted** — all six octets, marker `<redacted>`; no vendor OUI retained | correlating interface-level errors | `privacy.mac_addresses`: `redact` (default) / `vendor-prefix` (first three octets — an explicit disclosure of hardware vendor) / `include` | device identity; hardware vendor (OUI) |
| IPv4 addresses | free-form log/error strings | retained (`include` default) — network diagnostics is the product's subject matter; a user exporting diagnostics expects them | address/route observation diagnosis | `privacy.ip_addresses`: `include` / `redact` | local network topology (private ranges) |
| IPv6 addresses | free-form log/error strings | retained (same setting) | same — includes link-local and privacy-extension addresses | same | interface identity (EUI-64-derived bits can expose the MAC); topology |
| Interface names | log records (`detail` references interfaces) | retained | the primary correlation key in every diagnostic line | none (host-local names; a deliberate retention decision) | weak hardware hints (name encodes driver/slot); host-local only |
| SSIDs / wireless network identifiers | nowhere, by construction (§3 detail-string rule; summaries carry counts, not values) | omitted | not needed — section outcomes and counts diagnose Wi-Fi sections | none (a future disclosure option would be a reviewed schema change) | location (network names are location-correlated); organization |
| VPN names / provider identifiers | nowhere, by construction (same rule); generic VPN **type** classifications (`vpn`, `wireguard`, …) may appear as classification values | names omitted; type classifications retained | type evidence diagnoses the VPN section | none | organization / network affiliation |
| Route destinations & gateways | nowhere, by construction (full document bodies excluded; summaries carry counts; §3 rule keeps values out of `detail`) | omitted | counts and section outcomes suffice | none | network topology |
| DNS server addresses & search domains | nowhere, by construction (same); the resolver **provider state** (available / permissionDenied / …) is retained in capability reports | values omitted; provider states retained | diagnosing resolver provider failures needs the state, not the values | none | topology; organization (search domains) |
| Hostnames | nowhere (the environment summary deliberately excludes the hostname) | omitted | OS, kernel, version, channel, architecture suffice | none | machine identity; location |
| User / home-directory paths | startup/config records name the config and state paths in effect | **generalized**: `/home/<user>/…` → `~/…` (likewise any `$XDG_*` override) | diagnosing config/state path issues needs the path shape, not the username | none | username (identity) |
| Hardware identifiers | MAC addresses (above) are the only hardware identifiers the application ever observes; no serials, machine ids, or hardware UUIDs are collected | MAC per above; others not collected | — | — | device identity |
| Provider object paths (D-Bus) | capability/error strings (e.g. `/org/freedesktop/NetworkManager/Devices/3`) | retained | correlating provider failures to objects | none | generic service topology only (no user identity) |
| Timestamps & observation metadata | every record (§4 UTC RFC 3339; document-summary timestamps and ids) | retained | the timeline is the diagnostic | none | activity times (behavioral); no location |

Hard exclusions (never in an export, under any setting): credentials,
passwords, private keys, tokens, NetworkManager secrets, unrelated
personal files, arbitrary environment variables, unrelated process
data, browser data, and any undisclosed external communication or
telemetry (none of these exist in the application's data model; the
exclusion is audited — document 06 §5 check 8, document 07 §5).

- The export's first line is a manifest record listing what was
  included, what was excluded, and the effective redaction settings
  (`privacy.mac_addresses`, `privacy.ip_addresses`) — the file
  describes itself.
- Redaction and the allowlist are unit-tested (document 07 §5) against
  a representative corpus: MAC (all three modes), IPv4, IPv6,
  SSID-bearing, VPN-name-bearing, home-path, and secret-bearing inputs,
  plus field-allowlist totality (no field outside the list can appear
  in an export) and the §3 detail-string rule.

## 10. What this design deliberately does not do

- No remote log shipping, no crash-report upload, no analytics (none
  exist; document 06 §2 prohibits introducing any).
- No log-based observation: logs never become a data source for
  network facts (documents are the only contract — OD-03).
- No binary log format, no external logging service dependency
  (RD-21 rationale).
- No retention of observation documents in the state directory (the
  live view is in-memory; the user's documents are the user's files).

## 11. Decision summary

| Decision | Source | Status |
|---|---|---|
| XDG locations exactly as OD-08 | OD-08 | approved (binding) |
| TOML configuration, defaults-on-absence, warn-on-malformed | work order §3.8 | RD-20 |
| JSONL logs, fixed structured-field vocabulary | §3; export/redaction simplicity | RD-21 |
| UTC RFC 3339 log timestamps | project UTC rule | approved-derived |
| 5 MiB × rotations, 50 MiB default bound, oldest-first deletion | §5; bounded-storage principle | RD-22 |
| Append-only session registry with unclean detection | document 01 §6; crash forensics | RD-23 |
| Field-allowlist export with per-category privacy policy (§9.1), **full** MAC redaction by default (partial disclosure only by explicit user choice), and self-describing manifest | work order §3.8 privacy safeguards; Prompt 03-FC §6 (FC-03) | RD-24 |
| stderr fallback, never block, no silent alternate location | work order §3.8 | approved-derived |
