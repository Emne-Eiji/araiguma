//! # araiguma-sysfs
//!
//! The sysfs/procfs discovery provider: a read-only fallback that reads
//! `/sys/class/net` (interface attributes) and `/proc/net/if_inet6` /
//! `/proc/net/route` / `/proc/net/ipv6_route` (IPv6 addresses, main-table
//! IPv4 routes, multi-table IPv6 routes).
//!
//! The provider is strictly a *fallback*: netlink observes strictly more, so
//! this provider runs at lower merge priority and only fills gaps. Its
//! inherent fidelity limits (interface classification ambiguity, missing
//! route fields) are reported as notes and reflected in the `Degraded`
//! provider state.
//!
//! Both roots are injectable for fixture tests: `with_roots(sys_root,
//! proc_root)` expects `<sys_root>/class/net/...` and
//! `<proc_root>/net/route`, `<proc_root>/net/if_inet6`,
//! `<proc_root>/net/ipv6_route`.

use std::collections::HashMap;
use std::net::IpAddr;
use std::path::{Path, PathBuf};

use araiguma_core::model::{
    Address, AddressFamily, AddressScope, AdminState, Interface, InterfaceKind, IpNet,
    MacObservation, NextHops, ObservationSource, OperState, Route, RT_TABLE_MAIN,
};
use araiguma_core::provider::{DiscoveryProvider, ProviderId, ProviderObservation, SectionResult};

pub mod parse;

const NOTE_CLASSIFICATION: &str = "sysfs cannot distinguish dummy, veth, vlan and similar virtual interfaces from Ethernet (same ARPHRD type); netlink classification takes priority";
const NOTE_IF_INET6: &str = "/proc/net/if_inet6 exposes the address scope and IFA_F flag bits only; address labels are not available from procfs";
const NOTE_ROUTE_FIELDS: &str =
    "route fields protocol/scope/type are not available from /proc/net/route";
const NOTE_TABLE_IMPLIED: &str = "table implied: /proc/net/route exposes only the main table (254)";
const NOTE_V6_ROUTE_LIMITS: &str = "/proc/net/ipv6_route exposes no routing-table id (its rows span multiple tables, e.g. main and local) and no protocol/scope/type; the table stays not observed and netlink's table attribution takes priority";
const NOTE_V6_FLAGS: &str = "IPv6 route flags are mapped from the RTF_* constants in linux/ipv6_route.h and linux/route.h; bits outside those constants are not represented";

const NOTE_IFACE_CAVEAT_ETHERNET: &str = "classification caveat: sysfs cannot distinguish dummy, veth, vlan and similar virtual interfaces from Ethernet (same ARPHRD type)";
const NOTE_IFACE_CAVEAT_VIRTUAL: &str = "classification caveat: sysfs cannot classify this interface further (ARPHRD_NONE); netlink would likely classify it as wireguard or tunnel";

/// The sysfs/procfs discovery provider.
#[derive(Debug, Clone)]
pub struct SysFsProvider {
    sys_root: PathBuf,
    proc_root: PathBuf,
}

impl Default for SysFsProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl SysFsProvider {
    /// Provider against the real filesystem.
    pub fn new() -> Self {
        Self::with_roots("/sys", "/proc")
    }

    /// Provider against fixture roots (see crate docs for the layout).
    pub fn with_roots(sys_root: impl Into<PathBuf>, proc_root: impl Into<PathBuf>) -> Self {
        Self {
            sys_root: sys_root.into(),
            proc_root: proc_root.into(),
        }
    }

    /// Enumerate interfaces from `<sys_root>/class/net`.
    pub fn read_interfaces(&self) -> SectionResult<Interface> {
        self.read_interfaces_inner().0
    }

