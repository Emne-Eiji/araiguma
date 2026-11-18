//! Merge layer: combine provider observations into one coherent snapshot.
//!
//! Rules (WO-01R §4.3):
//! 1. Providers are ordered by priority (netlink first, sysfs second).
//! 2. For a given item, the highest-priority provider's non-`None` field
//!    values win; lower-priority providers only fill fields that are still
//!    `None` ("low priority only fills gaps").
//! 3. A provider failure degrades the snapshot, never discards it: items
//!    from failing providers are absent, but observations from other
//!    providers survive, and every provider's report is preserved.
//! 4. Cross-source duplicates (the same kernel fact observed by both
//!    netlink and procfs) are merged into one item whose `sources` list
//!    records every contributor.

use std::net::IpAddr;

use crate::model::{
    Address, AddressFamily, Interface, IpNet, MacObservation, Route, RT_TABLE_MAIN,
};
use crate::provider::{ProviderObservation, ProviderReport};
use crate::snapshot::{NetworkSnapshot, SNAPSHOT_SCHEMA_VERSION};

/// Merge provider observations into a snapshot.
pub fn merge(observations: Vec<ProviderObservation>, timestamp_ms: u64) -> NetworkSnapshot {
    let mut observations = observations;
    observations.sort_by_key(|o| (o.priority, o.provider));

    let mut interfaces: Vec<Interface> = Vec::new();
    let mut addresses: Vec<Address> = Vec::new();
    let mut routes: Vec<Route> = Vec::new();

    for observation in &observations {
        for iface in observation.interfaces.items() {
            merge_interface(&mut interfaces, iface.clone());
        }
    }
    for observation in &observations {
        for address in observation.addresses_v4.items() {
            merge_address(&mut addresses, address.clone());
        }
        for address in observation.addresses_v6.items() {
            merge_address(&mut addresses, address.clone());
        }
    }

    // Build interface maps for oif normalization before merging routes.
    let ifindex_to_name = |ifindex: u32| {
        interfaces
            .iter()
            .find(|i| i.ifindex == ifindex)
            .map(|i| i.name.clone())
    };
    let name_to_ifindex = |name: &str| {
        interfaces
            .iter()
            .find(|i| i.name == name)
            .map(|i| i.ifindex)
    };

    for observation in &observations {
        for route in observation.routes_v4.items() {
            merge_route(
                &mut routes,
                route.clone(),
                &ifindex_to_name,
                &name_to_ifindex,
            );
        }
        for route in observation.routes_v6.items() {
            merge_route(
                &mut routes,
                route.clone(),
                &ifindex_to_name,
                &name_to_ifindex,
            );
        }
    }

    // Enrich: make sure merged routes carry both oif ifindex and name when
    // either is resolvable.
    for route in &mut routes {
        match (
            route.output_interface,
            route.output_interface_name.as_deref(),
        ) {
            (None, Some(name)) => {
                if let Some(ifindex) = name_to_ifindex(name) {
                    route.output_interface = Some(ifindex);
                }
            }
            (Some(ifindex), None) => {
                if let Some(name) = ifindex_to_name(ifindex) {
                    route.output_interface_name = Some(name);
                }
            }
            _ => {}
        }
    }

    interfaces.sort_by_key(|i| i.ifindex);
    addresses.sort_by(|a, b| {
        (a.ifindex, a.family, a.address, a.prefix_len).cmp(&(
            b.ifindex,
            b.family,
            b.address,
            b.prefix_len,
        ))
    });
    routes.sort_by_key(route_sort_key);

    let providers: Vec<ProviderReport> = observations.iter().map(|o| o.summarize()).collect();

    NetworkSnapshot {
        schema_version: SNAPSHOT_SCHEMA_VERSION,
        timestamp_ms,
        interfaces,
        addresses,
        routes,
        providers,
    }
}

/// Display order for the merged route list (WO-02 correction §3).
///
/// Rules, in priority order:
/// 1. Address family groups (IPv4 before IPv6) — never interleaved.
/// 2. Routing-table groups (ascending table id; a not-observed table is
///    grouped with the main table, matching the merge identity). Routes
///    from different tables are never merged into one ordering group.
/// 3. Within a family+table group: observed destinations first, and among
///    them prefix length ascending — default routes (prefix length 0)
///    first, then progressively more-specific prefixes (broader before
///    narrower, matching `ip route`'s presentation). Routes whose
///    destination was not observed sort after every observed destination.
/// 4. Deterministic tie-breaking within the same prefix: metric ascending
///    (a not-observed metric sorts after every observed value — no
///    preference is invented), then gateway (on-link before a named
///    gateway), which keeps multiple default routes in a stable order.
///
/// Sort key for the merged route list (see [`route_sort_key`]): family,
/// table, destination-not-observed flag, (prefix length, address), metric
/// (not-observed last), gateway.
type RouteSortKey = (
    u8,
    u32,
    u8,
    Option<(u8, IpAddr)>,
    (bool, u32),
    Option<IpAddr>,
);

