//! Araiguma core: the domain model for network observations.
//!
//! Semantics the entire codebase relies on:
//!
//! * `Option<T>` on a field means **not observed by any provider**. `None` is
//!   never silently interpreted as a negative observation. The one deliberate
//!   exception is [`Route::gateway`], where the kernel itself distinguishes
//!   "no next hop" (on-link route) from "unknown": for routes that made it
//!   into an observation, `None` there means an *observed* absence of a
//!   gateway. See the field documentation.
//! * Every observed item carries `sources`: the providers that observed it,
//!   in merge-priority order (first entry = primary contributor).
//! * Synthetic/test data must be labeled as such by whoever produces it; the
//!   model itself never invents values.

use std::fmt;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

use serde::{Deserialize, Serialize};

/// The provider that produced a piece of network state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ObservationSource {
    /// rtnetlink (the primary structured kernel interface).
    Netlink,
    /// sysfs (`/sys/class/net/...`).
    SysFs,
    /// procfs (`/proc/net/...`).
    ProcFs,
}

/// IP address family.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum AddressFamily {
    Ipv4,
    Ipv6,
}

impl AddressFamily {
    pub fn of(addr: IpAddr) -> Self {
        match addr {
            IpAddr::V4(_) => Self::Ipv4,
            IpAddr::V6(_) => Self::Ipv6,
        }
    }
}

/// A network prefix: base address plus prefix length (e.g. `192.0.2.0/24`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IpNet {
    pub address: IpAddr,
    pub prefix_len: u8,
}

impl IpNet {
    pub fn new(address: IpAddr, prefix_len: u8) -> Self {
        Self {
            address,
            prefix_len,
        }
    }

    /// A default route destination (`0.0.0.0/0` or `::/0`).
    pub fn is_default(&self) -> bool {
        self.prefix_len == 0
    }

    /// Whether `addr` falls inside this prefix (same family only).
    pub fn contains(&self, addr: IpAddr) -> bool {
        match (self.address, addr) {
            (IpAddr::V4(net), IpAddr::V4(a)) => {
                let len = self.prefix_len.min(32);
                let mask = if len == 0 { 0 } else { u32::MAX << (32 - len) };
                (u32::from(net) & mask) == (u32::from(a) & mask)
            }
            (IpAddr::V6(net), IpAddr::V6(a)) => {
                let len = self.prefix_len.min(128);
                let mask = if len == 0 {
                    0
                } else {
                    u128::MAX << (128 - len)
                };
                (u128::from(net) & mask) == (u128::from(a) & mask)
            }
            _ => false,
        }
    }

    /// The IPv4 default prefix.
    pub fn ipv4_default() -> Self {
        Self::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), 0)
    }

    /// The IPv6 default prefix.
    pub fn ipv6_default() -> Self {
        Self::new(IpAddr::V6(Ipv6Addr::UNSPECIFIED), 0)
    }
}

impl fmt::Display for IpNet {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}/{}", self.address, self.prefix_len)
    }
}

/// Classification of a network interface.
///
/// `Ethernet` covers physical Ethernet-style links. Note that sysfs alone
/// cannot distinguish some virtual kinds from real Ethernet (identical
/// ARPHRD type); when only sysfs observed an interface, the classification
/// carries that caveat in the provider's notes, and netlink's classification
/// wins whenever it is available.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum InterfaceKind {
    Loopback,
    Ethernet,
    Wireless,
    Bridge,
    Bond,
    Dummy,
    Wireguard,
    /// Tunnel-style virtual interfaces (wireguard is separate).
    Tunnel,
    /// veth pair endpoint.
    Veth,
    Vlan,
    /// Virtual interface whose exact kind the source could not classify.
    Virtual,
    /// Unclassified raw kind; see `kind_detail` when present.
    Other,
}

