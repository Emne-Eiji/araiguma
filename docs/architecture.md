# Araiguma — architecture

Araiguma is a Linux desktop network observability application. Its single
governing principle is **observe, do not control**: everything below is
read-only, and every value the system reports is either an observation or an
explicit "not observed" — never a guess.

## Workspace layout

```
araiguma/
├── crates/
│   ├── core/      domain model, provider abstraction, merge, snapshot diff
│   ├── sysfs/     sysfs/procfs fallback provider (injectable roots)
│   ├── netlink/   rtnetlink primary provider + multicast change monitor
│   ├── engine/    shared observation engine, envelopes, subscriptions, resync
│   └── cli/       `araiguma snapshot` / `araiguma watch`
├── apps/
│   ├── desktop/   Tauri 2 native shell, IPC commands/events, lifecycle
│   └── frontend/  Svelte 5 + TypeScript + Vite UI
├── fixtures/      synthetic, clearly-labeled test data (schema-locked)
├── scripts/       environment, bootstrap, validation, recovery tooling
└── docs/          this documentation set
```

## Observation pipeline

```
        ┌──────────────────┐   ┌──────────────────┐
        │  rtnetlink dumps │   │  sysfs/procfs    │
        │  (primary,       │   │  (fallback,      │
        │   priority 0)    │   │   priority 1)    │
        └────────┬─────────┘   └────────┬─────────┘
                 │ ProviderObservation  │ ProviderObservation
                 │ (per-section results)│
                 └──────────┬───────────┘
                            ▼
                      merge (core)          netlink values win;
                            │               sysfs fills None fields;
                            ▼               duplicates merge, sources unite
                    NetworkSnapshot         provider reports preserved
                            │
        ┌───────────────────┼─────────────────────┐
        ▼                   ▼                     ▼
  `araiguma snapshot`  `araiguma watch`     Tauri desktop
  (one JSON document)  (NDJSON: snapshot,   (validated IPC snapshot,
                        changed, resync)    topology, live events)
                                │                    │
                                └──── shared engine ─┘
                                     (crates/engine)
                                             │
                                             ▼
                                      frontend documents
                              (explicit mock, imported, or live)
```

The Rust model is authoritative. The frontend never discovers or reconstructs
network state; it renders validated `NetworkSnapshot` documents and their
backend-projected topology.

## Domain model semantics (crates/core)

- `Option<T>` on a field means **not observed by any provider**. It is never
  interpreted as a negative fact.
- Documented exceptions where a missing value is itself an observation:
  - `Route::gateway == None` on an observed **single-path** route means the
    kernel reported **no gateway** (an on-link route).
  - `Route::next_hops == Multi(hops)` (WO-03 M3): the route-level
    `gateway`/`output_interface` are `None` **by design** — every hop
    carries its own; multiple hops are never flattened into one gateway.
    `Undecoded(reason)` keeps a kernel multipath attribute that could not
    be decoded as an explicit marker instead of dropping it.
  - `Interface::mac_address` is the three-state `MacObservation` (schema
    v2): `Present(addr)` (serialized as the plain address string),
    `Absent` (observed: the interface has **no** hardware address, e.g.
    wireguard), or `NotObserved`. A v1 `null` was ambiguous between the
    latter two states and is rejected on read rather than silently
    assigned a meaning.
- Every item carries `sources`: the providers that observed it, in priority
  order (primary first).
- Unmapped raw kernel values are preserved verbatim (`RouteProtocol::Other(
  u8)`, `RouteScope::Other(u8)`, `RouteType::Other(u8)`) — never collapsed.

### Document schema versions

| Version | Introduced | Change |
|---|---|---|
| 1 | WO-01R | original snapshot/topology documents |
| 2 | WO-02 correction | three-state `MacObservation` everywhere a MAC appears; topology edge kinds `bondMember` and `masterMember` (a master relationship's flavor comes from the master's own kind observation, never an assumption) |
| 3 | WO-03 | veth `peerIfindex` on interfaces + `vethPeer` topology edge (M2); route `nextHops` — `'single'` / `{ multi: [...] }` / `{ undecoded: reason }` (M3). All additions are serde-defaulted, so v2 documents parse unchanged |
| 4 | WO-06 | additive optional `dns` and `connections` collections; `dns` and `connections` section kinds; provider/source/section identities accept unknown non-empty strings and round-trip them as raw strings. v3 documents parse with both collections absent (`None` = not observed); explicit `null` is rejected |