fn route_sort_key(route: &Route) -> RouteSortKey {
    (
        route.family as u8,
        route.table.unwrap_or(RT_TABLE_MAIN as u32),
        route.destination.is_none() as u8,
        route
            .destination
            .map(|destination| (destination.prefix_len, destination.address)),
        (route.metric.is_none(), route.metric.unwrap_or(0)),
        route.gateway,
    )
}

fn push_unique<T: PartialEq + Copy>(list: &mut Vec<T>, values: &[T]) {
    for value in values {
        if !list.contains(value) {
            list.push(*value);
        }
    }
}

fn merge_field<T>(current: &mut Option<T>, incoming: Option<T>) {
    if current.is_none() {
        *current = incoming;
    }
}

fn merge_interface(into: &mut Vec<Interface>, new: Interface) {
    if let Some(existing) = into.iter_mut().find(|i| i.ifindex == new.ifindex) {
        // Higher-priority provider wins: existing values are only filled
        // where they are None. Classification is never "upgraded" from a
        // lower-priority source.
        merge_field(&mut existing.kind_detail, new.kind_detail);
        merge_field(&mut existing.oper_state, new.oper_state);
        merge_field(&mut existing.admin_state, new.admin_state);
        // MAC semantics (v2): a higher-priority observation of either
        // Present or Absent stands; NotObserved is the only gap a
        // lower-priority provider may fill.
        if existing.mac_address == MacObservation::NotObserved {
            existing.mac_address = new.mac_address.clone();
        }
        merge_field(&mut existing.mtu, new.mtu);
        merge_field(&mut existing.tx_queue_len, new.tx_queue_len);
        merge_field(&mut existing.master_ifindex, new.master_ifindex);
        merge_field(&mut existing.parent_ifindex, new.parent_ifindex);
        for note in new.notes {
            if !existing.notes.contains(&note) {
                existing.notes.push(note);
            }
        }
        push_unique(&mut existing.sources, &new.sources);
    } else {
        into.push(new);
    }
}

fn merge_address(into: &mut Vec<Address>, new: Address) {
    if let Some(existing) = into.iter_mut().find(|a| {
        a.ifindex == new.ifindex
            && a.family == new.family
            && a.address == new.address
            && a.prefix_len == new.prefix_len
    }) {
        merge_field(&mut existing.scope, new.scope);
        merge_field(&mut existing.flags, new.flags);
        merge_field(&mut existing.label, new.label);
        push_unique(&mut existing.sources, &new.sources);
    } else {
        into.push(new);
    }
}

/// The cross-provider identity of a route. A route from `/proc/net/route`
/// (implied main table, oif by name) must deduplicate against the same
/// kernel route observed via netlink (explicit table 254, oif by ifindex),
/// so both representations of the output interface are normalized through
/// the merged interface list before comparison.
#[derive(Debug, Clone, PartialEq, Eq)]
struct RouteIdentity {
    family: AddressFamily,
    table: u32,
    destination: Option<IpNet>,
    gateway: Option<IpAddr>,
    oif_index: Option<u32>,
    oif_name: Option<String>,
    metric: Option<u32>,
}

fn route_identity(
    route: &Route,
    ifindex_to_name: &dyn Fn(u32) -> Option<String>,
    name_to_ifindex: &dyn Fn(&str) -> Option<u32>,
) -> RouteIdentity {
    let table = route.table.unwrap_or(RT_TABLE_MAIN as u32);
    let oif_index = route.output_interface.or_else(|| {
        route
            .output_interface_name
            .as_deref()
            .and_then(name_to_ifindex)
    });
    let oif_name = route
        .output_interface_name
        .clone()
        .or_else(|| route.output_interface.and_then(ifindex_to_name));
    RouteIdentity {
        family: route.family,
        table,
        destination: route.destination,
        gateway: route.gateway,
        oif_index,
        oif_name,
        metric: route.metric,
    }
}

fn merge_route(
    into: &mut Vec<Route>,
    new: Route,
    ifindex_to_name: &dyn Fn(u32) -> Option<String>,
    name_to_ifindex: &dyn Fn(&str) -> Option<u32>,
) {
    let new_key = route_identity(&new, ifindex_to_name, name_to_ifindex);
    if let Some(existing) = into
        .iter_mut()
        .find(|r| route_identity(r, ifindex_to_name, name_to_ifindex) == new_key)
    {
        merge_field(&mut existing.destination, new.destination);
        merge_field(&mut existing.gateway, new.gateway);
        merge_field(&mut existing.output_interface, new.output_interface);
        merge_field(
            &mut existing.output_interface_name,
            new.output_interface_name,
        );
        merge_field(&mut existing.protocol, new.protocol);
        merge_field(&mut existing.scope, new.scope);
        merge_field(&mut existing.route_type, new.route_type);
        merge_field(&mut existing.metric, new.metric);
        merge_field(&mut existing.table, new.table);
        merge_field(&mut existing.flags, new.flags);
        for note in new.notes {
            if !existing.notes.contains(&note) {
                existing.notes.push(note);
            }
        }
        push_unique(&mut existing.sources, &new.sources);
    } else {
        into.push(new);
    }
}