    /// Enumerate interfaces, also reporting entries that had to be skipped
    /// (unreadable `ifindex`) so `observe()` can surface them as notes
    /// instead of silently dropping them.
    fn read_interfaces_inner(&self) -> (SectionResult<Interface>, Vec<String>) {
        let dir = self.sys_root.join("class/net");
        let entries = match std::fs::read_dir(&dir) {
            Ok(entries) => entries,
            Err(error) => {
                return (
                    SectionResult::Failed(format!("cannot read {}: {error}", dir.display())),
                    Vec::new(),
                );
            }
        };

        let mut interfaces: Vec<Interface> = Vec::new();
        let mut skipped: Vec<String> = Vec::new();
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }
            let Some(ifindex) =
                read_file(&path.join("ifindex")).and_then(|s| parse::parse_dec_u32(&s))
            else {
                skipped.push(name);
                continue;
            };
            interfaces.push(read_one_interface(&path, ifindex, &name));
        }

        // Second pass: resolve bridge/bond master symlinks to ifindexes.
        let by_name: HashMap<String, u32> = interfaces
            .iter()
            .map(|i| (i.name.clone(), i.ifindex))
            .collect();
        for interface in &mut interfaces {
            let master_path = self
                .sys_root
                .join("class/net")
                .join(&interface.name)
                .join("master");
            if let Some(master_name) = read_link_basename(&master_path) {
                interface.master_ifindex = by_name.get(&master_name).copied();
            }
        }

        (SectionResult::Observed(interfaces), skipped)
    }

    /// Read IPv6 addresses from `<proc_root>/net/if_inet6`.
    pub fn read_addresses_v6(&self) -> SectionResult<Address> {
        let path = self.proc_root.join("net/if_inet6");
        let content = match std::fs::read_to_string(&path) {
            Ok(content) => content,
            Err(error) => {
                return SectionResult::Failed(format!("cannot read {}: {error}", path.display()))
            }
        };
        let mut addresses = Vec::new();
        for line in content.lines() {
            if line.trim().is_empty() {
                continue;
            }
            let Some(entry) = parse::parse_if_inet6_line(line) else {
                continue;
            };
            addresses.push(Address {
                ifindex: entry.ifindex,
                family: AddressFamily::Ipv6,
                address: IpAddr::V6(entry.address),
                prefix_len: entry.prefix_len,
                scope: AddressScope::from_if_inet6_scope_raw(entry.scope_raw),
                flags: Some(parse::if_inet6_flags(entry.flags_raw)),
                label: None,
                sources: vec![ObservationSource::ProcFs],
            });
        }
        SectionResult::Observed(addresses)
    }

    /// Read main-table IPv4 routes from `<proc_root>/net/route`.
    pub fn read_routes_v4(&self, name_to_ifindex: &HashMap<String, u32>) -> SectionResult<Route> {
        let path = self.proc_root.join("net/route");
        let content = match std::fs::read_to_string(&path) {
            Ok(content) => content,
            Err(error) => {
                return SectionResult::Failed(format!("cannot read {}: {error}", path.display()))
            }
        };
        let mut routes = Vec::new();
        for line in content.lines() {
            if line.trim().is_empty() || parse::is_route_header(line) {
                continue;
            }
            let Some(entry) = parse::parse_route_line(line) else {
                continue;
            };
            routes.push(Route {
                family: AddressFamily::Ipv4,
                destination: Some(IpNet::new(
                    IpAddr::V4(entry.destination),
                    parse::prefix_len_from_mask(entry.mask),
                )),
                // Gateway None here means the kernel reported no gateway
                // (on-link) — an observed absence.
                gateway: entry.gateway.map(IpAddr::V4),
                output_interface: name_to_ifindex.get(&entry.iface).copied(),
                output_interface_name: Some(entry.iface),
                protocol: None,
                scope: None,
                route_type: None,
                metric: Some(entry.metric),
                table: Some(RT_TABLE_MAIN as u32),
                flags: None,
                next_hops: NextHops::Single,
                sources: vec![ObservationSource::ProcFs],
                notes: vec![NOTE_TABLE_IMPLIED.to_string()],
            });
        }
        SectionResult::Observed(routes)
    }

    /// Read IPv6 routes from `<proc_root>/net/ipv6_route` (WO-03 M4).
    ///
    /// Semantics verified live (kernel 5.10, cross-checked with `ip -6
    /// route`): columns dst dst_len src src_len nexthop metric refcnt use
    /// flags dev, all hex except dev. The file spans MULTIPLE routing tables
    /// without a table column, so the model records `table: None` — never
    /// an invented id — with a note. An all-zero nexthop is the kernel's
    /// observed "no gateway" (on-link) encoding. A missing file is a
    /// `Failed` section ("cannot read"), NOT an observed "no IPv6 routes":
    /// absence of the file is not evidence about the route table.
    pub fn read_routes_v6(&self, name_to_ifindex: &HashMap<String, u32>) -> SectionResult<Route> {
        let path = self.proc_root.join("net/ipv6_route");
        let content = match std::fs::read_to_string(&path) {
            Ok(content) => content,
            Err(error) => {
                return SectionResult::Failed(format!("cannot read {}: {error}", path.display()))
            }
        };
        let mut routes = Vec::new();
        for line in content.lines() {
            if line.trim().is_empty() {
                continue;
            }
            let Some(entry) = parse::parse_route_v6_line(line) else {
                continue;
            };
            routes.push(Route {
                family: AddressFamily::Ipv6,
                destination: Some(IpNet::new(IpAddr::V6(entry.destination), entry.prefix_len)),
                // Gateway None here means the kernel reported no gateway
                // (on-link) — an observed absence, same as the v4 file.
                gateway: entry.gateway.map(IpAddr::V6),
                output_interface: name_to_ifindex.get(&entry.iface).copied(),
                output_interface_name: Some(entry.iface),
                protocol: None,
                scope: None,
                route_type: None,
                metric: Some(entry.metric),
                // The file has no table column; rows span tables. Not
                // observed — never fabricated (the v4 file, by contrast,
                // genuinely exposes main-table rows only).
                table: None,
                flags: Some(parse::route_v6_flags(entry.flags_raw)),
                next_hops: NextHops::Single,
                sources: vec![ObservationSource::ProcFs],
                notes: vec![NOTE_V6_ROUTE_LIMITS.to_string(), NOTE_V6_FLAGS.to_string()],
            });
        }
        SectionResult::Observed(routes)
    }
}

