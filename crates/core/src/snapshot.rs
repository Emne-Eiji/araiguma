//! Snapshots and change events.
//!
//! A [`NetworkSnapshot`] is the merged, authoritative view of one observation
//! run. [`NetworkEvent`]s describe the difference between two snapshots; the
//! watch CLI emits them as newline-delimited JSON.

use serde::{Deserialize, Serialize};

use crate::model::{Address, AddressFamily, Interface, Route};
use crate::provider::ProviderReport;

/// Current snapshot schema revision.
///
/// * **v3** (WO-03): interfaces gained `peerIfindex` (the veth peer
///   relationship; `null` = not observed — see
///   [`crate::model::Interface::peer_ifindex`]), and routes gain a
///   complete multipath next-hop representation (`nextHops`) instead of
///   the previous first-hop-only form (landed with M3 in this version).
/// * **v2** (WO-02 correction §5): `macAddress` became the three-state
///   [`crate::model::MacObservation`] — the plain address string (present,
///   identical to v1), or the sentinels `"absent"` (observed to have no
///   hardware address) / `"notObserved"`. v1's ambiguous `null` is
///   rejected on read.
/// * v1: everything else, as originally defined.
pub const SNAPSHOT_SCHEMA_VERSION: u32 = 3;

/// One merged observation of the local network environment.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NetworkSnapshot {
    /// Model schema revision; bump on breaking JSON changes. See
    /// [`SNAPSHOT_SCHEMA_VERSION`].
    pub schema_version: u32,
    /// Unix epoch milliseconds of the observation run.
    pub timestamp_ms: u64,
    /// All observed interfaces, sorted by ifindex.
    pub interfaces: Vec<Interface>,
    /// All observed addresses, sorted by (ifindex, family, address).
    pub addresses: Vec<Address>,
    /// All observed routes (all tables), sorted for stable display.
    pub routes: Vec<Route>,
    /// Provider status reports for this run.
    pub providers: Vec<ProviderReport>,
}

impl NetworkSnapshot {
    pub fn interface_by_ifindex(&self, ifindex: u32) -> Option<&Interface> {
        self.interfaces.iter().find(|i| i.ifindex == ifindex)
    }

    pub fn interface_name(&self, ifindex: u32) -> Option<&str> {
        self.interface_by_ifindex(ifindex).map(|i| i.name.as_str())
    }

    pub fn addresses_on(&self, ifindex: u32) -> Vec<&Address> {
        self.addresses
            .iter()
            .filter(|a| a.ifindex == ifindex)
            .collect()
    }

    /// Routes whose next-hop set uses the interface: a single-path route's
    /// output interface, or any multipath hop's interface (WO-03 M3).
    pub fn routes_using_interface(&self, ifindex: u32) -> Vec<&Route> {
        self.routes
            .iter()
            .filter(|r| match &r.next_hops {
                crate::model::NextHops::Single => r.output_interface == Some(ifindex),
                crate::model::NextHops::Multi(hops) => {
                    r.output_interface == Some(ifindex)
                        || hops.iter().any(|hop| hop.output_interface == Some(ifindex))
                }
                crate::model::NextHops::Undecoded(_) => r.output_interface == Some(ifindex),
            })
            .collect()
    }

    /// Default routes, optionally restricted to a family.
    pub fn default_routes(&self, family: Option<AddressFamily>) -> Vec<&Route> {
        self.routes
            .iter()
            .filter(|r| r.is_default() && family.is_none_or(|f| r.family == f))
            .collect()
    }

    /// Distinct routing-table ids present in the observation.
    pub fn tables(&self) -> Vec<u32> {
        let mut tables: Vec<u32> = self.routes.iter().filter_map(|r| r.table).collect();
        tables.sort_unstable();
        tables.dedup();
        tables
    }

    /// Validate that every observation carries provenance (WO-03-F /
    /// AUD-06). An observation with NO source is not a valid observation:
    /// it cannot be traced to anything that actually supplied it, and
    /// downstream presentation must never invent an attribution. Returns
    /// the first offending location as a path-qualified error.
    pub fn validate_provenance(&self) -> Result<(), String> {
        for (index, interface) in self.interfaces.iter().enumerate() {
            if interface.sources.is_empty() {
                return Err(format!(
                    "$.interfaces[{index}].sources is empty — every observation must name at \
                     least one provider that actually supplied it"
                ));
            }
        }
        for (index, address) in self.addresses.iter().enumerate() {
            if address.sources.is_empty() {
                return Err(format!(
                    "$.addresses[{index}].sources is empty — every observation must name at \
                     least one provider that actually supplied it"
                ));
            }
        }
        for (index, route) in self.routes.iter().enumerate() {
            if route.sources.is_empty() {
                return Err(format!(
                    "$.routes[{index}].sources is empty — every observation must name at \
                     least one provider that actually supplied it"
                ));
            }
        }
        Ok(())
    }
}

/// A change between two snapshots (see [`diff_snapshots`]).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum NetworkEvent {
    InterfaceAdded {
        interface: Interface,
    },
    InterfaceRemoved {
        interface: Interface,
    },
    InterfaceChanged {
        interface: Interface,
        changed: Vec<String>,
    },
    AddressAdded {
        address: Address,
    },
    AddressRemoved {
        address: Address,
    },
    AddressChanged {
        address: Address,
    },
    RouteAdded {
        route: Route,
    },
    RouteRemoved {
        route: Route,
    },
    RouteChanged {
        route: Route,
    },
    /// Emitted after a monitoring reconnect: state was re-dumped, and the
    /// diff above (if any) may not cover every change that happened while
    /// the monitor was disconnected. Consumers should treat the latest
    /// snapshot as authoritative.
    Resynchronized,
}

