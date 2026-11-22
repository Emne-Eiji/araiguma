# 06 — Security and read-only enforcement design

Prompt 03 deliverable: read-only operation as a structural product
invariant — not a UI convention, not a promise — with layered
enforcement, a concrete reviewable D-Bus operation allow-list, Tauri
capability discipline, invariant tests, and the runtime behavior when
permissions or provider access are unavailable.

Sources: OD-02 (capability-probed optional providers), OD-01 (Tauri 2),
WO-02 chapter 5 (read-only feasibility, prohibited operations,
five-layer enforcement), R-03 (D-Bus mutation risk), Q-1/Q-2, work
order §3.3. Existing structural guarantees verified at baseline [REPO —
WO-02 §5.1] are the foundation and are kept binding.

## 1. The invariant

**Araiguma never modifies network state — kernel, service, or resolver —
through any path, at any time, for any purpose, including discovery.**
Observation is passive. No provider may change network state even
temporarily as a discovery technique (work order §3.3 — binding).

Corollary invariants:

- **No privileged anything**: no privileged helpers, daemons, or
  autostart services are installed or activated; no elevation is ever
  requested or attempted (pkexec, sudo, polkit agents — none).
- **No undisclosed communication**: the only outbound channels the
  application may ever open are (a) AF_NETLINK kernel sockets and
  (b) the local D-Bus sockets (system/session bus). No telemetry, no
  update pings, no HTTP client exists anywhere. Violations are
  structurally impossible (no such dependency — §5 audit) and audited.
- **No credential or sensitive-data collection**: the application never
  reads credentials, private keys, or unrelated user data;
  `Settings.Connection.GetSecrets` (NetworkManager) is explicitly
  prohibited (§4); log/diagnostic fields are allowlisted (document 05).

## 2. Explicit prohibitions (the review checklist)

Each prohibited operation class, its rationale, and its enforcement
layer (extending WO-02 Table 5.1, which this design adopts in full):

| Prohibited operation | Rationale | Enforcement |
|---|---|---|
| Constructing/sending any netlink `RTM_NEW*` / `RTM_SET*` / `RTM_DEL*` / `RTM_*LINK` mutation message | direct mutation of interfaces/addresses/routes | L1 structural (trait has no mutation surface [REPO]; codec exercised only through dump/subscribe paths) + L4 audit |
| Write/truncate/create on `/proc/sys/*`, `/sys/class/net/*`, `/etc/resolv.conf`, `/etc/network/*`, netplan files | kernel/distribution configuration | L1 (providers open files read-only; injectable roots [REPO]) + L4 audit (no write-mode opens in provider crates) |
| NetworkManager setters (`ActivateConnection`, `AddAndActivateConnection`, `DeactivateConnection`, `Enable`, `Sleep`, `Reload`, `Disconnect`, `RequestScan`, `CheckConnectivity`, any property **Set**) | connection/radio/service state changes; `CheckConnectivity` forces active probing traffic | L1 restricted proxy (§4) + L4 allow-list audit |
| NetworkManager `Settings.Connection.GetSecrets` | **credential access** — the single most sensitive read NM offers | L1 + L4; also excluded from diagnostics (document 05 §9) |
| systemd-resolved mutating methods (`SetLinkDNS`, `SetLinkDNSEx`, `SetLinkDomains`, `SetLinkLLMNR`, `SetLinkMulticastDNS`, `SetLinkDNSOverTLS`, `SetLinkDNSSEC`, `SetLinkDNSSECNegativeTrustAnchors`, `SetLinkLocalAddress`, `SetDnssecTrustAnchor`, `RevertLink`, `FlushCaches`) | resolver state changes (including cache mutation) | L1 + L4 |
| systemd-resolved query methods (`ResolveHostname`, `ResolveService`, `ResolveAddress`, `ResolveRecord`) | generate DNS traffic — active operations, not passive observation | L1 + L4 (§4 rationale) |
| `systemctl` / service start/stop/enable; unit installation | service state changes | L1 (no process spawning in application crates — existing property [REPO]) + L4 audit |
| Firewall manipulation (nftables/iptables/ufw/firewalld D-Bus) | security policy changes | L1 (no such dependency) + L4 |
| Raw sockets for injection; ARP/DNS spoofing; traffic interception/redirect; proxy or resolver installation | observe, never interfere | L1 (no such modules; only AF_NETLINK + D-Bus sockets) + L4 dependency audit |
| Creating/deleting network namespaces or links (outside the existing test harness) | mutation; test namespaces are confined to `unshare -Urn` children [REPO] | L1 (production code never spawns) + L4 |
| Installing kernel modules, privileged daemons, autostart services | product constraint: no privileged background services | packaging rules (R-09; document 07 §9 local validation includes this) |
| Tauri shell/exec/filesystem plugin or any arbitrary command-execution capability | escape hatch bypassing every rule above | L2 Tauri capabilities (§5) + L4 audit |
| Collecting credentials, private keys, unrelated sensitive data; undisclosed telemetry | privacy + scope | L1 (no code path) + L4 (dependency + field audits; document 05 allowlist) |