impl InterfaceKind {
    /// Map a kernel `IFLA_INFO_KIND` string (netlink link kind) to a
    /// classification. Returns `None` for unrecognized kinds (the caller
    /// keeps the raw string in `kind_detail`).
    pub fn from_link_kind(kind: &str) -> Option<Self> {
        Some(match kind {
            "bridge" => Self::Bridge,
            "bond" => Self::Bond,
            "dummy" => Self::Dummy,
            "wireguard" => Self::Wireguard,
            "vlan" => Self::Vlan,
            "veth" => Self::Veth,
            "vxlan" | "geneve" | "gretap" | "gretap6" | "gre" | "gre6" | "ipip" | "sit"
            | "ip6tnl" | "vti" | "vti6" | "erspan" | "tun" | "tap" => Self::Tunnel,
            "macvlan" | "ipvlan" | "macvtap" | "ipvtap" => Self::Virtual,
            _ => return None,
        })
    }

    /// Map an ARPHRD hardware type (sysfs `type` file) to a classification.
    ///
    /// Caveat: ARPHRD 1 (Ethernet) is also used by dummy, veth, vlan, and
    /// macvlan devices; sysfs alone cannot separate them.
    pub fn from_arphrd(arphrd: u16) -> Self {
        match arphrd {
            772 => Self::Loopback,
            801..=803 => Self::Wireless,
            65534 => Self::Virtual,
            _ if arphrd == 1 => Self::Ethernet,
            _ => Self::Other,
        }
    }
}

/// Interface operational state (RFC 2863 ifOperStatus values).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum OperState {
    Up,
    Down,
    Unknown,
    Testing,
    Dormant,
    NotPresent,
    LowerLayerDown,
}

impl OperState {
    /// From a sysfs `operstate` string.
    pub fn from_sysfs_str(s: &str) -> Option<Self> {
        Some(match s.trim() {
            "up" => Self::Up,
            "down" => Self::Down,
            "unknown" => Self::Unknown,
            "testing" => Self::Testing,
            "dormant" => Self::Dormant,
            "notpresent" => Self::NotPresent,
            "lowerlayerdown" => Self::LowerLayerDown,
            _ => return None,
        })
    }

    /// From a raw netlink/RFC 2863 value (1-7).
    pub fn from_raw(v: u8) -> Option<Self> {
        Some(match v {
            1 => Self::Up,
            2 => Self::Down,
            3 => Self::Testing,
            4 => Self::Unknown,
            5 => Self::Dormant,
            6 => Self::NotPresent,
            7 => Self::LowerLayerDown,
            _ => return None,
        })
    }
}

/// Interface administrative state (IFF_UP).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum AdminState {
    Up,
    Down,
}

/// Scope of an address.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum AddressScope {
    Global,
    Site,
    Link,
    Host,
}

impl AddressScope {
    /// From a netlink `RT_SCOPE_*` raw value (addresses use 0/200/253/254).
    pub fn from_route_scope_raw(v: u8) -> Option<Self> {
        Some(match v {
            0 => Self::Global,
            200 => Self::Site,
            253 => Self::Link,
            254 => Self::Host,
            _ => return None,
        })
    }

    /// From the `/proc/net/if_inet6` scope column: node-local 0x10,
    /// link-local 0x20, site-local 0x40, global 0x00 (verified on a live
    /// kernel; these are IPv6 address scopes, not RT_SCOPE values).
    pub fn from_if_inet6_scope_raw(v: u8) -> Option<Self> {
        Some(match v {
            0x00 => Self::Global,
            0x10 => Self::Host,
            0x20 => Self::Link,
            0x40 => Self::Site,
            _ => return None,
        })
    }
}

/// Routing protocol that installed a route (RTPROT_*).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum RouteProtocol {
    Unspec,
    Redirect,
    Kernel,
    Boot,
    Static,
    Ra,
    Dhcp,
    /// A recognized-but-unmapped protocol id (kept verbatim, never guessed).
    Other(u8),
}

impl RouteProtocol {
    pub fn from_raw(v: u8) -> Self {
        match v {
            0 => Self::Unspec,
            1 => Self::Redirect,
            2 => Self::Kernel,
            3 => Self::Boot,
            4 => Self::Static,
            9 => Self::Ra,
            16 => Self::Dhcp,
            other => Self::Other(other),
        }
    }
}

/// Route scope (RT_SCOPE_*).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum RouteScope {
    /// RT_SCOPE_UNIVERSE — "global" routes (the kernel's own naming).
    Universe,
    Site,
    Link,
    Host,
    Nowhere,
    /// A recognized-but-unmapped scope id (kept verbatim, never guessed).
    Other(u8),
}