Writers emit the current version; readers accept v1 only where it is
unambiguous (a v1 document whose every `macAddress` is a non-null string).
Unknown versions are rejected with the supported range stated. v4 `dns`
and `connections` are omitted when not observed; an observed empty
collection is `[]`. DNS `mode` is an optional resolver-mode token: known
and unknown non-empty tokens are preserved, while empty or non-token
values (whitespace, control characters, or punctuation other than `.`,
`_`, `-`, `/`) are rejected at the document boundary. DNS configuration
is evidence only and is not used in diagnostics.

## Provider abstraction

A provider is a read-only source of network state implementing
`DiscoveryProvider` (async). A run produces a `ProviderObservation` with a
per-section result (`Observed(items)` / `Failed(error)` / `Unavailable` /
`PermissionDenied` / `Unsupported` / `NotObserved`) for interfaces,
IPv4/IPv6 addresses, IPv4/IPv6 routes, DNS, and connections.

Provider states (six, derived from section results — `NotObserved` is never
conflated with `Unsupported`):

| State | Meaning |
|---|---|
| `available` | every supported section observed, no fidelity limits |
| `degraded` | observed with known fidelity limits, or the provider attempted work but every supported section failed or was permission-denied without producing an observation |
| `partial` | some sections observed, and at least one supported section failed, was unavailable, was permission-denied, or was never attempted |
| `unavailable` | no section observed and at least one section was unavailable, with no failure or permission denial |
| `notSupported` | no supported sections in this environment at all |
| `notYetObserved` | nothing attempted-and-supported yet (an unattempted section is not evidence of missing functionality) |

Aggregation rules, in order: all `NotObserved` → `NotYetObserved`; all
`Unsupported` → `NotSupported`; any `Observed` with any `Failed`,
`Unavailable`, `PermissionDenied`, or `NotObserved` → `Partial`, otherwise
`Degraded` when fidelity-limited and `Available` otherwise; with no
`Observed`, any `Failed` or `PermissionDenied` → `Degraded`, otherwise any
`Unavailable` → `Unavailable`, otherwise any `NotObserved` →
`NotYetObserved`, otherwise `NotSupported`. `Unsupported` never lowers a
state. An empty section list reports `NotSupported`.

A failing provider never discards other providers' observations: the merge
keeps whatever the remaining sources observed and preserves every failure
report in `snapshot.providers`.

DNS and connection sections are additive, provenance-preserving, and
duplicate-preserving. Each section has one owning provider in WO-06
(systemd-resolved owns `dns`, NetworkManager owns `connections`);
non-owners report `Unsupported`. The merged collection is `None` unless
every owner produced `Observed`; an owner's observed empty collection is
preserved as `Some([])`. DNS observations sort globally before link scope
and then by ifindex; connection observations sort by connection type,
state, devices, and VPN flag. Duplicate identical observations are kept.
DNS and connection observations never create topology nodes or edges.

The ownership matrix is declared once, in `ProviderId::owned_optional_section()`,
and is used by both the v4 report validator and the engine's
pre-observation placeholder. It is a property of the provider identity,
never of a report index or an array position.

Future providers (systemd-networkd) implement the
same trait at their own priority; nothing in the pipeline assumes a specific
provider set.

## D-Bus foundation (WO-06B)

`crates/provider-dbus` is the only crate permitted to speak D-Bus. It is a
deliberately narrow, read-only transport foundation for the two optional
providers; it is not a general-purpose client and the desktop crate does not
depend on it.

The dependency is pinned exactly to `zbus = 6.5.0` with
`default-features = false` and only the `tokio` and `proxy` features. Each
provider gets its own `DbusFoundation`, which owns one system-bus connection
resolved through `DBUS_SYSTEM_BUS_ADDRESS` and the platform default.