Seccomp is **deferred and must not be introduced as a v1.0.0
requirement** (work order §3.3; WO-02 Layer 5 unchanged).

## 3. Layered enforcement (five layers, binding)

**L1 — structural (compile time).** Providers implement a trait with no
mutation surface [REPO]; D-Bus access is mediated exclusively through
restricted proxy types whose public API exposes only allow-listed
operations (§4): a prohibited operation is not "forbidden but
reachable" — it is unrepresentable in provider code. File access in
providers is read-only with injectable roots [REPO]. No process
spawning exists in application crates [REPO].

**L2 — Tauri capability configuration (deny-by-default).** §5.

**L3 — invariant tests (executed proof).** The before/after
network-state comparison around every provider and the monitor
(document 07 §7): observable state must be byte-identical (modulo
normalization). Runs per-commit in the sandbox netns harness and per-
checkpoint on the Ubuntu 24.04 host (OD-10). "Prove it, don't promise
it."

**L4 — review gates (static audits in the validation gate).** §5.

**L5 — optional hardening (post-1.0.0, not required).** Seccomp
write-syscall filtering on provider threads: deferred (work order).

## 4. The D-Bus allow-list (concrete, reviewable, deny-by-default)

Design rule (Q-1 resolved): **no D-Bus member is allowed because its
name sounds observational.** Each allowed operation is listed with its
actual semantics at the granularity of **service name, object path (or
path pattern), interface, member (property or method), and access
operation**. Anything not listed — any service, path, interface,
property, or method absent from the tables below — is prohibited by
construction.

Failure and permission behavior, common to every row: a denied operation
records `permissionDenied { mechanism: "dbus-denied" }` for the affected
section; a failed operation records `Failed { error }`; either way the
section reports an **evidence limitation**, never a network-absence
claim (§7; document 02 §3 — binding).

### 4.1 Generic primitives (the only raw D-Bus operations the connection
wrapper exposes — each scoped, none blanket)