impl RouteScope {
    pub fn from_raw(v: u8) -> Self {
        match v {
            0 => Self::Universe,
            200 => Self::Site,
            253 => Self::Link,
            254 => Self::Host,
            255 => Self::Nowhere,
            other => Self::Other(other),
        }
    }
}

/// Route type (RTN_*).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum RouteType {
    Unspec,
    Unicast,
    Local,
    Broadcast,
    Anycast,
    Multicast,
    Blackhole,
    Unreachable,
    Prohibit,
    Throw,
    Nat,
    Xresolve,
    /// A recognized-but-unmapped type id (kept verbatim, never guessed).
    Other(u8),
}

impl RouteType {
    pub fn from_raw(v: u8) -> Self {
        match v {
            0 => Self::Unspec,
            1 => Self::Unicast,
            2 => Self::Local,
            3 => Self::Broadcast,
            4 => Self::Anycast,
            5 => Self::Multicast,
            6 => Self::Blackhole,
            7 => Self::Unreachable,
            8 => Self::Prohibit,
            9 => Self::Throw,
            10 => Self::Nat,
            11 => Self::Xresolve,
            other => Self::Other(other),
        }
    }
}

/// The main routing table (RT_TABLE_MAIN). `/proc/net/route` implicitly
/// exposes only this table.
pub const RT_TABLE_MAIN: u8 = 254;

/// The local routing table (RT_TABLE_LOCAL), present in full netlink dumps.
pub const RT_TABLE_LOCAL: u8 = 255;

/// Observation of an interface's hardware (MAC) address — three states the
/// presentation must never conflate (WO-02 correction §5):
///
/// * [`MacObservation::Present`] — a hardware address was observed.
/// * [`MacObservation::Absent`] — the interface was **observed to have no
///   hardware address** (e.g. WireGuard and other `ARPHRD_NONE` devices:
///   netlink sends no `IFLA_ADDRESS`, sysfs' `address` file is empty).
/// * [`MacObservation::NotObserved`] — no provider obtained the address
///   (missing sysfs file, or no provider covered the interface). A
///   provider that failed wholesale is expressed at the section level,
///   which this per-field model deliberately does not duplicate.
///
/// Serialized as the plain address string when present (byte-identical to
/// the pre-v2 representation, so existing documents with real addresses
/// parse unchanged) and as the explicit sentinels `"absent"` /
/// `"notObserved"` otherwise. A JSON `null` is **rejected**: schema-v1
/// `null` was ambiguous between "no hardware address" and "not observed",
/// and ambiguous data is rejected rather than silently assigned a meaning.
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash)]
pub enum MacObservation {
    /// A normalized `aa:bb:cc:dd:ee:ff` address (lowercase; the kernel may
    /// expose longer hardware addresses, e.g. 20-byte IPoIB — any number
    /// of colon-separated hex byte pairs is valid).
    Present(String),
    /// Observed: this interface has no hardware address.
    Absent,
    /// The hardware address was not observed.
    #[default]
    NotObserved,
}

impl MacObservation {
    /// Whether a concrete hardware address was observed.
    pub fn present(&self) -> Option<&str> {
        match self {
            MacObservation::Present(address) => Some(address),
            _ => None,
        }
    }
}

impl std::fmt::Display for MacObservation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            MacObservation::Present(address) => write!(f, "{address}"),
            MacObservation::Absent => write!(f, "absent"),
            MacObservation::NotObserved => write!(f, "notObserved"),
        }
    }
}

/// Structural check for a hardware address: one or more pairs of hex
/// digits separated by single colons (case-insensitive).
fn is_mac_shaped(value: &str) -> bool {
    let mut pairs = value.split(':');
    let mut count = 0usize;
    for pair in pairs.by_ref() {
        if pair.len() != 2 || !pair.bytes().all(|b| b.is_ascii_hexdigit()) {
            return false;
        }
        count += 1;
    }
    count >= 1
}

impl Serialize for MacObservation {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_string())
    }
}

