# Araiguma — network discovery notes

What each discovery source provides, what it cannot provide, and the
kernel behaviors the implementation depends on. Everything here was
verified against a live kernel (5.10) inside transient network namespaces
unless marked otherwise.

## Source capabilities

### rtnetlink (primary, priority 0)

| Observation | Netlink source |
|---|---|
| Interface enumeration, ifindex, name | `RTM_GETLINK` dump |
| Interface classification | `IFLA_LINKINFO/IFLA_INFO_KIND` (bridge, dummy, wireguard, vlan, veth, tunnels, ...) |
| Fallback classification | ARPHRD link-layer type (loopback 772, ethernet 1, wireless 801–803, none 65534) |
| Admin state | `IFF_UP` in link flags |
| Oper state | `IFLA_OPERSTATE` (RFC 2863) |
| MAC, MTU, txqueuelen | `IFLA_ADDRESS`, `IFLA_MTU`, `IFLA_TXQLEN` |
| Bridge/bond master | `IFLA_MASTER` (exposed by the crate under its modern name `Controller`) |
| Parent link (vlan, tunnels) | `IFLA_LINK` |
| Addresses + prefix, scope, flags, label | `RTM_GETADDR` dump |
| Routes: all tables, protocol, scope, type, metric, flags | `RTM_GETROUTE` dump (v4 and v6) |
| Route table incl. extended ids | `RTA_TABLE` (u32) preferred over the u8 `rtm_table` compat field |

Netlink strictly observes more than the fallback sources, so it wins every
merge conflict.

### sysfs (fallback, priority 1, injectable root)

| Observation | Path |
|---|---|
| Interface enumeration | `/sys/class/net/<if>/` |
| ifindex, MTU, MAC, txqueuelen | `ifindex`, `mtu`, `address`, `tx_queue_len` |
| Oper state | `operstate` |
| Admin state | `flags` (hex, `IFF_UP` 0x1) |
| Bridge detection | presence of `bridge/` |
| Wi-Fi detection | presence of `wireless/` or `phy80211/` (beats ARPHRD) |
| Bridge/bond master | `master` symlink basename → ifindex resolution |
| Classification fallback | `type` (ARPHRD) — with hard limits, see below |

### procfs (fallback, part of the sysfs provider)

| Observation | Path |
|---|---|
| IPv6 addresses + scope + flags | `/proc/net/if_inet6` |
| Main-table IPv4 routes | `/proc/net/route` |

`/proc/net/ipv6_route` also exists but is not parsed (documented possible
future work); IPv6 routes therefore come only from netlink today.

## Known kernel behaviors the code depends on

1. **Dummy and loopback report operstate UNKNOWN even when up.** An up dummy
   with an address is fully functional yet `operstate` is `unknown`. The UI
   displays this honestly ("unknown state") instead of dressing it up as up.
   `lowerLayerDown` and `notPresent` are explicitly non-operational in every
   view ("lower layer down" / "not present" with the down tone).
2. **A full route dump includes the local table (255)** — local, broadcast,
   and anycast entries appear alongside main-table routes. The model keeps
   them (with `table: 255`); presentation filters by default but never
   deletes.
3. **`/proc/net/route` contains only the main table.** procfs routes are
   recorded with `table = 254` plus a note stating that the table is
   implied, so they deduplicate against netlink's explicit main-table
   routes.
4. **`/proc/net/route` values are little-endian hex** (gateway `0100A8C0`
   is 192.168.0.1), including the mask column (`00FFFFFF` is /24). The
   surviving WO-01 fixture script had written masks big-endian; corrected
   during reconstruction.
5. **`/proc/net/if_inet6` columns**: address (big-endian hex), ifindex,
   prefix, scope, flags (hex), name. Scope values are IPv6 address scopes:
   `0x00` global, `0x10` node-local (host), `0x20` link-local, `0x40`
   site-local — these are **not** `RT_SCOPE_*` values. Flags carry `IFA_F_*`
   bits (`0x80` permanent, `0x40` tentative, ...).
6. **An absent `RTA_PRIORITY` means effective metric 0.** Verified live:
   a default route created by `ip route add default via ...` carries no
   `RTA_PRIORITY` while `/proc/net/route` reports metric 0 for the same
   route. The netlink translation records `Some(0)`, keeping cross-source
   deduplication correct.
7. **sysfs cannot distinguish virtual Ethernet-like devices from physical
   Ethernet**: dummy, veth, vlan, macvlan share ARPHRD 1 with real NICs.
   sysfs-only classification therefore reports `Ethernet` with a caveat
   note, and the provider runs in the `degraded` state. Netlink
   classification (priority 0) wins whenever available.
8. **ARPHRD 65534 (NONE)** devices (wireguard, some tunnels) are
   unclassifiable from sysfs alone → `Virtual` + caveat note.