| Primitive | Scope restriction | Status |
|---|---|---|
| `org.freedesktop.DBus.Properties.Get(service, path, interface, property)` | permitted **only** for the exact (service, object-path-pattern, interface, property) rows of §4.2–4.3; a `Get` naming any other tuple is denied by the wrapper | allowed (scoped) |
| `org.freedesktop.DBus.Properties.GetAll(service, path, interface)` | **no interface is approved for `GetAll` in v1.0.0**: every candidate interface carries properties outside the reviewed observation scope (e.g. NetworkManager Manager's `GlobalDnsConfiguration`, Device's `Udi`/`Vendor`/`Product`, resolved Manager's `Transactions`), so a blanket read cannot be bounded to the allow-list. All property access is **named `Get`** against the tables. A future amendment may approve `GetAll` for one exact interface only with a row-by-row review of every property that interface exposes | **prohibited (v1.0.0; amendment path stated)** |
| `org.freedesktop.DBus.ObjectManager.GetManagedObjects` | returns every property of every interface of every managed object — unboundable to the allow-list; enumeration uses the service-specific methods and object-path properties in §4.2 instead | **prohibited (v1.0.0; amendment path as above)** |
| `org.freedesktop.DBus.AddMatch(rule)` | signal subscription only, and only the exact signals enumerated in §4.2–4.3 | allowed (scoped) |
| `org.freedesktop.DBus.NameHasOwner` / `ListNames` (bus driver) | capability probing only (is the service running?) — bus-driver queries, no service state | allowed |
| `org.freedesktop.DBus.Properties.Set` | mutation | **prohibited** |
| `org.freedesktop.DBus.Introspect`; any method member not in the tables below | introspection unnecessary (statically generated clients); unlisted members out of scope by definition | **prohibited** |

Runtime introspection is unnecessary (statically generated clients) and
`Introspect` is not used.

### 4.2 NetworkManager (`org.freedesktop.NetworkManager`) — allowed operations

Treatment of the sensitive member classes: **NetworkManager settings**
are read only via `Settings.Connection.GetSettings` (secrets-free by
NM's own semantics — the row below); **NetworkManager secrets**
(`Settings.Connection.GetSecrets`) are prohibited outright (§2); device
and connection properties follow the rows below; signals are
receive-only subscriptions.

| Object path (pattern) | Interface | Member | Operation | Purpose (observation requirement) | Data classification |
|---|---|---|---|---|---|
| `/org/freedesktop/NetworkManager` | `org.freedesktop.NetworkManager` | `State`, `Version`, `Connectivity`, `NetworkingEnabled`, `WirelessEnabled`, `WwanEnabled`, `Metered`, `Startup` | Get (each) | manager overview; `Connectivity` is NM's **cached** value (NM's own past checks produced it — we never trigger one) | public-observable (enums, bools, strings) |
| same | same | `Devices`, `AllDevices`, `ActiveConnections`, `PrimaryConnection`, `PrimaryConnectionType` | Get (each) | object-path handles for device/connection enumeration | public-observable (object paths, type strings) |
| same | same | `GetDevices`, `GetAllDevices` | method | device enumeration (returns object paths only — pure enumeration, no state change) | public-observable |
| same | same | `StateChanged`, `DeviceAdded`, `DeviceRemoved`, `PropertiesChanged` | AddMatch (signals) | change feed for re-observation; notifications only, change nothing | — |
| `/org/freedesktop/NetworkManager/Devices/{N}` (paths from `Devices`/`AllDevices`/`DeviceAdded`) | `org.freedesktop.NetworkManager.Device` | `DeviceType`, `Interface`, `IpInterface`, `State`, `Driver`, `Real`, `Metered` | Get (each) | per-device observed facts (device-mapping section) | public-observable |
| same | same | `HwAddress` | Get | device hardware address | **privacy-noted** (MAC — document 05 §9.1 redaction applies at export) |
| same | same | `ActiveConnection` | Get | device ↔ active-connection mapping | public-observable (object path) |
| same | same | `Ip4Config`, `Ip6Config`, `Dhcp4Config`, `Dhcp6Config` | Get (each) | correlation facts (which config object is bound). The config objects' **own** properties are **not** in the v1.0.0 allow-list: addresses are netlink-authoritative (document 03 §5) and no observation requirement names them | public-observable (object paths) |
| same | `org.freedesktop.NetworkManager.Device.Wireless` | `Wireless`, `WirelessCapabilities` | Get (each) | Wi-Fi capability facts | public-observable |
| same | same | `AccessPoints`, `LastScan` | Get (each) | the **current** scan-list handle; no scan is ever requested (§2: `RequestScan` prohibited); `LastScan` timestamps NM's last scan, not ours | object paths / timestamp |
| `/org/freedesktop/NetworkManager/AccessPoint/{N}` (paths from `AccessPoints`) | `org.freedesktop.NetworkManager.AccessPoint` | `Ssid`, `Strength`, `Flags`, `WpaFlags`, `RsnFlags`, `Frequency`, `Mode` | Get (each) | Wi-Fi details (current scan list only) | `Ssid` **privacy-noted** (wireless network name — never logged, never exported; document 05 §3/§9.1); others public-observable |
| `/org/freedesktop/NetworkManager/ActiveConnection/{N}` (paths from `ActiveConnections`) | `org.freedesktop.NetworkManager.ActiveConnection` | `Connection`, `Devices`, `State`, `Type`, `Vpn`, `Ip4Config`, `Ip6Config`, `Master`, `SpecificObject` | Get (each) | active-connection facts — the VPN evidence source (document 03 §12); `Ip4Config`/`Ip6Config` are correlation object paths as above | public-observable (object paths, enums); `Type`/`Vpn` carry the VPN classification |
| `/org/freedesktop/NetworkManager/Settings/{N}` (paths from ActiveConnection `Connection`) | `org.freedesktop.NetworkManager.Settings.Connection` | `GetSettings` | method | the connection's **secrets-free** settings (NM's own semantics; audited); the credential-bearing sibling `GetSecrets` is prohibited (§2) | settings bundle, no secrets |
| per-object, all read interfaces above | each interface read via Get | `PropertiesChanged` | AddMatch (signal) | per-object change notification | — |