impl<'de> Deserialize<'de> for MacObservation {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct MacVisitor;
        impl serde::de::Visitor<'_> for MacVisitor {
            type Value = MacObservation;
            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(
                    f,
                    "a hardware address (\"aa:bb:cc:dd:ee:ff\") or the sentinels \
                     \"absent\"/\"notObserved\"; JSON null is rejected because schema-v1 \
                     null could not distinguish an observed absence from a not-observed \
                     address"
                )
            }
            fn visit_str<E: serde::de::Error>(self, raw: &str) -> Result<Self::Value, E> {
                match raw {
                    "absent" => Ok(MacObservation::Absent),
                    "notObserved" => Ok(MacObservation::NotObserved),
                    _ => {
                        if is_mac_shaped(raw) {
                            // Canonicalize to lowercase so equivalent
                            // documents compare equal.
                            Ok(MacObservation::Present(raw.to_ascii_lowercase()))
                        } else {
                            Err(E::invalid_value(
                                serde::de::Unexpected::Str(raw),
                                &"a hardware address (\"aa:bb:cc:dd:ee:ff\") or the \
                                 sentinels \"absent\"/\"notObserved\"",
                            ))
                        }
                    }
                }
            }
            fn visit_unit<E: serde::de::Error>(self) -> Result<Self::Value, E> {
                Err(E::custom(
                    "macAddress must not be null: schema-v1 null was ambiguous between \
                     \"observed no hardware address\" and \"not observed\"; use the \
                     sentinels \"absent\"/\"notObserved\" (schema v2)",
                ))
            }
        }
        deserializer.deserialize_str(MacVisitor)
    }
}

/// An observed network interface.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Interface {
    pub ifindex: u32,
    pub name: String,
    pub kind: InterfaceKind,
    /// Raw kernel kind string (netlink `IFLA_INFO_KIND`), e.g. "vxlan",
    /// when the source exposed one.
    pub kind_detail: Option<String>,
    /// Operational state; `None` = not observed.
    pub oper_state: Option<OperState>,
    /// Administrative state (IFF_UP); `None` = not observed.
    pub admin_state: Option<AdminState>,
    /// Hardware (MAC) address; see [`MacObservation`] for the three
    /// observed states (schema v2 — v1's ambiguous `null` is rejected).
    pub mac_address: MacObservation,
    pub mtu: Option<u32>,
    pub tx_queue_len: Option<u32>,
    /// Bridge/bond master ifindex (netlink `IFLA_MASTER`, sysfs `master`).
    pub master_ifindex: Option<u32>,
    /// Parent/underlying ifindex (netlink `IFLA_LINK`: vlan parent, tunnel
    /// underlying device). For veth devices the kernel reports the PEER
    /// here — the translator moves it to `peer_ifindex` for interfaces the
    /// kernel classified as veth, so this field never carries a peer.
    pub parent_ifindex: Option<u32>,
    /// veth peer ifindex (WO-03 M2). `None` = not observed. Only set for
    /// interfaces the kernel itself classified as veth; see
    /// [`crate::topology::EdgeKind::VethPeer`] for the evidence-based
    /// semantics. The peer may live in another network namespace —
    /// ifindex is namespace-scoped, so this value alone never identifies
    /// an interface of this observation; see [`Self::link_netnsid`].
    pub peer_ifindex: Option<u32>,
    /// Netnsid of the network namespace containing this interface's link
    /// target — the kernel's `IFLA_LINK_NETNSID` (WO-03-F / AUD-01).
    ///
    /// Semantics (live-verified on kernel 5.10): a cross-namespace link
    /// target — a veth peer or a vlan/tunnel parent in another network
    /// namespace — ALWAYS carries an allocated netnsid in the dump, while
    /// a same-namespace target never carries the attribute. `None` = the
    /// kernel did not report the attribute; `Some(-1)` = reported as
    /// present but unassigned (the kernel does not emit this for dumps;
    /// the raw value is preserved rather than clamped).
    ///
    /// For veth interfaces the link target is the peer
    /// ([`Self::peer_ifindex`]); for other kinds it is the parent
    /// ([`Self::parent_ifindex`]). When this field is `Some(_)`, the
    /// target lives in ANOTHER network namespace: its ifindex is scoped
    /// to that namespace and can never be resolved to a local interface,
    /// no matter which local ifindexes coincide.
    #[serde(default)]
    pub link_netnsid: Option<i32>,
    /// Providers that observed this interface, primary first.
    pub sources: Vec<ObservationSource>,
    /// Caveats recorded by providers (kept verbatim).
    pub notes: Vec<String>,
}