/// Field names that changed between two interface observations (used by
/// `InterfaceChanged`).
pub fn interface_changed_fields(old: &Interface, new: &Interface) -> Vec<String> {
    let mut changed = Vec::new();
    if old.name != new.name {
        changed.push("name".into());
    }
    if old.kind != new.kind || old.kind_detail != new.kind_detail {
        changed.push("kind".into());
    }
    if old.oper_state != new.oper_state {
        changed.push("operState".into());
    }
    if old.admin_state != new.admin_state {
        changed.push("adminState".into());
    }
    if old.mac_address != new.mac_address {
        changed.push("macAddress".into());
    }
    if old.mtu != new.mtu {
        changed.push("mtu".into());
    }
    if old.tx_queue_len != new.tx_queue_len {
        changed.push("txQueueLen".into());
    }
    if old.master_ifindex != new.master_ifindex {
        changed.push("masterIfindex".into());
    }
    if old.parent_ifindex != new.parent_ifindex {
        changed.push("parentIfindex".into());
    }
    if old.peer_ifindex != new.peer_ifindex {
        changed.push("peerIfindex".into());
    }
    if old.link_netnsid != new.link_netnsid {
        changed.push("linkNetnsid".into());
    }
    changed
}

/// Canonical route identity for diffing (sources and notes excluded).
/// Complete identity: every field that distinguishes two kernel routes
/// participates (WO-03-F §5.5 — a change in any discriminator must be
/// visible as a change event, never silently absorbed).
#[derive(Debug, Clone, PartialEq)]
struct RouteKey {
    family: AddressFamily,
    table: Option<u32>,
    tos: Option<u8>,
    destination: Option<crate::model::IpNet>,
    gateway: Option<std::net::IpAddr>,
    output_interface: Option<u32>,
    output_interface_name: Option<String>,
    metric: Option<u32>,
    protocol: Option<crate::model::RouteProtocol>,
    scope: Option<crate::model::RouteScope>,
    route_type: Option<crate::model::RouteType>,
    flags: Option<Vec<String>>,
    next_hops: crate::model::NextHops,
}

fn route_key(route: &Route) -> RouteKey {
    RouteKey {
        family: route.family,
        table: route.table,
        tos: route.tos,
        destination: route.destination,
        gateway: route.gateway,
        output_interface: route.output_interface,
        output_interface_name: route.output_interface_name.clone(),
        metric: route.metric,
        protocol: route.protocol,
        scope: route.scope,
        route_type: route.route_type,
        flags: route.flags.clone(),
        next_hops: route.next_hops.clone(),
    }
}

/// Compute the events that turn `previous` into `current`. Pure function.
pub fn diff_snapshots(previous: &NetworkSnapshot, current: &NetworkSnapshot) -> Vec<NetworkEvent> {
    let mut events = Vec::new();

    // Interfaces.
    for new_iface in &current.interfaces {
        match previous
            .interfaces
            .iter()
            .find(|i| i.ifindex == new_iface.ifindex)
        {
            None => events.push(NetworkEvent::InterfaceAdded {
                interface: new_iface.clone(),
            }),
            Some(old) => {
                let changed = interface_changed_fields(old, new_iface);
                if !changed.is_empty() {
                    events.push(NetworkEvent::InterfaceChanged {
                        interface: new_iface.clone(),
                        changed,
                    });
                }
            }
        }
    }
    for old_iface in &previous.interfaces {
        if !current
            .interfaces
            .iter()
            .any(|i| i.ifindex == old_iface.ifindex)
        {
            events.push(NetworkEvent::InterfaceRemoved {
                interface: old_iface.clone(),
            });
        }
    }

    // Addresses (key: ifindex + family + address + prefix).
    for new_addr in &current.addresses {
        let key = |a: &Address| (a.ifindex, a.family, a.address, a.prefix_len);
        match previous.addresses.iter().find(|a| key(a) == key(new_addr)) {
            None => events.push(NetworkEvent::AddressAdded {
                address: new_addr.clone(),
            }),
            Some(old) if old != new_addr => events.push(NetworkEvent::AddressChanged {
                address: new_addr.clone(),
            }),
            _ => {}
        }
    }
    for old_addr in &previous.addresses {
        let key = |a: &Address| (a.ifindex, a.family, a.address, a.prefix_len);
        if !current.addresses.iter().any(|a| key(a) == key(old_addr)) {
            events.push(NetworkEvent::AddressRemoved {
                address: old_addr.clone(),
            });
        }
    }

    // Routes (key: canonical identity without sources/notes).
    for new_route in &current.routes {
        let key = route_key;
        match previous.routes.iter().find(|r| key(r) == key(new_route)) {
            None => events.push(NetworkEvent::RouteAdded {
                route: new_route.clone(),
            }),
            Some(old) if old != new_route => events.push(NetworkEvent::RouteChanged {
                route: new_route.clone(),
            }),
            _ => {}
        }
    }
    for old_route in &previous.routes {
        if !current
            .routes
            .iter()
            .any(|r| route_key(r) == route_key(old_route))
        {
            events.push(NetworkEvent::RouteRemoved {
                route: old_route.clone(),
            });
        }
    }

    events
}