fn read_file(path: &Path) -> Option<String> {
    std::fs::read_to_string(path).ok()
}

fn read_link_basename(path: &Path) -> Option<String> {
    let target = std::fs::read_link(path).ok()?;
    let basename = target.file_name()?.to_string_lossy().into_owned();
    Some(basename)
}

fn read_one_interface(path: &Path, ifindex: u32, name: &str) -> Interface {
    let oper_state = read_file(&path.join("operstate")).and_then(|s| OperState::from_sysfs_str(&s));
    let admin_state = read_file(&path.join("flags"))
        .and_then(|s| parse::parse_hex_u32(&s))
        .map(|flags| {
            if flags & parse::IFF_UP != 0 {
                AdminState::Up
            } else {
                AdminState::Down
            }
        });
    // MAC observation (v2): the sysfs `address` file is authoritative
    // about the interface's own claim. An existing-but-empty file is an
    // observed absence (e.g. wireguard); a missing/unreadable file means
    // the address was not observed — never a silent zero/empty value.
    let mac_address = match std::fs::read_to_string(path.join("address")) {
        Ok(content) => {
            let trimmed = content.trim().to_ascii_lowercase();
            if trimmed.is_empty() {
                MacObservation::Absent
            } else {
                MacObservation::Present(trimmed)
            }
        }
        Err(_) => MacObservation::NotObserved,
    };
    let mtu = read_file(&path.join("mtu")).and_then(|s| parse::parse_dec_u32(&s));
    let tx_queue_len = read_file(&path.join("tx_queue_len")).and_then(|s| {
        s.trim()
            .parse::<i64>()
            .ok()
            .and_then(|v| u32::try_from(v).ok())
    });
    let arphrd = read_file(&path.join("type"))
        .and_then(|s| parse::parse_dec_u32(&s))
        .and_then(|v| u16::try_from(v).ok());

    // Wireless detection beats ARPHRD (ARPHRD 1 is also used by Wi-Fi).
    let mut notes = Vec::new();
    let kind = if path.join("wireless").is_dir() || path.join("phy80211").is_dir() {
        InterfaceKind::Wireless
    } else if path.join("bridge").is_dir() {
        InterfaceKind::Bridge
    } else if let Some(arphrd) = arphrd {
        let kind = InterfaceKind::from_arphrd(arphrd);
        if kind == InterfaceKind::Ethernet {
            notes.push(NOTE_IFACE_CAVEAT_ETHERNET.to_string());
        } else if kind == InterfaceKind::Virtual {
            notes.push(NOTE_IFACE_CAVEAT_VIRTUAL.to_string());
        }
        kind
    } else {
        InterfaceKind::Other
    };

    Interface {
        ifindex,
        name: name.to_string(),
        kind,
        kind_detail: None,
        oper_state,
        admin_state,
        mac_address,
        mtu,
        tx_queue_len,
        master_ifindex: None,
        parent_ifindex: None,
        peer_ifindex: None,
        sources: vec![ObservationSource::SysFs],
        notes,
    }
}

#[async_trait::async_trait]
impl DiscoveryProvider for SysFsProvider {
    fn id(&self) -> ProviderId {
        ProviderId::SysFs
    }

    fn priority(&self) -> u8 {
        1
    }

    async fn observe(&mut self) -> ProviderObservation {
        // Small, fast file reads; blocking calls are acceptable here.
        let (interfaces, skipped) = self.read_interfaces_inner();
        let name_to_ifindex: HashMap<String, u32> = match &interfaces {
            SectionResult::Observed(items) => {
                items.iter().map(|i| (i.name.clone(), i.ifindex)).collect()
            }
            _ => HashMap::new(),
        };
        let addresses_v6 = self.read_addresses_v6();
        let routes_v4 = self.read_routes_v4(&name_to_ifindex);
        let routes_v6 = self.read_routes_v6(&name_to_ifindex);

        let mut notes = vec![
            NOTE_CLASSIFICATION.to_string(),
            NOTE_IF_INET6.to_string(),
            NOTE_ROUTE_FIELDS.to_string(),
        ];
        if !skipped.is_empty() {
            notes.push(format!(
                "skipped sysfs interface entries without a readable ifindex: {}",
                skipped.join(", ")
            ));
        }

        ProviderObservation {
            provider: ProviderId::SysFs,
            priority: 1,
            interfaces,
            addresses_v4: SectionResult::Unsupported,
            addresses_v6,
            routes_v4,
            routes_v6,
            fidelity_limited: true,
            notes,
        }
    }
}