The permitted surface is:

- **Bus-driver ownership preflight:** `NameHasOwner` followed by
  `GetNameOwner`. Both are addressed to `org.freedesktop.DBus`; they are
  classified separately from application calls. If `NameHasOwner` is false,
  the connection is closed and `ServiceUnavailable` is returned without any
  application message.
- **Application property reads:** only
  `org.freedesktop.DBus.Properties.Get`, and only for the exact
  service/path/interface/property combinations in
  `crates/provider-dbus/src/policy.rs`. Every call sets `NO_AUTO_START`;
  every proxy is built with `CacheProperties::No`, so no implicit `GetAll`
  is generated. The reply variant's signature is checked against the
  approved property type before it reaches a provider.
- **NetworkManager signals:** only `StateChanged`, `DeviceAdded`, and
  `DeviceRemoved` on `/org/freedesktop/NetworkManager`. Match rules pin the
  well-known sender, path, interface, and member; every delivered message is
  validated again against the unique owner captured by preflight, plus the
  exact member and body signature. `PropertiesChanged` and
  `NameOwnerChanged` are never subscribed. systemd-resolved has no signal
  surface; its observation stays polling-only.

The allow-list excludes NetworkManager manager metadata, Wi-Fi/access-point
data, active-connection/VPN type metadata, `GetAll`, `Set`, `Introspect`,
`GetManagedObjects`, `GetSettings`, `GetSecrets`, device `Ip4Config` /
`Ip6Config` / DHCP configs, every resolved link-level object, and all active
query methods. There is no name/object enumeration and no service
activation. Failures are reduced to the bounded `DbusError` categories
(`BusUnavailable`, `ServiceUnavailable`, `PermissionDenied`, `Timeout`,
`MalformedResponse`, `UnsupportedRequest`, `RejectedSignal`, `Protocol`);
raw zbus errors, D-Bus error strings, object paths, DNS values, connection
identifiers, addresses, and arbitrary service text never enter public
diagnostics.

`crates/provider-dbus/tests/private_bus.rs` proves the contract on a private
`dbus-daemon` with no service directories: mock NetworkManager and resolved
services with programmable replies, a separate `BecomeMonitor` connection,
and wire assertions for preflight, request flags, signatures, prohibited
requests, message absence, signal rules, shutdown, timeouts, and service
disappearance. No system bus or host service is touched.

### WO-06B-C1 corrective amendment (2026-11-25)

The original WO-06B allow-list recorded `DNSEx` as `a(iiay)`. That is the
signature of the distinct `DNS` property. The accepted `DNSEx` property is
`a(iiayqs)`:

- `i` ifindex;
- `i` address family;
- `ay` address bytes;
- `q` port;
- `s` server name/SNI.

The correction is required because real systemd-resolved services expose the
five-field form. The original WO-06B implementation and acceptance history
remain recorded; this amendment supersedes only the `DNSEx` signature and its
directly affected tests/mocks. The service, object path, interface, property
name, `Properties.Get` restriction, `NO_AUTO_START`, `CacheProperties::No`,
and all prohibitions on activation, enumeration, setters, `GetAll`,
`GetManagedObjects`, `Introspect`, settings/secrets, and signals are
unchanged.

Affected files: `crates/provider-dbus/src/policy.rs`,
`crates/provider-dbus/tests/private_bus.rs`,
`crates/provider-resolved/src/lib.rs`,
`crates/provider-resolved/tests/private_bus.rs`, and this document.

New verification evidence: the foundation accepts a five-field `DNSEx` reply
and rejects a legacy three-field reply; the provider retains non-default,
zero, and maximum `u16` ports as `Some(port)`; same-address/different-port
servers remain distinct; SNI is absent from observations, reports,
serialization, errors, and `Debug`; all prior wire-level restrictions remain
asserted.

## NetworkManager provider (WO-06C)