9. **Interface classification precedence**: netlink `IFLA_INFO_KIND` >
   sysfs wireless/bridge directories > ARPHRD. An `IFLA_INFO_KIND` string
   that the model does **not** map is stronger evidence than ARPHRD: the
   device is a specific (virtual) kind we do not recognize, so the raw
   string is kept in `kind_detail`, the classification stays `Other` with
   an explicit "classification uncertain" note, and the ARPHRD fallback is
   deliberately not consulted (an unrecognized virtual device whose
   hardware type is ARPHRD_ETHER would otherwise be confidently
   misclassified as Ethernet). The fallback applies only when the kernel
   exposed no kind string at all.
10. **MAC address observation is three-state** (schema v2). Inside a
    successfully dumped link message, an absent or empty `IFLA_ADDRESS` is
    the kernel stating the device has **no** hardware address (wireguard,
    ARPHRD_NONE devices) — an observed absence, not missing data. From
    sysfs, an existing-but-empty `address` file is an observed absence; a
    missing file means not observed. Verified live: loopback reports its
    all-zero address as a real value (`Present("00:00:00:00:00:00")` — the
    kernel's own claim, reported as-is), wireguard reports none.
11. **Netlink receive-buffer overrun** (`ENOBUFS`): when notifications
    arrive faster than the socket drains, the kernel drops messages and
    the next receive returns ENOBUFS, which the netlink codec surfaces as
    an `NLMSG_OVERRUN` message. Verified live inside a netns by starving
    the reader with a 4000-operation batch: the monitor reports
    `Resynchronized` (dropped events are unobserved) and the subscription
    continues delivering fresh events afterwards.
12. **This kernel (5.10) does not install the connected route for an
    address on a DOWN interface**: `ip route add default via X dev Y` fails
    with "Nexthop has invalid gateway" unless Y is up (or the address is
    otherwise on-link). Discovered because a netns test violated this and
    its failure was hidden by the old re-exec pattern (see the WO-02
    correction closeout); tests now bring interfaces up before route
    setup.
13. **Multipath route decoding** (`RTA_MULTIPATH`): each hop carries its
    own gateway, output interface, per-hop flags, and `rtnh_hops`, which
    the kernel stores as weight − 1 (iproute2 displays `rtnh_hops + 1`;
    verified live inside a netns with `ip route add ... nexthop ... weight
    2/3`). `/proc/net/route` shows only the FIRST hop of a multipath
    route (verified live), so a procfs observation is a partial view of
    the same kernel route — the merge treats overlapping next-hop sets as
    the same route rather than counting it twice (WO-03 M3).
14. **`/proc/net/ipv6_route` format** (WO-03 M4, verified live on kernel
    5.10 and cross-checked with `ip -6 route`): columns
    `dst dst_len src src_len nexthop metric refcnt use flags dev`, every
    numeric column hex except the device; addresses big-endian (unlike
    `/proc/net/route`'s little-endian IPv4). The metric matches the
    netlink `RTA_PRIORITY` (connected/addrconf routes 256 =
    `IP6_RT_PRIO_ADDRCONF`, user-added 1024 = `IP6_RT_PRIO_USER`), so
    cross-provider dedup aligns. The file spans MULTIPLE tables (main,
    local, ...) with NO table column — the table is not observable and
    the model records `None` with a note. It also contains the kernel's
    unreachable-default sentinels (`::/0`, metric `ffffffff`, flags
    `RTF_NONEXTHOP|RTF_REJECT`) — printed twice on this kernel and NOT
    included in netlink dumps; they are kept as honest procfs-only
    observations. iproute2 omits `proto boot` from its display (boot is
    the default display protocol), so a protocol cross-check stays at the
    model level.

## Not observable (today, honestly reported as not observed)

- **WireGuard peers and endpoints** — requires generic-netlink `wg` queries
  with elevated privileges; out of scope.
- **Internet connectivity** — a default route is not proof of an upstream.
  The UI never claims connectivity.
- **Which interface is actually used for a destination** — that is a routing
  *decision*, not an observation; it would require an active `ip route get`
  probe (not performed).
- **DNS resolver state** — future systemd-resolved provider.
- **Per-hop traffic distribution of a multipath route** — the kernel
  reports the hop set and weights (represented in full since WO-03 M3),
  but not which hop carried a given packet; that would require active
  probing.
- **A veth peer living in another network namespace** — the kernel
  reports the peer's ifindex (represented since WO-03 M2); when the peer
  is not part of the same observation it stays unknown, and its ifindex
  is not assumed to be visible here.

## Permissions

- rtnetlink dumps and multicast subscriptions work unprivileged.
- sysfs/procfs paths used are world-readable.
- Nothing in the application requires root, capabilities, or a privileged
  daemon. If a source is unavailable, the provider reports it and the
  fallback carries what it can.

## How future providers fit

`DiscoveryProvider` (crates/core) is the only integration point: implement
`observe()` returning per-section results at a declared priority. A
NetworkManager provider would report its own capabilities (connection
state, Wi-Fi details) as additional observations merged at its priority;
nothing today assumes two providers or hardcodes their identities in the
merge.