### 4.3 systemd-resolved (`org.freedesktop.resolve1`) — allowed operations (properties and signals only; **every method is prohibited**)

| Object path (pattern) | Interface | Member | Operation | Purpose (observation requirement) | Data classification |
|---|---|---|---|---|---|
| `/org/freedesktop/resolve1` | `org.freedesktop.resolve1.Manager` | `DNS`, `DNSEx`, `Domains`, `LLMNR`, `MulticastDNS`, `DNSOverTLS`, `DNSSEC`, `ResolvConf`, `DNSStubListener`, `FallbackDNS`, `CacheStatistics` | Get (each) | global resolver configuration as resolved holds it (the `DnsObservation` source — document 03 §8) | resolver configuration (addresses and domain strings — the product's subject matter; search domains can reveal organization) |
| same | same | `DNSSECNegativeTrustAnchors` | Get | DNSSEC negative-trust configuration (part of the resolver configuration observation) | resolver configuration; domain strings (organization-revealing — noted) |
| same | same | `PropertiesChanged` | AddMatch (signal) | change notification | — |
| `/org/freedesktop/resolve1/link/{N}` | `org.freedesktop.resolve1.Link` | `DNS`, `DNSEx`, `Domains`, `LLMNR`, `MulticastDNS`, `DNSOverTLS`, `DNSSEC`, `CurrentDNSServer`, `ScopesMask` | Get (each) | per-link resolver configuration | resolver configuration (as above) |
| same | same | `PropertiesChanged` | AddMatch (signal) | change notification | — |

### 4.4 systemd-networkd (`org.freedesktop.network1`) — no allowed operations

systemd-networkd is an anticipated optional provider (OD-02; document
02 §11) whose concrete observation sections are a Prompt 05 finding
(WO-02 §4). **No networkd service, object path, interface, property,
method, or signal is allowed by this specification.** Deny-by-default
applies to anticipation itself: any future networkd D-Bus access
requires an explicit allow-list amendment — rows with the same
per-operation justification as §4.2–4.3 — through review. Nothing is
pre-approved.

### 4.5 Methods that look observational but are not (prohibited with reason)

`CheckConnectivity` (forces NM to probe — network traffic we did not
passively observe); `RequestScan` (emits management frames);
`FlushCaches` (mutates resolver state); `ResolveHostname`/
`ResolveRecord`/`ResolveService`/`ResolveAddress` (each **generates**
DNS traffic — an active query, not an observation; excluded from the
v1.0.0 passive posture, document 03 §8). The same rule governs any
future member: classification is by **documented semantics, never by
name**; a member that cannot be confidently classified as read-only is
excluded, and the corresponding observation is reported `unsupported`
or `unavailable` — never fabricated.

### 4.6 Enforcement mechanics (L1)

One `dbus-client` module per service provider exposes typed functions
(`nm_devices()`, `nm_connection_settings(path)`,
`resolved_link_dns(path)`, …) whose implementations call only the rows
above. The restricted wrapper:

- exposes no free-form `(service, path, interface, member)` call and no
  generic `call(member, …)` escape hatch — every operation is a fixed
  function bound to an exact allow-list row;
- implements the §4.1 primitives with their scope checks (a `Get` whose
  tuple matches no row is denied inside the wrapper);
- is mechanically cross-checked by the L4 audit (§5, check 5): the
  module's member, interface, and object-path strings must be a subset
  of the tables, which live in one reviewed source-of-truth file the
  script consumes; `GetAll` and `GetManagedObjects` additionally have
  explicit negative assertions (no call sites anywhere in provider
  code).

Negative tests (document 07 §4): mock services that **offer** prohibited
members, unlisted properties, `Properties.GetAll`, and
`ObjectManager.GetManagedObjects` — the restricted proxy must be proven
never to call them; unknown operations are unreachable by construction,
which the audit demonstrates.

## 5. Tauri capabilities and command exposure (L2)

- Deny-by-default (OD-01/OD-03): the capability set grants **only**
  `core:event:allow-listen` and `core:event:allow-unlisten`, the two
  frontend permissions needed to consume the backend-owned live-event
  channel. The frontend cannot emit on that channel, so it cannot spoof
  backend-owned monitor-health facts. No `shell`, no `fs`, no `http`, no
  `clipboard`, no `global-shortcut`, no `notification`, no `dialog` (file
  import uses the standard HTML file input / drag-and-drop in the webview
  — no native-dialog plugin needed).
- Command surface: the four IPC commands of document 04 §9
  (`get_snapshot`, `get_topology`, `subscribe_watch`,
  `unsubscribe_watch`) and nothing else. Every command is registered
  explicitly; Tauri exposes nothing unregistered (deny-by-default
  principle applied to commands).
- CSP: `default-src 'self'`; no remote origins, no inline scripts
  beyond the framework's build output; no network fetch surface at all.
- The capability file(s) are **versioned review artifacts**: the audit
  step diffs them against the reviewed allow-list; any addition fails
  the gate until this document's table is amended through review.

## 6. Static audit gates (L4 — specified, not yet implemented)

A `scripts/check-readonly.sh` step in the validation gate (design;
added at implementation time), each check mechanical and grep/parse-
based in the spirit of the existing gate steps [REPO —
`scripts/test-all.sh`]:

1. `tauri` appears in no manifest outside `apps/desktop`.
2. No write-mode file API in provider/engine/core crates (audit
   `OpenOptions`, `.write(`, `create_new`, truncate flags; test code
   excluded by path).
3. No process spawning (`std::process`, `Command`) outside tests.
4. Netlink codec construction limited to dump/subscribe message types
   (existing review-gate practice [REPO — WO-02 §5.1], mechanized).
5. D-Bus member, interface, and object-path strings in provider modules
   ⊆ the allow-list tables (§4.1–4.4); the tables live in one reviewed
   source-of-truth file the script consumes; `Properties.GetAll` and
   `ObjectManager.GetManagedObjects` carry explicit negative assertions
   (no call sites in provider code — §4.1).
6. No dependency on any HTTP/TLS/network client crate (the only
   socket-creating crates are `netlink` and the D-Bus client crate).
7. Tauri capability permissions ⊆ the allow-list of §5.
8. Log/diagnostics field names ⊆ the vocabulary of document 05 §3.

## 7. Runtime behavior when permissions or providers are unavailable

- Access denied ⇒ explicit `permissionDenied` outcomes (document 02
  §3); the app continues with remaining providers; the UI presents an
  evidence limitation ("resolver configuration not observed:
  permission denied"), never a network claim, never a crash.
- No elevation attempts, no prompting for privilege, no retries beyond
  the bounded backoff (document 02 §7).
- A failed/absent provider never implies its entities' absence
  (documents 02 §1, 03 §1 — binding).
- The window in front of the user always says what was **not**
  observed and why (capability metadata is in every document —
  document 04 §2).

## 8. Decision summary

| Decision | Source | Status |
|---|---|---|
| Five-layer enforcement; seccomp deferred, not a v1.0.0 requirement | WO-02 ch. 5; work order §3.3 | approved-derived |
| Restricted-proxy D-Bus design; per-operation allow-list (service/path/interface/member/operation) with named-`Get`-only property access; `GetAll` and `GetManagedObjects` excluded from v1.0.0 | Q-1; R-03; work order §3.3; Prompt 03-FC §5 (FC-02) | RD-25 |
| Properties/signals-only posture for resolved; active query methods prohibited | work order §3.3/§3.4 | approved-derived |
| `CheckConnectivity`/`RequestScan` prohibited despite observational names | work order §3.3 ("not safe merely because observational") | approved-derived |
| Capability set = event listen/unlisten only; no event emit, dialog, shell, or fs plugins | OD-01/OD-03; WO-02 Table 5.1; WO-05 monitor-health trust boundary | RD-26 |
| `scripts/check-readonly.sh` audit step (8 checks) | WO-02 Layer 4; work order §3.3 | RD-27 |
| Before/after netns + host state-comparison invariant test | WO-02 Layer 3; OD-10 | approved-derived (design in document 07) |