/// An observed address assigned to an interface.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Address {
    pub ifindex: u32,
    pub family: AddressFamily,
    pub address: IpAddr,
    pub prefix_len: u8,
    pub scope: Option<AddressScope>,
    /// Kernel address flags as lowercase names ("permanent", "tentative",
    /// "deprecated", "secondary", ...). `None` = not observed; `Some(vec![])`
    /// = observed and no flags set.
    pub flags: Option<Vec<String>>,
    /// Address label (IFA_LABEL, e.g. "eth0:1"); `None` = not observed.
    pub label: Option<String>,
    pub sources: Vec<ObservationSource>,
}

/// A next-hop address encoded by the kernel's `RTA_VIA` — a gateway with
/// its OWN address family, which may differ from the route's family
/// (WO-03-F / AUD-13). Distinct from `RTA_GATEWAY`: MPLS routes and some
/// encapsulations encode their next hop this way, including non-IP
/// (`AF_PACKET`) forms. The family is preserved verbatim; nothing is
/// silently reinterpreted as a same-family gateway, and a route carrying
/// a via is NEVER presented as on-link.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum RouteViaAddr {
    /// `RTA_VIA` with an IPv4 address (route family may still be IPv6).
    Inet(Ipv4Addr),
    /// `RTA_VIA` with an IPv6 address (route family may still be IPv4).
    Inet6(Ipv6Addr),
    /// `RTA_VIA` with `AF_PACKET` bytes (e.g. MPLS MAC next hops).
    Packet(Vec<u8>),
    /// `RTA_VIA` with any other address family; the raw family number and
    /// bytes are preserved verbatim, never guessed.
    Other { family: u16, bytes: Vec<u8> },
}

impl RouteViaAddr {
    /// The via as a plain IP address when it is one (for gateway-style
    /// presentation); `None` for non-IP encodings — those stay what they
    /// are and are never forced into an IP shape.
    pub fn as_ip(&self) -> Option<IpAddr> {
        match self {
            RouteViaAddr::Inet(addr) => Some(IpAddr::V4(*addr)),
            RouteViaAddr::Inet6(addr) => Some(IpAddr::V6(*addr)),
            RouteViaAddr::Packet(_) | RouteViaAddr::Other { .. } => None,
        }
    }
}

impl fmt::Display for RouteViaAddr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RouteViaAddr::Inet(addr) => write!(f, "{addr} (rt-via, ipv4)"),
            RouteViaAddr::Inet6(addr) => write!(f, "{addr} (rt-via, ipv6)"),
            RouteViaAddr::Packet(bytes) => write!(f, "{bytes:02x?} (rt-via, af-packet)"),
            RouteViaAddr::Other { family, bytes } => {
                write!(f, "{bytes:02x?} (rt-via, af-{family})")
            }
        }
    }
}

/// One observed next hop of a multipath route (`RTA_MULTIPATH` entry).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NextHop {
    /// Next-hop gateway. `None` = the kernel reported no gateway for this
    /// hop (on-link) — the same observed-absence semantics as
    /// [`Route::gateway`]. A hop may instead carry its next hop as
    /// [`RouteViaAddr`] (`RTA_VIA` inside the hop); a via-carrying hop is
    /// never on-link.
    pub gateway: Option<IpAddr>,
    /// `RTA_VIA` next-hop encoding of this hop, when the kernel used it
    /// (WO-03-F / AUD-13); preserved with its own address family.
    #[serde(default)]
    pub via: Option<RouteViaAddr>,
    /// Output interface index; `None` = not observed.
    pub output_interface: Option<u32>,
    /// Output interface name, resolved during the merge; `None` = not
    /// resolved (translators record only the ifindex).
    pub output_interface_name: Option<String>,
    /// Hop weight. The kernel stores `rtnh_hops` (weight − 1); this field
    /// carries the effective weight (`rtnh_hops + 1`) as iproute2
    /// displays it. `None` = not observed (the field is absent for
    /// unweighted hops on some kernels).
    pub weight: Option<u16>,
    /// Per-hop flags (`RTNH_F_*`: dead, onlink, linkdown, ...) as
    /// lowercase names; `None` = not observed.
    pub flags: Option<Vec<String>>,
}

