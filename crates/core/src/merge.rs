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

use crate::model::{Address, AddressFamily, Interface, IpNet, Route, RT_TABLE_MAIN};
use crate::provider::{ProviderObservation, ProviderReport};
use crate::snapshot::NetworkSnapshot;

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
        schema_version: 1,
        timestamp_ms,
        interfaces,
        addresses,
        routes,
        providers,
    }
}

fn route_sort_key(route: &Route) -> (u8, u32, u8, Option<IpNet>, Option<u32>, Option<IpAddr>) {
    (
        route.family as u8,
        route.table.unwrap_or(RT_TABLE_MAIN as u32),
        // default routes first within a table
        u8::MAX - route.destination.as_ref().map_or(0, |d| d.prefix_len),
        route.destination,
        route.metric,
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
        merge_field(&mut existing.mac_address, new.mac_address);
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