`crates/provider-nm` implements the optional NetworkManager discovery
provider on top of the accepted D-Bus foundation. Construction is
side-effect-free; the first `observe()` connects, preflights ownership,
subscribes to the three approved signals, drains pending signals, and then
performs a fresh property read. Signals are a refresh hint only and never
replace the property observation.

The provider reads only:

- manager `ActiveConnections` (`ao`);
- per active connection `Devices` (`ao`), `State` (`u`), `Type` (`s`), and
  `Vpn` (`b`) on `org.freedesktop.NetworkManager.Connection.Active`;
- per device `Interface` (`s`), falling back to `IpInterface` (`s`) only when
  `Interface` is absent or empty.

Device names are resolved to kernel ifindexes through
`<sys_root>/class/net/<name>/ifindex` (default `/sys`, injectable for
tests). A name that cannot be resolved is reported with the static
`unresolvedDevice` note and an empty device list; an ifindex is never
invented. D-Bus object paths are used transiently for property reads and
never become observation identity.

The provider does not read hardware addresses, IP4Config/IP6Config, settings
or secrets, Wi-Fi/AP data, display names, UUIDs, VPN type, manager metadata,
or any other property. It never calls activation, deactivation, enable,
sleep, reload, scan, setters, `GetAll`, `GetSettings`, `GetSecrets`,
`GetManagedObjects`, or `Introspect`.

A successful read maps to `Observed` (including a genuinely empty active
connection list). A missing service or bus maps to `Unavailable`; access
denial maps to `PermissionDenied`; timeout and malformed replies map to the
bounded `timeout` / `malformedResponse` failure codes. If some active
connections can be read and others cannot, the successful observations are
kept and the run is marked fidelity-limited with `partialConnectionRead`.
A failed refresh never leaves a previous observation in place. The provider
report carries all seven v4 sections, with only `connections` populated.

`crates/provider-nm/tests/private_bus.rs` exercises the provider against
the same private-bus harness, with a path-aware mock and fixture sysfs
tree. It verifies the approved request set, `NO_AUTO_START`, signal
subscription and sender pinning, empty versus unobserved results,
duplicates, partial reads, unresolved devices, timeouts, malformed
replies, stale-data prevention, v4 serialization, and diagnostic privacy.

### WO-06C-C1 corrective amendment (2026-11-26)

The original WO-06C allow-list constant named the active-connection
interface `org.freedesktop.NetworkManager.ActiveConnection`. Real
NetworkManager services export active-connection objects under
`org.freedesktop.NetworkManager.Connection.Active` instead; the old name is
not exported at all, so every `Properties.Get` for `Devices`, `State`,
`Type`, or `Vpn` on an active connection would have failed against the
real service.

The correction replaces the interface name in the allow-list constant
(`crates/provider-dbus/src/policy.rs`) and therefore in every
active-connection property request the provider issues. The
object-path restrictions (`/org/freedesktop/NetworkManager/ActiveConnection/{N}`
with a canonical numeric suffix), the four approved properties, their
signatures (`ao`, `u`, `s`, `b`), `Properties.Get`-only enforcement,
`NO_AUTO_START`, `CacheProperties::No`, ownership preflight, signal
handling, and all prohibitions on activation, enumeration, setters,
`GetAll`, `Set`, `GetManagedObjects`, `Introspect`, `GetSettings`, and
`GetSecrets` are unchanged.

The original private-bus mocks did not detect the defect because their
reply tables were keyed by the same production constant, so mock and
implementation repeated the identical wrong interface name and the reads
succeeded tautologically. The corrected mocks key on an independently
written literal (`org.freedesktop.NetworkManager.Connection.Active`), and
regression tests now assert the production constant against that literal,
reject the legacy interface name before the wire, reject unapproved
properties and malformed paths, and verify the interface name on the
actual wire messages. A mutation check (reverting the constant to the
legacy value) fails the policy contract tests, the provider-dbus wire
test, and the provider-nm observation test.

Affected files: `crates/provider-dbus/src/policy.rs`,
`crates/provider-dbus/tests/private_bus.rs`,
`crates/provider-nm/src/lib.rs`,
`crates/provider-nm/tests/private_bus.rs`, and this document.

