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

## 4. The D-Bus allow-list (concrete and reviewable)

Design rule (Q-1 resolved): **no D-Bus member is allowed because its
name sounds observational.** Each allowed member is listed with its
actual semantics; anything not listed is prohibited by construction.
The generic primitives are narrowed first, then members:

**Generic primitives (the only raw D-Bus operations the connection
wrapper exposes):**

| Primitive | Semantics | Status |
|---|---|---|
| `org.freedesktop.DBus.Properties.Get` / `GetAll` | read a property's cached value; no side effects | allowed |
| `org.freedesktop.DBus.ObjectManager.GetManagedObjects` | enumerate objects + properties in one read | allowed |
| `org.freedesktop.DBus.AddMatch` (signals) | subscribe — receiving only | allowed, matched signals only (below) |
| `org.freedesktop.DBus.NameHasOwner` / `ListNames` (bus driver) | capability probing (is the service running?) | allowed |
| `org.freedesktop.DBus.Properties.Set` | mutation | **prohibited** |
| any method member not in the tables below | — | **prohibited** |

Runtime introspection is unnecessary (statically generated clients) and
`Introspect` is not used.

**NetworkManager (`org.freedesktop.NetworkManager`) — allowed members:**

| Member | Kind | Semantics (why it is safe) |
|---|---|---|
| `Devices`, `AllDevices`, `ActiveConnections`, `PrimaryConnection`, `PrimaryConnectionType`, `State`, `Version`, `Connectivity`, `NetworkingEnabled`, `WirelessEnabled`, `WwanEnabled`, `Metered`, `Startup` (Manager properties) | property get | cached state reads; `Connectivity` is NM's **cached** value (NM's own checks produced it — we do not trigger one) |
| `GetDevices`, `GetAllDevices` (Manager methods) | method | return object paths of existing devices — pure enumeration, no state change |
| Device properties: `DeviceType`, `Interface`, `IpInterface`, `State`, `Driver`, `HwAddress`, `Real`, `Metered`, `ActiveConnection`, `Ip4Config`, `Ip6Config`, `Dhcp4Config`, `Dhcp6Config`, `Wireless`, `WirelessCapabilities` | property get | per-device observed facts; `HwAddress` is privacy-noted (document 05) |
| Wireless AP properties (`Ssid`, `Strength`, `Flags`, `WpaFlags`, `RsnFlags`, `Frequency`, `Mode`) via `AccessPoints`/`LastScan` device properties | property get | the **current** scan list only; no scan is requested |
| ActiveConnection properties: `Connection`, `Devices`, `State`, `Type`, `Vpn`, `Ip4Config`, `Ip6Config`, `Master`, `SpecificObject` | property get | active-connection facts (the VPN evidence source — document 03 §12) |
| `Settings.Connection.GetSettings` | method | returns the connection's **secrets-free** settings; the credential-bearing sibling `GetSecrets` is prohibited (§2) |
| Manager signals: `StateChanged`, `DeviceAdded`, `DeviceRemoved`, `PropertiesChanged`; per-object `PropertiesChanged` | subscription | notifications only; they change nothing |

**systemd-resolved (`org.freedesktop.resolve1`) — allowed members
(properties and signals only; every method is prohibited):**

| Member | Kind | Semantics |
|---|---|---|
| Manager properties: `DNS`, `DNSEx`, `Domains`, `LLMNR`, `MulticastDNS`, `DNSOverTLS`, `DNSSEC`, `DNSSECNegativeTrustAnchors`, `ResolvConf`, `DNSStubListener`, `FallbackDNS`, `CacheStatistics` | property get | global resolver configuration as resolved holds it (the `DnsObservation` source — document 03 §8) |
| Link properties (`/org/freedesktop/resolve1/link/_N`): `DNS`, `DNSEx`, `Domains`, `LLMNR`, `MulticastDNS`, `DNSOverTLS`, `DNSSEC`, `CurrentDNSServer`, `ScopesMask` | property get | per-link resolver configuration |
| `PropertiesChanged` signals | subscription | notifications only |

Prohibited-with-reason highlights (the "sounds observational but is
not" class the work order demands be identified): `CheckConnectivity`
(forces NM to probe — network traffic we did not passively observe);
`RequestScan` (emits management frames); `FlushCaches` (mutates
resolver state); `ResolveHostname`/`ResolveRecord`/`ResolveService`/
`ResolveAddress` (each **generates** DNS traffic — an active query, not
an observation; excluded from v1.0.0 passive posture, document 03 §8).

Enforcement mechanics (L1): one `dbus-client` module per service
provider exposes typed functions (`nm_devices()`, `nm_connection_settings(path)`,
`resolved_link_dns(path)`, …) whose implementations call only the
members above; the audit (§5) mechanically cross-checks the module's
member strings against this table. The restricted wrapper exposes no
generic `call(member, …)` escape hatch.

## 5. Tauri capabilities and command exposure (L2)

- Deny-by-default (OD-01/OD-03): the capability set grants **only**
  `core:default` (app/window/webview/event essentials for a local UI).
  No `shell`, no `fs`, no `http`, no `clipboard`, no `global-shortcut`,
  no `notification`, no `dialog` (file import uses the standard HTML
  file input / drag-and-drop in the webview — no native-dialog plugin
  needed).
- Command surface: the three IPC commands of document 04 §9 and
  nothing else. Every command is registered explicitly; Tauri exposes
  nothing unregistered (deny-by-default principle applied to commands).
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
5. D-Bus member strings in provider modules ⊆ the allow-list tables
   (§4); the tables live in one reviewed source-of-truth file the
   script consumes.
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
| Restricted-proxy D-Bus design; semantics-verified member allow-list | Q-1; R-03; work order §3.3 | RD-25 |
| Properties/signals-only posture for resolved; active query methods prohibited | work order §3.3/§3.4 | approved-derived |
| `CheckConnectivity`/`RequestScan` prohibited despite observational names | work order §3.3 ("not safe merely because observational") | approved-derived |
| Capability set = `core:default` only; no dialog/shell/fs plugins | OD-01/OD-03; WO-02 Table 5.1 | RD-26 |
| `scripts/check-readonly.sh` audit step (8 checks) | WO-02 Layer 4; work order §3.3 | RD-27 |
| Before/after netns + host state-comparison invariant test | WO-02 Layer 3; OD-10 | approved-derived (design in document 07) |