/// How a route's next hops were observed (WO-03 M3, schema v3).
///
/// * [`NextHops::Single`] — the kernel sent no `RTA_MULTIPATH`: the
///   route-level `gateway`/`output_interface` ARE the next hop.
/// * [`NextHops::Multi`] — `RTA_MULTIPATH` was decoded; every observed
///   hop is listed and the route-level `gateway`/`output_interface` are
///   `None` (the hops carry them — never flattened into one gateway).
/// * [`NextHops::Undecoded`] — `RTA_MULTIPATH` was present but could not
///   be decoded; the reason is kept verbatim.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum NextHops {
    #[default]
    Single,
    Multi(Vec<NextHop>),
    Undecoded(String),
}

impl NextHops {
    /// Number of observed next hops (0 when not represented or undecoded).
    pub fn observed_hop_count(&self) -> usize {
        match self {
            NextHops::Single => 1,
            NextHops::Multi(hops) => hops.len(),
            NextHops::Undecoded(_) => 0,
        }
    }
}

/// An observed route.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Route {
    pub family: AddressFamily,
    /// Destination prefix. `Some(0.0.0.0/0)` / `Some(::/0)` is the default
    /// route. `None` = the source could not observe a destination.
    pub destination: Option<IpNet>,
    /// Next-hop gateway. For a route that was successfully observed,
    /// `None` means the kernel reported **no gateway** (an on-link route) —
    /// an observed absence, not missing data. A route that no provider
    /// observed at all simply does not appear in the snapshot. A route
    /// may instead encode its next hop as `RTA_VIA` ([`Self::via`], with
    /// its own address family); a via-carrying route is never on-link.
    pub gateway: Option<IpAddr>,
    /// `RTA_VIA` next-hop encoding, when the kernel used it (WO-03-F /
    /// AUD-13): a gateway with its own address family, which may differ
    /// from the route's family (MPLS and encapsulation setups). Preserved
    /// verbatim; never reinterpreted as an on-link absence.
    #[serde(default)]
    pub via: Option<RouteViaAddr>,
    /// Output interface index; `None` = not observed.
    pub output_interface: Option<u32>,
    /// Output interface name; `None` = not observed.
    pub output_interface_name: Option<String>,
    pub protocol: Option<RouteProtocol>,
    pub scope: Option<RouteScope>,
    pub route_type: Option<RouteType>,
    /// Route metric / priority; `None` = not observed (the kernel does not
    /// set RTA_PRIORITY for some route classes, e.g. IPv6 connected routes).
    pub metric: Option<u32>,
    /// Routing table id. `/proc/net/route` exposes only the main table, so
    /// procfs routes are recorded with `table = Some(254)` plus a note.
    /// u32 because the kernel's RTA_TABLE carries extended table ids
    /// beyond the u8 rtm_table compat field.
    pub table: Option<u32>,
    /// Type-of-service selector of the route (netlink `rtm_tos`). Two
    /// routes with the same destination but different TOS are DISTINCT
    /// kernel routes; TOS therefore participates in route identity
    /// (WO-03-F / AUD-02). `None` = not observed (procfs exposes no TOS
    /// column).
    #[serde(default)]
    pub tos: Option<u8>,
    /// Kernel route flags as lowercase names; `None` = not observed.
    pub flags: Option<Vec<String>>,
    /// The route's next hops: single-path (route-level fields), the full
    /// multipath hop list, or an explicit undecodable marker. See
    /// [`NextHops`].
    ///
    /// Defaulted so pre-M3 documents (schema v1/v2, and M2-era v3) parse
    /// unchanged: their routes predate multipath representation and are
    /// single-path by construction, which is exactly `NextHops::Single`.
    #[serde(default)]
    pub next_hops: NextHops,
    pub sources: Vec<ObservationSource>,
    pub notes: Vec<String>,
}

impl Route {
    /// Whether this is a default route (prefix length 0).
    pub fn is_default(&self) -> bool {
        self.destination.as_ref().is_some_and(IpNet::is_default)
    }
}