The historical design record (`docs/design/prompt-03/…`) still names the
pre-correction interface together with a wider property set that the
implemented policy never adopted; it is preserved as history and superseded
by this amendment. The original WO-06C implementation and acceptance
history remain recorded; this amendment supersedes only the
active-connection interface name and its directly affected tests and
mocks. Real Ubuntu 24.04 host compatibility remains unverified until
WO-06F is completed; this amendment does not claim WO-06F passage.

## systemd-resolved provider (WO-06D)

`crates/provider-resolved` implements the optional systemd-resolved discovery
provider on the accepted D-Bus foundation. Construction is side-effect-free;
each `observe()` connects if needed, reads the three approved manager-level
properties, and assembles deterministic DNS observations. It never reads
per-link objects, never calls `GetLink`, never invokes a resolution method,
never mutates configuration, and never subscribes to signals.

The approved reads are exactly:

- `DNSEx` (`a(iiayqs)`) — ifindex, address family, address bytes, port,
  and server name/SNI;
- `Domains` (`a(isb)`) — ifindex, domain, route-only boolean;
- `ResolvConfMode` (`s`) — global resolver mode.

`DNSEx` is not the older `DNS` property: `DNS` is `a(iiay)`, while `DNSEx`
adds the port and server name. The provider retains the port as
`DnsServer.port = Some(port)` (including port zero) and discards the
server-name/SNI string immediately during decoding; SNI never enters an
observation, report, diagnostic, error, or `Debug` representation.

`DNSEx` entries are grouped by ifindex: `0` is global scope and a positive
index is link scope. Invalid indexes, address families, and address lengths
are skipped as malformed entries; an invalid index is never translated into
global scope. IPv4 uses a four-byte address and IPv6 a sixteen-byte address.
`DnsServer.port` is therefore `None` only for a source that does not supply a
port; `DNSEx` always supplies one.

`Domains` entries use the same scope rules. `route_only = false` becomes a
search domain; `route_only = true` becomes a routing domain. `ResolvConfMode`
is attached only to a global observation that already exists from observed
DNS servers or domains; the provider never fabricates an empty global scope
merely to carry the mode. An invalid or unavailable mode leaves the global
observation's `mode` as `None` and adds the static
`resolvConfModeUnavailable` note.

The provider does not read DNSSEC negative trust anchors, cache statistics,
current query activity, fallback DNS, security metadata, default-route
metadata, LLMNR, MulticastDNS, DNS-over-TLS, or any other property. It never
claims that a configured server is reachable or that name resolution works.

A successful read with no entries is `Observed([])`. A missing service or bus
is `Unavailable`; access denial is `PermissionDenied`; timeout and malformed
replies map to the bounded `timeout` / `malformedResponse` codes. If one
property fails while others supply valid data, the valid observations are
kept and the run is marked fidelity-limited. If no valid observation can be
assembled, the section reports the failure rather than an observed empty
collection. A failed refresh keeps no stale DNS data. Provider reports carry
all seven v4 sections, with only `dns` populated.

`crates/provider-resolved/tests/private_bus.rs` verifies the provider on the
private-bus harness with a path-aware resolved mock. It covers global and
per-link scopes, IPv4/IPv6, search/routing domains, mode mapping, empty and
partial results, malformed tuples, invalid modes, permission/timeout/
malformed failures, service disappearance, v4 serialization, and diagnostic
privacy. Wire assertions prove that only the three approved `Properties.Get`
requests are sent, all carry `NO_AUTO_START`, and no `GetLink`, resolution
method, `GetAll`, setter, or signal subscription is transmitted.

## Engine provider registry and optional lifecycle (WO-06E)

`crates/engine` owns a deterministic `ProviderRegistry` and the optional
provider lifecycle. The registry registers providers independently, rejects
duplicate identities, and orders them by `(priority, identity)`:

- netlink — 0;
- sysfs — 1;
- NetworkManager — 2;
- systemd-resolved — 3.

