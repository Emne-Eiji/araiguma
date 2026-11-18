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
    /// underlying device).
    pub parent_ifindex: Option<u32>,
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
    /// observed at all simply does not appear in the snapshot.
    pub gateway: Option<IpAddr>,
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
    /// Kernel route flags as lowercase names; `None` = not observed.
    pub flags: Option<Vec<String>>,
    pub sources: Vec<ObservationSource>,
    pub notes: Vec<String>,
}

impl Route {
    /// Whether this is a default route (prefix length 0).
    pub fn is_default(&self) -> bool {
        self.destination.as_ref().is_some_and(IpNet::is_default)
    }
}
