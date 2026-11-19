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
    Address, AddressFamily, Interface, IpNet, MacObservation, NextHops, ObservationSource, Route,
    RT_TABLE_MAIN,
};
use crate::provider::{ProviderObservation, ProviderReport};
use crate::snapshot::{NetworkSnapshot, SNAPSHOT_SCHEMA_VERSION};

/// Merge provider observations into a snapshot.
pub fn merge(observations: Vec<ProviderObservation>, timestamp_ms: u64) -> NetworkSnapshot {
    let mut observations = observations;
    observations.sort_by_key(|o| (o.priority, o.provider));

    let mut interfaces: Vec<Interface> = Vec::new();
    let mut addresses: Vec<Address> = Vec::new();

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

    // Route merging runs in two phases (WO-03-F / AUD-02 + AUD-03 +
    // AUD-05). Phase A accumulates netlink observations under a full
    // identity; Phase B reconciles procfs file rows against them. See the
    // documentation on `merge_netlink_route` and `reconcile_procfs_rows`.
    let mut accrued: Vec<AccruedRoute> = Vec::new();
    let mut procfs_rows: Vec<Route> = Vec::new();
    for observation in &observations {
        let section_routes = observation
            .routes_v4
            .items()
            .iter()
            .chain(observation.routes_v6.items().iter());
        for route in section_routes {
            if route.sources.first() == Some(&ObservationSource::ProcFs) {
                procfs_rows.push(route.clone());
            } else {
                merge_netlink_route(&mut accrued, route.clone(), &name_to_ifindex);
            }
        }
    }
    reconcile_procfs_rows(&mut accrued, procfs_rows, &name_to_ifindex);
    let mut routes: Vec<Route> = accrued.into_iter().map(|entry| entry.route).collect();
    // Enrich: make sure merged routes carry both oif ifindex and name when
    // either is resolvable — including every multipath hop (WO-03 M3).
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
        if let NextHops::Multi(hops) = &mut route.next_hops {
            for hop in hops {
                match (hop.output_interface, hop.output_interface_name.as_deref()) {
                    (None, Some(name)) => {
                        if let Some(ifindex) = name_to_ifindex(name) {
                            hop.output_interface = Some(ifindex);
                        }
                    }
                    (Some(ifindex), None) => {
                        if let Some(name) = ifindex_to_name(ifindex) {
                            hop.output_interface_name = Some(name);
                        }
                    }
                    _ => {}
                }
            }
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
/// (not-observed last), gateway, TOS (not-observed last).
type RouteSortKey = (
    u8,
    u32,
    u8,
    Option<(u8, IpAddr)>,
    (bool, u32),
    Option<IpAddr>,
    (bool, u8),
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
        (route.tos.is_none(), route.tos.unwrap_or(0)),
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
        merge_field(&mut existing.peer_ifindex, new.peer_ifindex);
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

/// One netlink-accrued route plus Phase-B bookkeeping: how many procfs
/// rows were reconciled into it. A kernel route produces AT MOST ONE row
/// per procfs file, so a second matching row can never be reconciled with
/// the same netlink route (see `reconcile_procfs_rows`).
struct AccruedRoute {
    route: Route,
    procfs_rows_reconciled: usize,
}

/// Whether two OPTIONAL observations agree: unobserved on either side is
/// a gap (the merge may fill it); two observed values must be equal.
/// Never turns a disagreement into a match — that is the AUD-02 lesson:
/// superficial similarity (overlapping next hops, same destination and
/// metric) is not identity.
fn both_agree<T: PartialEq>(a: &Option<T>, b: &Option<T>) -> bool {
    match (a, b) {
        (Some(x), Some(y)) => x == y,
        _ => true,
    }
}

/// A next hop reduced to the fields the procfs files expose: the gateway
/// and the output interface (the index — names are presentation, resolved
/// through the merged interface list). Weight and per-hop flags are NOT
/// part of the reduced key: `/proc/net/route` and `/proc/net/ipv6_route`
/// do not carry them.
type ReducedHop = (Option<IpAddr>, Option<u32>);

/// The COMPLETE next-hop identity two netlink observations of the same
/// route must share: gateway, output interface, kernel weight, and hop
/// flags, in kernel order.
#[derive(Debug, Clone, PartialEq, Eq)]
struct FullHop {
    gateway: Option<IpAddr>,
    oif_index: Option<u32>,
    weight: Option<u16>,
    flags: Option<Vec<String>>,
}

fn full_hops(route: &Route, name_to_ifindex: &dyn Fn(&str) -> Option<u32>) -> Vec<FullHop> {
    let resolve =
        |gateway: Option<IpAddr>, oif_index: Option<u32>, oif_name: Option<&str>| FullHop {
            gateway,
            oif_index: oif_index.or_else(|| oif_name.and_then(name_to_ifindex)),
            weight: None,
            flags: None,
        };
    match &route.next_hops {
        NextHops::Single => vec![resolve(
            route.gateway,
            route.output_interface,
            route.output_interface_name.as_deref(),
        )],
        NextHops::Multi(hops) => hops
            .iter()
            .map(|hop| {
                let mut full = resolve(
                    hop.gateway,
                    hop.output_interface,
                    hop.output_interface_name.as_deref(),
                );
                full.weight = hop.weight;
                full.flags = hop.flags.clone();
                full
            })
            .collect(),
        NextHops::Undecoded(_) => Vec::new(),
    }
}

/// Phase A — whether two netlink observations describe the SAME kernel
/// route: every observed discriminator agrees (family, destination,
/// table, metric, protocol, scope, type, TOS, flags), and the complete
/// next-hop representations are equal. Overlapping next-hop sets prove
/// NOTHING: the kernel holds distinct ECMP routes with overlapping first
/// hops (`ip route append`, reproduced live in WO-03-F), so an overlap
/// must never collapse two observations. An undecodable hop set only
/// matches an identical undecodable marker.
fn same_netlink_route(a: &Route, b: &Route, name_to_ifindex: &dyn Fn(&str) -> Option<u32>) -> bool {
    a.family == b.family
        && both_agree(&a.destination, &b.destination)
        && both_agree(&a.table, &b.table)
        && both_agree(&a.metric, &b.metric)
        && both_agree(&a.protocol, &b.protocol)
        && both_agree(&a.scope, &b.scope)
        && both_agree(&a.route_type, &b.route_type)
        && both_agree(&a.tos, &b.tos)
        && both_agree(&a.flags, &b.flags)
        && match (&a.next_hops, &b.next_hops) {
            (NextHops::Undecoded(reason_a), NextHops::Undecoded(reason_b)) => reason_a == reason_b,
            (NextHops::Undecoded(_), _) | (_, NextHops::Undecoded(_)) => false,
            _ => full_hops(a, name_to_ifindex) == full_hops(b, name_to_ifindex),
        }
}

/// Phase A — merge one netlink observation into the accrued list. The
/// existing (higher-priority) observation wins field conflicts; the
/// incoming one only fills gaps and unites provenance.
fn merge_netlink_route(
    accrued: &mut Vec<AccruedRoute>,
    new: Route,
    name_to_ifindex: &dyn Fn(&str) -> Option<u32>,
) {
    let existing = accrued
        .iter_mut()
        .find(|entry| same_netlink_route(&entry.route, &new, name_to_ifindex));
    if let Some(entry) = existing {
        let existing = &mut entry.route;
        merge_field(&mut existing.destination, new.destination);
        // Route-level gateway/oif only exist on single-path routes; a
        // multipath observation's hops carry them and are never flattened.
        if existing.next_hops == NextHops::Single && new.next_hops == NextHops::Single {
            merge_field(&mut existing.gateway, new.gateway);
            merge_field(&mut existing.output_interface, new.output_interface);
            merge_field(
                &mut existing.output_interface_name,
                new.output_interface_name,
            );
        }
        merge_field(&mut existing.protocol, new.protocol);
        merge_field(&mut existing.scope, new.scope);
        merge_field(&mut existing.route_type, new.route_type);
        merge_field(&mut existing.metric, new.metric);
        merge_field(&mut existing.table, new.table);
        merge_field(&mut existing.tos, new.tos);
        merge_field(&mut existing.flags, new.flags);
        for note in new.notes {
            if !existing.notes.contains(&note) {
                existing.notes.push(note);
            }
        }
        push_unique(&mut existing.sources, &new.sources);
    } else {
        accrued.push(AccruedRoute {
            route: new,
            procfs_rows_reconciled: 0,
        });
    }
}

/// The reduced (gateway, oif) key of a single-path procfs row.
fn procfs_row_hop(row: &Route, name_to_ifindex: &dyn Fn(&str) -> Option<u32>) -> ReducedHop {
    (
        row.gateway,
        row.output_interface.or_else(|| {
            row.output_interface_name
                .as_deref()
                .and_then(name_to_ifindex)
        }),
    )
}

/// Whether a procfs row may belong to a netlink-observed route: same
/// family, destination, and metric; the row's table requirement
/// satisfied (a v4 row implies the main table; a v6 row exposes no table
/// at all and may match any); and the row's first hop contained in the
/// candidate's hop set — `/proc/net/route` shows only the first hop of a
/// multipath route (verified live), so containment, not equality, is the
/// reconciliation evidence.
fn procfs_row_matches(
    row: &Route,
    candidate: &Route,
    name_to_ifindex: &dyn Fn(&str) -> Option<u32>,
) -> bool {
    if candidate.family != row.family
        || candidate.destination != row.destination
        || candidate.metric != row.metric
        || matches!(candidate.next_hops, NextHops::Undecoded(_))
        || row
            .table
            .is_some_and(|row_table| candidate.table != Some(row_table))
    {
        return false;
    }
    let hop = procfs_row_hop(row, name_to_ifindex);
    match &candidate.next_hops {
        NextHops::Single => {
            // Strict (gateway, oif) equality, with one gap rule: a
            // candidate whose oif was not observed accepts the row's hop
            // on gateway alone. The REVERSE is deliberately NOT allowed —
            // a row naming an interface that is not part of this
            // observation (unresolvable name) points at a DIFFERENT
            // interface than any observed candidate oif, and must not
            // merge with it (the audit's false-merge counterexample).
            candidate.gateway == hop.0
                && (candidate.output_interface.is_none() || candidate.output_interface == hop.1)
        }
        NextHops::Multi(hops) => hops.iter().any(|candidate_hop| {
            candidate_hop.gateway == hop.0
                && (candidate_hop.output_interface.is_none()
                    || candidate_hop.output_interface == hop.1)
        }),
        NextHops::Undecoded(_) => false,
    }
}

/// Phase B — reconcile procfs file rows with the netlink observation.
///
/// `/proc/net/route` carries one row per MAIN-table IPv4 route (first
/// hop only for multipath); `/proc/net/ipv6_route` carries one row per
/// IPv6 route across ALL tables with NO table column. Identity evidence
/// is therefore asymmetric: a row is the same kernel fact as a netlink
/// route only when the netlink route is the UNIQUE unused match. Every
/// other row stays an independent observation — multiple matches are
/// never resolved by iteration order (AUD-05), and identical duplicate
/// rows are never silently deduplicated (AUD-03: the multiplicity may be
/// distinct routes in different tables; the file cannot tell).
fn reconcile_procfs_rows(
    accrued: &mut Vec<AccruedRoute>,
    rows: Vec<Route>,
    name_to_ifindex: &dyn Fn(&str) -> Option<u32>,
) {
    let mut standalone: Vec<usize> = Vec::new();
    for row in rows {
        let matches: Vec<usize> = accrued
            .iter()
            .enumerate()
            .filter(|(_, entry)| {
                entry.route.sources.first() != Some(&ObservationSource::ProcFs)
                    && procfs_row_matches(&row, &entry.route, name_to_ifindex)
            })
            .map(|(index, _)| index)
            .collect();
        let unused: Vec<usize> = matches
            .iter()
            .copied()
            .filter(|index| accrued[*index].procfs_rows_reconciled == 0)
            .collect();
        match unused.as_slice() {
            [only] => {
                let entry = &mut accrued[*only];
                entry.procfs_rows_reconciled = 1;
                let existing = &mut entry.route;
                // The netlink observation stands; the row fills gaps
                // (flags, names) and unites provenance.
                if existing.next_hops == NextHops::Single {
                    merge_field(&mut existing.gateway, row.gateway);
                    merge_field(&mut existing.output_interface, row.output_interface);
                    merge_field(
                        &mut existing.output_interface_name,
                        row.output_interface_name,
                    );
                }
                merge_field(&mut existing.protocol, row.protocol);
                merge_field(&mut existing.scope, row.scope);
                merge_field(&mut existing.route_type, row.route_type);
                merge_field(&mut existing.metric, row.metric);
                merge_field(&mut existing.table, row.table);
                merge_field(&mut existing.tos, row.tos);
                merge_field(&mut existing.flags, row.flags);
                for note in row.notes {
                    if !existing.notes.contains(&note) {
                        existing.notes.push(note);
                    }
                }
                push_unique(&mut existing.sources, &row.sources);
            }
            [] if !matches.is_empty() => {
                // The row matches netlink-observed route(s) whose file row
                // was already reconciled — a kernel route produces ONE row
                // per file, so this is a DISTINCT observation (most likely
                // the same route in another table the file cannot name).
                let mut route = row;
                route.notes.push(
                    "an identical row from this source was already reconciled with a netlink-observed route; this row is a distinct observation (the source exposes no routing-table id, so identity is not established)"
                        .to_string(),
                );
                accrued.push(AccruedRoute {
                    route,
                    procfs_rows_reconciled: 0,
                });
                standalone.push(accrued.len() - 1);
            }
            [] => {
                accrued.push(AccruedRoute {
                    route: row,
                    procfs_rows_reconciled: 0,
                });
                standalone.push(accrued.len() - 1);
            }
            many => {
                let mut route = row;
                route.notes.push(format!(
                    "this row matches {} netlink-observed routes with the same destination, metric, and first hop; the source exposes no routing-table id, so the attribution is not established — kept as an independent observation",
                    many.len()
                ));
                accrued.push(AccruedRoute {
                    route,
                    procfs_rows_reconciled: 0,
                });
                standalone.push(accrued.len() - 1);
            }
        }
    }

    // AUD-03 multiplicity honesty: identical standalone rows (same source,
    // no table id) are preserved as separate observations. Annotate them so
    // the preserved multiplicity is explicit rather than looking like a
    // duplicate by accident.
    type RowSignature = (
        AddressFamily,
        Option<IpNet>,
        Option<u32>,
        Option<u32>,
        Option<IpAddr>,
        Option<u32>,
        Option<String>,
    );
    fn row_signature(route: &Route) -> RowSignature {
        (
            route.family,
            route.destination,
            route.metric,
            route.table,
            route.gateway,
            route.output_interface,
            route.output_interface_name.clone(),
        )
    }
    let mut signatures: std::collections::HashMap<RowSignature, usize> =
        std::collections::HashMap::new();
    for &index in &standalone {
        let route = &accrued[index].route;
        if route.sources.first() == Some(&ObservationSource::ProcFs) {
            *signatures.entry(row_signature(route)).or_insert(0) += 1;
        }
    }
    for &index in &standalone {
        let route = &mut accrued[index].route;
        if route.sources.first() != Some(&ObservationSource::ProcFs) {
            continue;
        }
        let identical = signatures[&row_signature(route)];
        if identical > 1 {
            let note = format!(
                "this row appears {identical} times in a source that exposes no routing-table id; the copies may be distinct routes in different tables — multiplicity preserved, never deduplicated"
            );
            if !route.notes.contains(&note) {
                route.notes.push(note);
            }
        }
    }
}