Registration and construction are side-effect-free. The default registry
constructs all four providers but performs no D-Bus I/O until an optional
provider is actually observed.

The opening document is baseline-only. `snapshot_document()` observes the
baseline providers and merges them with stable `NotYetObserved` placeholders
for optional providers that have not completed a first observation. It never
waits for optional D-Bus work. A successful optional observation replaces
its placeholder; a successful empty observation remains `Some([])`; a
failed, unavailable, denied, or malformed refresh removes stale optional
data from the merged document.

A placeholder preserves the ownership matrix rather than reporting one
uniform outcome. The section the optional provider owns is `NotObserved`
("not attempted yet"), while every section it does not own is
`Unsupported` ("not ours to report"). The opening document is therefore
valid at the v4 report boundary before the first optional poll, and the
desktop can project a topology immediately after construction without
waiting for — or triggering — optional D-Bus work (WO-07).


Optional providers are refreshed independently on the authorized 30 s
poll cadence (RD-08). The existing 150 ms change debounce and 5 s shutdown
grace remain unchanged. The engine stores the latest baseline observations
and the latest optional observations, so a baseline change/resync never
re-observes optional providers, and an optional refresh never re-observes
the baseline. Refreshes are serialized by the engine lock; there is no
overlapping refresh and no busy loop. The engine itself spawns no background
task: the CLI watch loop and desktop driver own the timer and stop it on
cancellation/shutdown.

An optional refresh that changes the document produces the existing
`changed` envelope with `notifiedBy: ["optionalRefresh"]`; the frontend’s
existing changed-event path fetches a fresh snapshot. Provider reports,
section outcomes, DNS port-sensitive identity, DNS multiset behavior, and
content-based connection identity are unchanged.

The 2 s per-provider deadline and 5 s total optional refresh budget remain
open decisions; they are not silently implemented. The current bounds are
the providers’ existing 5 s operation timeout, the 30 s optional cadence,
and the 5 s shutdown grace.

The sysfs provider reads `/sys/class/net` (interfaces), `/proc/net/if_inet6`
(IPv6 addresses), `/proc/net/route` (main-table IPv4 routes), and — since
WO-03 M4 — `/proc/net/ipv6_route` (IPv6 routes). That file spans multiple
routing tables without a table column, so its routes carry `table: None`
with a note; a missing file is a `Failed` section, never an observed "no
IPv6 routes" (absence of the file is not evidence about the route table).

## Merge rules

1. Providers ordered by priority (netlink 0, sysfs 1).
2. Field-wise: the highest-priority provider's `Some` values win;
   lower-priority providers only fill fields that are still `None`. For the
   v2 MAC observation the analogous rule is: a higher-priority `Present` or
   `Absent` stands; `NotObserved` is the only gap a lower-priority provider
   may fill.
3. Cross-source duplicates (same kernel fact seen by netlink and procfs)
   deduplicate into one item with a united `sources` list. Route identity
   (redesigned by WO-03-F / AUD-02 — the previous "next-hop-set overlap"
   rule collapsed distinct ECMP routes the kernel holds under one
   superficial key, verified live with `ip route append`) is two-level
   and evidence-based:
   - Two netlink observations are the same kernel route only when every
     observed discriminator agrees — family, destination, table, metric,
     protocol, scope, type, TOS (`rtm_tos`), flags — AND the complete
     next-hop representations are equal (gateway, via, output interface,
     weight, hop flags). Overlapping hop sets prove nothing. Identical
     `Undecoded` markers deduplicate; a partial single-path view never
     writes route-level gateway/oif onto a multipath observation.
   - A procfs row reconciles with a netlink route only when that route is
     the UNIQUE unused match on family + destination + metric (+ the
     row's table requirement: v4 rows imply main, v6 rows expose no
     table) with the row's first hop contained in the route's hop set —
     `/proc/net/route` shows only the first hop of a multipath route
     (verified live). Multiple candidate matches are never resolved by
     iteration order: the row stays an independent observation with an
     explicit ambiguity note (AUD-05). Identical duplicate table-less
     rows (e.g. the kernel's twice-printed IPv6 unreachable-default
     sentinel) are never silently deduplicated — the file cannot prove
     they are one route, so the multiplicity is preserved and annotated
     (AUD-03).
4. Output-interface enrichment: merged routes carry both ifindex and name
   when either is resolvable — including every multipath hop.

### Merged route order

Routes are ordered for stable display: address family groups (IPv4 before
IPv6), then routing-table groups (ascending table id; a not-observed table
groups with main, matching the merge identity); within a family+table,
observed destinations by ascending prefix length — **default routes first**,
then progressively more-specific prefixes — with destination-not-observed
routes after every observed destination; ties break deterministically by
metric ascending (a not-observed metric sorts last — no preference is
invented) and then gateway (on-link before a named gateway). Multiple
default routes always remain visible.

## Monitoring (crates/netlink)

`Monitor` subscribes to the link / IPv4+IPv6 address / IPv4+IPv6 route
multicast groups (equivalent to `ip monitor`) via a dedicated netlink
connection. Relevant notifications are classified (`newLink`, `newAddress`,
`newRoute`, `delLink`, ...); ACKs and unrelated messages are filtered.

If the connection dies, the monitor reports `Signal::Disconnected`
immediately and reconnects with exponential backoff (250 ms doubling to 8 s).
Each failed attempt reports `Signal::ReconnectFailed`; a successful attempt
reports `Signal::Reconnected`. The desktop turns these transitions into the
narrow `monitorHealth` IPC signal: degraded mode keeps the last confirmed
observation visible as STALE (or UNAVAILABLE when none exists), and LIVE is
restored only after a fresh observation and an explicit recovered signal.
The CLI keeps its existing three-event contract and emits the same
`resynchronized` envelope after recovery.

A receive-buffer overrun (`ENOBUFS` → `NLMSG_OVERRUN`) reports
`Signal::Resynchronized`: dropped notifications are unobserved, and the
stream never claims to be current. Ordinary bursts are debounced (150 ms)
and emitted as `changed` envelopes carrying pure snapshot diffs.

## Topology projection (crates/core::topology, WO-02)

`build_topology(&NetworkSnapshot) -> TopologyDocument` is a pure,
deterministic projection. Nodes exist for every observed interface (all
kinds preserved) and for gateways that a route explicitly names; there are
deliberately no internet/upstream nodes. Edges require evidence:

| Edge | Evidence |
|---|---|
| `bridgeMember` | kernel master statement **and the master's own kind observed as a bridge** |
| `bondMember` | kernel master statement **and the master's own kind observed as a bond** |
| `masterMember` | kernel master statement where the master's type could not be established — presented neutrally, never assumed to be a bridge |
| `linkParent` | kernel parent statement (IFLA_LINK: vlan parent, tunnel underlay) |
| `gatewayRoute` | a main-table route names both oif and gateway (destinations aggregated); a multipath route (WO-03 M3) contributes EVERY hop as the same evidence class — one edge per (interface, gateway) pair, with the kernel weight in the edge detail |
| `gatewayInferred` | gateway without oif falls inside exactly one interface's subnet (reason recorded) |

A master or parent ifindex that names a missing interface produces an
annotation on the member node, never a dangling edge; self-referential
master/parent indexes are invalid references and are never drawn.

Observed vs inferred evidence classes are carried on every edge and rendered
differently (solid vs dashed). Unknown relationships (VPN peers, upstream
beyond a gateway, ambiguous inference) surface as node annotations — never
as fabricated edges. `araiguma topology` projects a snapshot document
(file or stdin) into this format; `fixtures/topology/*.json` are committed
goldens generated by that projection and parsed by both the Rust and the
frontend test suites.

## Testing architecture

| Layer | What | Where |
|---|---|---|
| Unit | model serde, merge precedence, dedup, provider states, diff, kernel-format parsing, netlink message translation (constructed messages, no sockets) | `crates/*/tests`, `crates/netlink/src/translate.rs` |
| Fixtures | synthetic sysfs/procfs trees + snapshot JSON documents, schema-locked from Rust **and** TypeScript | `fixtures/` |
| Integration | real kernel behavior inside transient unprivileged network namespaces (`unshare -Urn` self-re-exec): dumps, classification, local-table presence, monitor lifecycle, resync, merge precedence | `crates/netlink/tests/netns.rs` |
| Live cross-validation | the real binary against iproute2 JSON output in a netns, plus a watch smoke | `scripts/live-check.sh` |
| Frontend unit | formatters, state semantics, fixture contract, parse validation | `apps/frontend/tests` (vitest) |
| Browser | smoke, design verification (dark theme, 56px rail, ≥96% workspace, badge), panel-overlays-workspace | `apps/frontend/e2e` (Playwright, production build) |
| Native desktop | real Tauri window; LIVE/STALE, monitor disconnect/recovery, XDG paths, graceful close and SIGTERM/SIGINT shutdown; UNAVAILABLE frontend regression | `apps/desktop` plus workstation evidence |
| Automated gate | workspace tests + frontend checks + fmt/clippy + budget + identity audit | `scripts/test-all.sh` |

Network namespace tests create and destroy interfaces **only** inside
`unshare -Urn` child processes; the host namespace is never modified. `ip`
is used for test setup and independent verification only — the application
itself never shells out.

## Frontend

Svelte 5 (runes) + TypeScript + Vite. Dark charcoal design system, 56 px
icon-led navigation rail, central workspace at ≥96% of viewport width, and a
contextual details panel that overlays (never shrinks) the workspace. While
synthetic data is displayed, a persistent `MOCK DATA` badge is visible. In
the native Tauri application, live documents occupy a separate store and
are labeled LIVE, STALE, or UNAVAILABLE according to the event monitor's
actual health; an explicit import or fixture selection remains the rendered
view.

The TypeScript model in `apps/frontend/src/types.ts` mirrors the Rust serde
output. The sync contract: `fixtures/snapshots/*.json` are parsed by Rust
tests (authoritative model) and by the frontend tests (mirror) — any drift
fails the suite. See `docs/development.md` for the change procedure.

Document loading goes through fully-validated boundaries (`lib/snapshot.ts`,
`lib/topology.ts` on top of `lib/validate.ts`): every nested structure —
interfaces, addresses, routes, provider outcomes, node ids, edges, evidence —
is checked, including numeric ranges, family consistency, duplicate node
identifiers, and dangling edge references. Invalid documents are rejected
with a path-qualified reason and never partially rendered.

Route rows are keyboard-operable (Enter/Space select, visible focus,
aria labels); selecting a route opens a details overlay exposing flags,
observation notes (including representation limitations), and provenance.
A small production test hook exists on the topology view only when the page
is opened with the explicit `?araiguma-test-hooks` parameter (or in dev
mode), and is removed when the view unmounts.

## Remaining integration points

- NetworkManager (WO-06C) and systemd-resolved (WO-06D) providers are
  implemented, and the engine registry/optional lifecycle (WO-06E) is in
  place. The real Ubuntu 24.04 host checkpoint (WO-06F) remains separate.
- Typed IPC errors and the full Document-04 subscription lifecycle (UUID v4
  IDs, idempotent cancellation, `Cancelled`, terminal `ended`, and
  per-subscription channels) remain deferred. Stage 1 returns plain string
  errors and a minimal subscription lifecycle.
- Tray, notifications, and packaging remain outside WO-05 Stage 1.

## Design trajectory (Prompt 03 specification)

The backend architecture and network data model that govern the
implementation stages are specified in
[`docs/design/prompt-03/`](design/prompt-03/README.md) (approved decisions
OD-01/02/03/08/09/10 of the accepted WO-02 feasibility assessment,
translated into crate boundaries, a provider contract, evidence semantics, a
schema-v4 document/IPC contract, logging/diagnostics design, read-only
enforcement, and the test strategy). WO-05 Stage 1 implements the shared
engine and Tauri shell portions; later optional providers, tray behavior,
logging, and packaging remain separate work.
