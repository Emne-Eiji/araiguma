//! Merge-layer behavior: netlink primary, sysfs fills gaps, failures
//! degrade without destroying observations.

use std::net::{IpAddr, Ipv4Addr};

use araiguma_core::merge::merge;
use araiguma_core::model::{
    Address, AddressFamily, AddressScope, AdminState, Interface, InterfaceKind, IpNet,
    MacObservation, NextHop, NextHops, ObservationSource, OperState, Route, RouteProtocol,
    RouteScope, RouteType, RT_TABLE_LOCAL, RT_TABLE_MAIN,
};
use araiguma_core::provider::{ProviderId, ProviderObservation, SectionResult};

fn iface(ifindex: u32, name: &str, kind: InterfaceKind) -> Interface {
    Interface {
        ifindex,
        name: name.into(),
        kind,
        kind_detail: None,
        oper_state: Some(OperState::Up),
        admin_state: Some(AdminState::Up),
        mac_address: MacObservation::NotObserved,
        mtu: None,
        tx_queue_len: None,
        master_ifindex: None,
        parent_ifindex: None,
        peer_ifindex: None,
        sources: vec![],
        notes: vec![],
    }
}

fn v4_route(dst: &str, prefix: u8, gateway: Option<IpAddr>, oif: Option<u32>) -> Route {
    Route {
        family: AddressFamily::Ipv4,
        destination: Some(IpNet::new(dst.parse().unwrap(), prefix)),
        gateway,
        output_interface: oif,
        output_interface_name: None,
        protocol: None,
        scope: None,
        route_type: Some(RouteType::Unicast),
        metric: Some(0),
        table: Some(RT_TABLE_MAIN as u32),
        flags: None,
        next_hops: NextHops::Single,
        sources: vec![],
        notes: vec![],
    }
}

fn observation(provider: ProviderId, priority: u8) -> ProviderObservation {
    ProviderObservation {
        provider,
        priority,
        interfaces: SectionResult::NotObserved,
        addresses_v4: SectionResult::NotObserved,
        addresses_v6: SectionResult::NotObserved,
        routes_v4: SectionResult::NotObserved,
        routes_v6: SectionResult::NotObserved,
        fidelity_limited: false,
        notes: vec![],
    }
}

#[test]
fn netlink_primary_and_sysfs_fills_gaps() {
    let mut netlink = observation(ProviderId::Netlink, 0);
    let mut eth = iface(2, "eth0", InterfaceKind::Ethernet);
    eth.mac_address = MacObservation::Present("52:54:00:12:34:56".into());
    eth.sources = vec![ObservationSource::Netlink];
    netlink.interfaces = SectionResult::Observed(vec![eth]);

    let mut sysfs = observation(ProviderId::SysFs, 1);
    let mut eth_sysfs = iface(2, "eth0", InterfaceKind::Ethernet);
    eth_sysfs.mtu = Some(1500);
    eth_sysfs.sources = vec![ObservationSource::SysFs];
    sysfs.interfaces = SectionResult::Observed(vec![eth_sysfs]);
    sysfs.fidelity_limited = true;

    let snapshot = merge(vec![netlink, sysfs], 1);
    assert_eq!(snapshot.interfaces.len(), 1);
    let merged = &snapshot.interfaces[0];
    // Netlink's value wins where both observed.
    assert_eq!(merged.mac_address.present(), Some("52:54:00:12:34:56"));
    // Sysfs fills the gap netlink left.
    assert_eq!(merged.mtu, Some(1500));
    // Sources keep priority order, primary first.
    assert_eq!(
        merged.sources,
        vec![ObservationSource::Netlink, ObservationSource::SysFs]
    );
}

#[test]
fn netlink_classification_wins_over_sysfs() {
    let mut netlink = observation(ProviderId::Netlink, 0);
    let mut dummy = iface(11, "dummy0", InterfaceKind::Dummy);
    dummy.kind_detail = Some("dummy".into());
    dummy.sources = vec![ObservationSource::Netlink];
    netlink.interfaces = SectionResult::Observed(vec![dummy]);

    let mut sysfs = observation(ProviderId::SysFs, 1);
    let mut as_ethernet = iface(11, "dummy0", InterfaceKind::Ethernet);
    as_ethernet.sources = vec![ObservationSource::SysFs];
    sysfs.interfaces = SectionResult::Observed(vec![as_ethernet]);

    let snapshot = merge(vec![netlink, sysfs], 1);
    assert_eq!(snapshot.interfaces[0].kind, InterfaceKind::Dummy);
}

#[test]
fn provider_failure_degrades_without_destroying_other_observations() {
    // Netlink fails entirely; sysfs observations survive.
    let mut netlink = observation(ProviderId::Netlink, 0);
    netlink.interfaces = SectionResult::Failed("socket: blocked".into());
    netlink.routes_v4 = SectionResult::Failed("socket: blocked".into());

    let mut sysfs = observation(ProviderId::SysFs, 1);
    let lo = iface(1, "lo", InterfaceKind::Loopback);
    sysfs.interfaces = SectionResult::Observed(vec![lo]);
    sysfs.fidelity_limited = true;

    let snapshot = merge(vec![netlink, sysfs], 1);
    assert_eq!(snapshot.interfaces.len(), 1);
    assert_eq!(snapshot.interfaces[0].name, "lo");
    assert_eq!(snapshot.providers.len(), 2);
    let netlink_report = snapshot
        .providers
        .iter()
        .find(|p| p.provider == ProviderId::Netlink)
        .unwrap();
    assert_eq!(
        netlink_report.state,
        araiguma_core::provider::ProviderState::Unavailable
    );
}

#[test]
fn route_dedup_across_netlink_and_procfs() {
    let mut netlink = observation(ProviderId::Netlink, 0);
    let mut nl_iface = iface(2, "eth0", InterfaceKind::Ethernet);
    nl_iface.sources = vec![ObservationSource::Netlink];
    netlink.interfaces = SectionResult::Observed(vec![nl_iface]);
    let mut route = v4_route("192.168.0.0", 24, None, Some(2));
    route.gateway = Some(IpAddr::V4(Ipv4Addr::new(192, 168, 0, 1)));
    route.protocol = Some(RouteProtocol::Dhcp);
    route.scope = Some(RouteScope::Universe);
    route.sources = vec![ObservationSource::Netlink];
    netlink.routes_v4 = SectionResult::Observed(vec![route]);

    let mut sysfs = observation(ProviderId::SysFs, 1);
    let mut s_iface = iface(2, "eth0", InterfaceKind::Ethernet);
    s_iface.sources = vec![ObservationSource::SysFs];
    sysfs.interfaces = SectionResult::Observed(vec![s_iface]);
    // Same kernel route observed through /proc/net/route: name-based oif,
    // implied main table, no protocol.
    let mut proc_route = v4_route("192.168.0.0", 24, None, None);
    proc_route.gateway = Some(IpAddr::V4(Ipv4Addr::new(192, 168, 0, 1)));
    proc_route.output_interface_name = Some("eth0".into());
    proc_route.table = Some(RT_TABLE_MAIN as u32);
    proc_route
        .notes
        .push("table implied: /proc/net/route exposes only the main table (254)".into());
    proc_route.sources = vec![ObservationSource::ProcFs];
    sysfs.routes_v4 = SectionResult::Observed(vec![proc_route]);

    let snapshot = merge(vec![netlink, sysfs], 1);
    assert_eq!(
        snapshot.routes.len(),
        1,
        "same kernel route must deduplicate"
    );
    let merged = &snapshot.routes[0];
    assert_eq!(
        merged.sources,
        vec![ObservationSource::Netlink, ObservationSource::ProcFs]
    );
    // Netlink's richer fields survive; procfs note is preserved.
    assert_eq!(merged.protocol, Some(RouteProtocol::Dhcp));
    assert_eq!(merged.output_interface, Some(2));
    assert_eq!(merged.output_interface_name.as_deref(), Some("eth0"));
    assert!(!merged.notes.is_empty());
}

#[test]
fn local_table_routes_are_kept_and_never_merged_into_main() {
    let mut netlink = observation(ProviderId::Netlink, 0);
    let mut nl_iface = iface(2, "eth0", InterfaceKind::Ethernet);
    nl_iface.sources = vec![ObservationSource::Netlink];
    netlink.interfaces = SectionResult::Observed(vec![nl_iface]);
    let mut local = v4_route("192.168.0.42", 32, None, Some(2));
    local.table = Some(RT_TABLE_LOCAL as u32);
    local.sources = vec![ObservationSource::Netlink];
    netlink.routes_v4 = SectionResult::Observed(vec![local]);

    let mut sysfs = observation(ProviderId::SysFs, 1);
    let mut s_iface = iface(2, "eth0", InterfaceKind::Ethernet);
    s_iface.sources = vec![ObservationSource::SysFs];
    sysfs.interfaces = SectionResult::Observed(vec![s_iface]);
    let mut proc_route = v4_route("192.168.0.42", 32, None, None);
    proc_route.output_interface_name = Some("eth0".into());
    proc_route.table = Some(RT_TABLE_MAIN as u32);
    proc_route.sources = vec![ObservationSource::ProcFs];
    sysfs.routes_v4 = SectionResult::Observed(vec![proc_route]);

    let snapshot = merge(vec![netlink, sysfs], 1);
    // Different tables are different routes: both must be preserved.
    assert_eq!(snapshot.routes.len(), 2);
    let tables: Vec<u32> = snapshot.routes.iter().filter_map(|r| r.table).collect();
    assert!(tables.contains(&(RT_TABLE_MAIN as u32)));
    assert!(tables.contains(&(RT_TABLE_LOCAL as u32)));
}

// ── IPv6 procfs route fallback (WO-03 M4) ───────────────────────────────────
// /proc/net/ipv6_route carries no table column and its rows span multiple
// tables, so its observations have table: None. The same prefix + metric +
// next hop with the other side's table known is the same kernel fact: the
// table-less observation merges into the knowing one (which keeps its
// table), never the other way around.

/// A /proc/net/ipv6_route-shaped observation: table None, oif by name,
/// procfs provenance, and the file's inherent caveats as notes.
fn v6_procfs_route(
    dst: &str,
    prefix: u8,
    gateway: Option<IpAddr>,
    oif_name: &str,
    metric: u32,
) -> Route {
    Route {
        family: AddressFamily::Ipv6,
        destination: Some(IpNet::new(dst.parse().unwrap(), prefix)),
        gateway,
        output_interface: None,
        output_interface_name: Some(oif_name.into()),
        protocol: None,
        scope: None,
        route_type: None,
        metric: Some(metric),
        table: None,
        flags: Some(vec!["up".into()]),
        next_hops: NextHops::Single,
        sources: vec![ObservationSource::ProcFs],
        notes: vec!["/proc/net/ipv6_route exposes no routing-table id".to_string()],
    }
}

#[test]
fn ipv6_procfs_route_merges_into_netlink_route_with_known_table() {
    // The live shape on every system: ::1/128 is a local-table (255) route;
    // /proc/net/ipv6_route reports it without a table. The wildcard merge
    // must unite them instead of showing ::1/128 twice.
    let mut netlink = observation(ProviderId::Netlink, 0);
    netlink.interfaces = SectionResult::Observed(vec![iface(1, "lo", InterfaceKind::Loopback)]);
    let mut local = v6_route("::1", 128, None, Some(1));
    local.table = Some(RT_TABLE_LOCAL as u32);
    local.sources = vec![ObservationSource::Netlink];
    netlink.routes_v6 = SectionResult::Observed(vec![local]);

    let mut sysfs = observation(ProviderId::SysFs, 1);
    sysfs.interfaces = SectionResult::Observed(vec![iface(1, "lo", InterfaceKind::Loopback)]);
    sysfs.routes_v6 = SectionResult::Observed(vec![v6_procfs_route("::1", 128, None, "lo", 0)]);

    let snapshot = merge(vec![netlink, sysfs], 1);
    assert_eq!(
        snapshot.routes.len(),
        1,
        "the table-less procfs observation is the same kernel route"
    );
    let route = &snapshot.routes[0];
    assert_eq!(
        route.table,
        Some(RT_TABLE_LOCAL as u32),
        "the knowing observation's table stands; None never overwrites Some"
    );
    assert_eq!(
        route.sources,
        vec![ObservationSource::Netlink, ObservationSource::ProcFs]
    );
    assert!(
        route
            .notes
            .iter()
            .any(|n| n.contains("no routing-table id")),
        "the procfs caveat survives as provenance"
    );
}

#[test]
fn ipv6_procfs_gateway_route_merges_and_netlink_fields_win() {
    // A gateway route observed by both: netlink knows protocol and table;
    // procfs knows flags. The merged route carries netlink's protocol and
    // table, and procfs's flags only where netlink had none.
    let mut netlink = observation(ProviderId::Netlink, 0);
    netlink.interfaces = SectionResult::Observed(vec![iface(2, "eth0", InterfaceKind::Ethernet)]);
    let mut gw = v6_route(
        "2001:db8:9::",
        64,
        Some("2001:db8::2".parse().unwrap()),
        Some(2),
    );
    gw.metric = Some(1024);
    gw.protocol = Some(RouteProtocol::Static);
    gw.sources = vec![ObservationSource::Netlink];
    netlink.routes_v6 = SectionResult::Observed(vec![gw]);

    let mut sysfs = observation(ProviderId::SysFs, 1);
    sysfs.interfaces = SectionResult::Observed(vec![iface(2, "eth0", InterfaceKind::Ethernet)]);
    sysfs.routes_v6 = SectionResult::Observed(vec![v6_procfs_route(
        "2001:db8:9::",
        64,
        Some("2001:db8::2".parse().unwrap()),
        "eth0",
        1024,
    )]);

    let snapshot = merge(vec![netlink, sysfs], 1);
    assert_eq!(snapshot.routes.len(), 1);
    let route = &snapshot.routes[0];
    assert_eq!(route.protocol, Some(RouteProtocol::Static), "netlink wins");
    assert_eq!(
        route.flags,
        Some(vec!["up".to_string()]),
        "procfs fills the flags netlink did not observe"
    );
    assert_eq!(route.metric, Some(1024));
    assert_eq!(route.table, Some(RT_TABLE_MAIN as u32));
    assert_eq!(route.output_interface, Some(2));
}

#[test]
fn ipv6_procfs_route_with_different_metric_or_hop_never_merges() {
    // Same prefix, different metric: a different kernel route. The
    // table-less wildcard must not become a same-prefix wildcard.
    let mut netlink = observation(ProviderId::Netlink, 0);
    netlink.interfaces = SectionResult::Observed(vec![iface(2, "eth0", InterfaceKind::Ethernet)]);
    let mut r = v6_route("2001:db8::", 64, None, Some(2));
    r.metric = Some(256);
    r.sources = vec![ObservationSource::Netlink];
    netlink.routes_v6 = SectionResult::Observed(vec![r]);

    let mut sysfs = observation(ProviderId::SysFs, 1);
    sysfs.interfaces = SectionResult::Observed(vec![iface(2, "eth0", InterfaceKind::Ethernet)]);
    sysfs.routes_v6 = SectionResult::Observed(vec![
        // Different metric.
        v6_procfs_route("2001:db8::", 64, None, "eth0", 512),
        // Same metric, different output interface (a second, genuinely
        // distinct route).
        v6_procfs_route("2001:db8::", 64, None, "wlan0", 256),
    ]);

    let snapshot = merge(vec![netlink, sysfs], 2);
    assert_eq!(snapshot.routes.len(), 3, "no false merges: {snapshot:?}");
}

#[test]
fn ipv6_procfs_only_observation_keeps_unknown_table_and_survives_dedup() {
    // The fallback scenario: netlink unavailable, only /proc/net/ipv6_route.
    // The kernel prints its unreachable-default sentinel twice (one per
    // table) — identical lines dedup among themselves; the table stays
    // not observed, never invented.
    let mut sysfs = observation(ProviderId::SysFs, 1);
    sysfs.interfaces = SectionResult::Observed(vec![iface(1, "lo", InterfaceKind::Loopback)]);
    sysfs.routes_v6 = SectionResult::Observed(vec![
        v6_procfs_route("::1", 128, None, "lo", 0),
        v6_procfs_route("::", 0, None, "lo", u32::MAX),
        v6_procfs_route("::", 0, None, "lo", u32::MAX),
    ]);

    let snapshot = merge(vec![sysfs], 3);
    assert_eq!(snapshot.routes.len(), 2, "identical sentinel lines dedup");
    let unreachable = snapshot.routes.iter().find(|r| r.is_default()).unwrap();
    assert_eq!(
        unreachable.table, None,
        "table not observed, not fabricated"
    );
    assert_eq!(unreachable.metric, Some(u32::MAX));
    assert_eq!(
        unreachable.sources,
        vec![ObservationSource::ProcFs],
        "dedup unites provenance instead of duplicating it"
    );
}

#[test]
fn address_flags_fill_from_procfs() {
    let mut netlink = observation(ProviderId::Netlink, 0);
    netlink.addresses_v6 = SectionResult::Observed(vec![Address {
        ifindex: 2,
        family: AddressFamily::Ipv6,
        address: "fe80::1".parse().unwrap(),
        prefix_len: 64,
        scope: Some(AddressScope::Link),
        flags: None,
        label: None,
        sources: vec![ObservationSource::Netlink],
    }]);

    let mut sysfs = observation(ProviderId::SysFs, 1);
    sysfs.addresses_v6 = SectionResult::Observed(vec![Address {
        ifindex: 2,
        family: AddressFamily::Ipv6,
        address: "fe80::1".parse().unwrap(),
        prefix_len: 64,
        scope: Some(AddressScope::Link),
        flags: Some(vec!["permanent".into()]),
        label: None,
        sources: vec![ObservationSource::ProcFs],
    }]);

    let snapshot = merge(vec![netlink, sysfs], 1);
    assert_eq!(snapshot.addresses.len(), 1);
    assert_eq!(
        snapshot.addresses[0].flags,
        Some(vec!["permanent".to_string()])
    );
}

#[test]
fn oif_name_resolved_to_ifindex_during_merge() {
    // Sysfs-only observation: route names eth0, and the interface list
    // carries its ifindex; the merged route must gain the ifindex.
    let mut sysfs = observation(ProviderId::SysFs, 1);
    sysfs.interfaces = SectionResult::Observed(vec![iface(2, "eth0", InterfaceKind::Ethernet)]);
    let mut route = v4_route("0.0.0.0", 0, None, None);
    route.gateway = Some(IpAddr::V4(Ipv4Addr::new(192, 168, 0, 1)));
    route.output_interface_name = Some("eth0".into());
    route.metric = Some(600);
    route.sources = vec![ObservationSource::ProcFs];
    sysfs.routes_v4 = SectionResult::Observed(vec![route]);

    let snapshot = merge(vec![sysfs], 1);
    assert_eq!(snapshot.routes[0].output_interface, Some(2));
}

// ── Route ordering (WO-02 correction §3) ────────────────────────────────────
// The regression tests below assert the ACTUAL resulting sequence of the
// merged route list, not merely the presence of routes.

fn v6_route(dst: &str, prefix: u8, gateway: Option<IpAddr>, oif: Option<u32>) -> Route {
    Route {
        family: AddressFamily::Ipv6,
        destination: Some(IpNet::new(dst.parse().unwrap(), prefix)),
        gateway,
        output_interface: oif,
        output_interface_name: None,
        protocol: None,
        scope: None,
        route_type: Some(RouteType::Unicast),
        metric: Some(0),
        table: Some(RT_TABLE_MAIN as u32),
        flags: None,
        next_hops: NextHops::Single,
        sources: vec![],
        notes: vec![],
    }
}

/// Compact per-route label for sequence assertions: "family table dest/prefix
/// [via gateway] [metric m]".
fn sequence(snapshot: &araiguma_core::NetworkSnapshot) -> Vec<String> {
    snapshot
        .routes
        .iter()
        .map(|r| {
            format!(
                "{} {} {}{}{}",
                match r.family {
                    AddressFamily::Ipv4 => "v4",
                    AddressFamily::Ipv6 => "v6",
                },
                r.table.unwrap_or(RT_TABLE_MAIN as u32),
                r.destination
                    .as_ref()
                    .map(|d| d.to_string())
                    .unwrap_or_else(|| "unknown".into()),
                r.gateway.map(|g| format!(" via {g}")).unwrap_or_default(),
                r.metric
                    .map(|m| format!(" m{m}"))
                    .unwrap_or_else(|| " m?".into()),
            )
        })
        .collect()
}

fn routes_snapshot(routes: Vec<Route>) -> araiguma_core::NetworkSnapshot {
    let mut netlink = observation(ProviderId::Netlink, 0);
    netlink.routes_v4 = SectionResult::Observed(
        routes
            .iter()
            .filter(|r| r.family == AddressFamily::Ipv4)
            .cloned()
            .collect(),
    );
    netlink.routes_v6 = SectionResult::Observed(
        routes
            .iter()
            .filter(|r| r.family == AddressFamily::Ipv6)
            .cloned()
            .collect(),
    );
    merge(vec![netlink], 1)
}

#[test]
fn ipv4_default_routes_sort_first_then_broader_before_more_specific() {
    // Deliberately scrambled input order.
    let snapshot = routes_snapshot(vec![
        v4_route("192.168.1.0", 24, None, Some(2)),
        v4_route("10.0.0.0", 8, None, Some(2)),
        v4_route("0.0.0.0", 0, Some("192.168.1.1".parse().unwrap()), Some(2)),
        v4_route("172.16.5.0", 24, None, Some(3)),
        v4_route("10.1.0.0", 16, None, Some(2)),
    ]);
    assert_eq!(
        sequence(&snapshot),
        vec![
            "v4 254 0.0.0.0/0 via 192.168.1.1 m0", // default first
            "v4 254 10.0.0.0/8 m0",                // then ascending prefix length
            "v4 254 10.1.0.0/16 m0",
            "v4 254 172.16.5.0/24 m0",
            "v4 254 192.168.1.0/24 m0",
        ]
    );
}

#[test]
fn ipv6_defaults_and_specifics_stay_in_their_own_family_group() {
    let snapshot = routes_snapshot(vec![
        v6_route("2001:db8:1::", 48, None, Some(2)),
        v6_route("::", 0, Some("fe80::1".parse().unwrap()), Some(2)),
        v4_route("192.168.1.0", 24, None, Some(2)),
        v6_route("fe80::", 64, None, Some(2)),
        v4_route("0.0.0.0", 0, Some("192.168.1.1".parse().unwrap()), Some(2)),
    ]);
    assert_eq!(
        sequence(&snapshot),
        vec![
            "v4 254 0.0.0.0/0 via 192.168.1.1 m0",
            "v4 254 192.168.1.0/24 m0",
            // The IPv6 group is never interleaved with IPv4.
            "v6 254 ::/0 via fe80::1 m0",
            "v6 254 2001:db8:1::/48 m0",
            "v6 254 fe80::/64 m0",
        ]
    );
}

#[test]
fn multiple_default_routes_remain_visible_and_order_by_metric() {
    let mut low = v4_route("0.0.0.0", 0, Some("10.0.0.1".parse().unwrap()), Some(2));
    low.metric = Some(100);
    let mut mid = v4_route("0.0.0.0", 0, Some("10.0.0.2".parse().unwrap()), Some(3));
    mid.metric = Some(600);
    let mut high = v4_route("0.0.0.0", 0, Some("10.0.0.3".parse().unwrap()), Some(4));
    high.metric = Some(1200);
    let snapshot = routes_snapshot(vec![high, mid, low]);
    let defaults: Vec<&Route> = snapshot.routes.iter().filter(|r| r.is_default()).collect();
    assert_eq!(defaults.len(), 3, "every default route stays visible");
    assert_eq!(
        sequence(&snapshot),
        vec![
            "v4 254 0.0.0.0/0 via 10.0.0.1 m100", // metric ascending
            "v4 254 0.0.0.0/0 via 10.0.0.2 m600",
            "v4 254 0.0.0.0/0 via 10.0.0.3 m1200",
        ]
    );
}

#[test]
fn equal_metric_defaults_order_deterministically_by_gateway() {
    let mut a = v4_route(
        "0.0.0.0",
        0,
        Some("192.168.1.254".parse().unwrap()),
        Some(2),
    );
    a.metric = Some(50);
    let mut b = v4_route("0.0.0.0", 0, Some("192.168.1.1".parse().unwrap()), Some(3));
    b.metric = Some(50);
    let mut onlink = v4_route("0.0.0.0", 0, None, Some(5));
    onlink.metric = Some(50);
    let snapshot = routes_snapshot(vec![a, b, onlink]);
    assert_eq!(
        sequence(&snapshot),
        vec![
            // Same metric: on-link first, then gateways ascending — stable.
            "v4 254 0.0.0.0/0 m50",
            "v4 254 0.0.0.0/0 via 192.168.1.1 m50",
            "v4 254 0.0.0.0/0 via 192.168.1.254 m50",
        ]
    );
}

#[test]
fn missing_metric_sorts_after_observed_metrics_without_claiming_preference() {
    let mut zero = v4_route("0.0.0.0", 0, Some("10.0.0.1".parse().unwrap()), Some(2));
    zero.metric = Some(0);
    let mut six = v4_route("0.0.0.0", 0, Some("10.0.0.2".parse().unwrap()), Some(3));
    six.metric = Some(600);
    let mut unknown = v4_route("0.0.0.0", 0, Some("10.0.0.3".parse().unwrap()), Some(4));
    unknown.metric = None;
    let snapshot = routes_snapshot(vec![unknown, six, zero]);
    assert_eq!(
        sequence(&snapshot),
        vec![
            "v4 254 0.0.0.0/0 via 10.0.0.1 m0",
            "v4 254 0.0.0.0/0 via 10.0.0.2 m600",
            "v4 254 0.0.0.0/0 via 10.0.0.3 m?", // not-observed metric last
        ]
    );
}

#[test]
fn routes_from_different_tables_stay_in_separate_ordered_groups() {
    let mut main_default = v4_route("0.0.0.0", 0, Some("10.0.0.1".parse().unwrap()), Some(2));
    main_default.table = Some(254);
    let mut main_specific = v4_route("10.0.0.0", 8, None, Some(2));
    main_specific.table = Some(254);
    let mut local_host = v4_route("127.0.0.0", 8, None, Some(1));
    local_host.table = Some(255);
    let mut custom_default = v4_route("0.0.0.0", 0, Some("10.9.9.9".parse().unwrap()), Some(6));
    custom_default.table = Some(100);
    let mut custom_specific = v4_route("198.51.100.0", 24, None, Some(6));
    custom_specific.table = Some(100);
    let snapshot = routes_snapshot(vec![
        local_host,
        main_specific,
        custom_specific,
        main_default,
        custom_default,
    ]);
    assert_eq!(
        sequence(&snapshot),
        vec![
            // Table groups ascending, each group self-contained with its
            // own default first — never merged into one ordering.
            "v4 100 0.0.0.0/0 via 10.9.9.9 m0",
            "v4 100 198.51.100.0/24 m0",
            "v4 254 0.0.0.0/0 via 10.0.0.1 m0",
            "v4 254 10.0.0.0/8 m0",
            "v4 255 127.0.0.0/8 m0",
        ]
    );
}

#[test]
fn routes_without_an_observed_destination_sort_after_observed_ones() {
    let mut unknown = v4_route("0.0.0.0", 0, Some("10.0.0.1".parse().unwrap()), Some(2));
    unknown.destination = None;
    let snapshot = routes_snapshot(vec![
        unknown,
        v4_route("192.168.1.0", 24, None, Some(2)),
        v4_route("0.0.0.0", 0, Some("10.0.0.2".parse().unwrap()), Some(3)),
    ]);
    assert_eq!(
        sequence(&snapshot),
        vec![
            "v4 254 0.0.0.0/0 via 10.0.0.2 m0",
            "v4 254 192.168.1.0/24 m0",
            "v4 254 unknown via 10.0.0.1 m0", // destination-less last
        ]
    );
}

// ── MAC observation merge semantics (WO-02 correction §5) ──────────────────

#[test]
fn mac_observation_merge_respects_priority_and_only_fills_not_observed() {
    // Netlink (priority 0) observed an ABSENCE (wireguard); sysfs (priority
    // 1) reports a present address from the same interface: the
    // higher-priority observed absence stands — it is never "filled".
    let mut netlink = observation(ProviderId::Netlink, 0);
    let mut wg = iface(10, "wg0", InterfaceKind::Wireguard);
    wg.mac_address = MacObservation::Absent;
    wg.sources = vec![ObservationSource::Netlink];
    netlink.interfaces = SectionResult::Observed(vec![wg]);

    let mut sysfs = observation(ProviderId::SysFs, 1);
    let mut wg_sysfs = iface(10, "wg0", InterfaceKind::Virtual);
    wg_sysfs.mac_address = MacObservation::Present("00:11:22:33:44:55".into());
    wg_sysfs.sources = vec![ObservationSource::SysFs];
    sysfs.interfaces = SectionResult::Observed(vec![wg_sysfs]);

    let snapshot = merge(vec![netlink, sysfs], 1);
    assert_eq!(snapshot.interfaces[0].mac_address, MacObservation::Absent);
}

#[test]
fn mac_observation_not_observed_is_filled_by_lower_priority_provider() {
    // Netlink did not obtain the address; sysfs did: the gap is filled.
    let mut netlink = observation(ProviderId::Netlink, 0);
    let mut weird = iface(12, "weird0", InterfaceKind::Other);
    weird.mac_address = MacObservation::NotObserved;
    weird.sources = vec![ObservationSource::Netlink];
    netlink.interfaces = SectionResult::Observed(vec![weird]);

    let mut sysfs = observation(ProviderId::SysFs, 1);
    let mut weird_sysfs = iface(12, "weird0", InterfaceKind::Ethernet);
    weird_sysfs.mac_address = MacObservation::Present("66:2d:1a:55:aa:bb".into());
    weird_sysfs.sources = vec![ObservationSource::SysFs];
    sysfs.interfaces = SectionResult::Observed(vec![weird_sysfs]);

    let snapshot = merge(vec![netlink, sysfs], 1);
    assert_eq!(
        snapshot.interfaces[0].mac_address,
        MacObservation::Present("66:2d:1a:55:aa:bb".into())
    );
}

#[test]
fn mac_observation_present_survives_lower_priority_absent() {
    // Netlink observed a real address; sysfs saw an empty file: the
    // present observation stands.
    let mut netlink = observation(ProviderId::Netlink, 0);
    let mut eth = iface(2, "eth0", InterfaceKind::Ethernet);
    eth.mac_address = MacObservation::Present("52:54:00:12:34:56".into());
    eth.sources = vec![ObservationSource::Netlink];
    netlink.interfaces = SectionResult::Observed(vec![eth]);

    let mut sysfs = observation(ProviderId::SysFs, 1);
    let mut eth_sysfs = iface(2, "eth0", InterfaceKind::Ethernet);
    eth_sysfs.mac_address = MacObservation::Absent;
    eth_sysfs.sources = vec![ObservationSource::SysFs];
    sysfs.interfaces = SectionResult::Observed(vec![eth_sysfs]);

    let snapshot = merge(vec![netlink, sysfs], 1);
    assert_eq!(
        snapshot.interfaces[0].mac_address,
        MacObservation::Present("52:54:00:12:34:56".into())
    );
}

// ── Multipath merge semantics (WO-03 M3) ────────────────────────────────────

fn hop(gateway: Option<IpAddr>, oif: Option<u32>, weight: Option<u16>) -> NextHop {
    NextHop {
        gateway,
        output_interface: oif,
        output_interface_name: None,
        weight,
        flags: Some(vec![]),
    }
}

fn multipath_route(dst: &str, prefix: u8, hops: Vec<NextHop>) -> Route {
    Route {
        family: AddressFamily::Ipv4,
        destination: Some(IpNet::new(dst.parse().unwrap(), prefix)),
        gateway: None,
        output_interface: None,
        output_interface_name: None,
        protocol: Some(RouteProtocol::Static),
        scope: Some(RouteScope::Universe),
        route_type: Some(RouteType::Unicast),
        metric: Some(0),
        table: Some(254),
        flags: Some(vec![]),
        next_hops: NextHops::Multi(hops),
        sources: vec![ObservationSource::Netlink],
        notes: vec![],
    }
}

#[test]
fn multipath_route_deduplicates_against_procfs_first_hop_view() {
    // Verified live: /proc/net/route shows only the FIRST hop of a
    // multipath route. The procfs line must merge into the netlink
    // multipath observation (provenance united), not appear as a separate
    // route — and the hop set must survive intact.
    let mut netlink = observation(ProviderId::Netlink, 0);
    netlink.interfaces = SectionResult::Observed(vec![
        iface(2, "eth0", InterfaceKind::Ethernet),
        iface(3, "eth1", InterfaceKind::Ethernet),
    ]);
    let multi = multipath_route(
        "10.9.0.0",
        24,
        vec![
            hop(Some("10.0.0.1".parse().unwrap()), Some(2), Some(2)),
            hop(Some("10.1.0.1".parse().unwrap()), Some(3), Some(3)),
        ],
    );
    netlink.routes_v4 = SectionResult::Observed(vec![multi]);

    let mut sysfs = observation(ProviderId::SysFs, 1);
    sysfs.interfaces = SectionResult::Observed(vec![
        iface(2, "eth0", InterfaceKind::Ethernet),
        iface(3, "eth1", InterfaceKind::Ethernet),
    ]);
    let mut proc_view = v4_route("10.9.0.0", 24, None, None);
    proc_view.gateway = Some("10.0.0.1".parse().unwrap());
    proc_view.output_interface_name = Some("eth0".into());
    proc_view.table = Some(254);
    proc_view.sources = vec![ObservationSource::ProcFs];
    proc_view.notes =
        vec!["table implied: /proc/net/route exposes only the main table (254)".into()];
    sysfs.routes_v4 = SectionResult::Observed(vec![proc_view]);

    let snapshot = merge(vec![netlink, sysfs], 1);
    assert_eq!(snapshot.routes.len(), 1, "one kernel route, not two");
    let merged = &snapshot.routes[0];
    assert_eq!(
        merged.sources,
        vec![ObservationSource::Netlink, ObservationSource::ProcFs]
    );
    // The full hop set survives; route-level gateway/oif stay None.
    match &merged.next_hops {
        NextHops::Multi(hops) => {
            assert_eq!(hops.len(), 2);
            assert_eq!(hops[0].gateway.unwrap().to_string(), "10.0.0.1");
            assert_eq!(hops[0].output_interface, Some(2));
            assert_eq!(hops[0].weight, Some(2));
            assert_eq!(hops[1].gateway.unwrap().to_string(), "10.1.0.1");
            assert_eq!(hops[1].output_interface, Some(3));
            // Hop names are enriched through the merged interface list.
            assert_eq!(hops[0].output_interface_name.as_deref(), Some("eth0"));
            assert_eq!(hops[1].output_interface_name.as_deref(), Some("eth1"));
        }
        other => panic!("hop set must survive the merge: {other:?}"),
    }
    assert_eq!(merged.gateway, None, "hops carry the gateways");
    assert_eq!(merged.output_interface, None);
}

#[test]
fn single_path_routes_with_different_gateways_stay_separate() {
    let snapshot = routes_snapshot(vec![
        {
            let mut a = v4_route("10.9.0.0", 24, None, Some(2));
            a.gateway = Some("10.0.0.1".parse().unwrap());
            a
        },
        {
            let mut b = v4_route("10.9.0.0", 24, None, Some(3));
            b.gateway = Some("10.1.0.1".parse().unwrap());
            b
        },
    ]);
    assert_eq!(
        snapshot.routes.len(),
        2,
        "different ECMP members, different routes"
    );
}

#[test]
fn multipath_routes_with_different_hop_sets_stay_separate() {
    let snapshot = routes_snapshot(vec![
        multipath_route(
            "10.9.0.0",
            24,
            vec![
                hop(Some("10.0.0.1".parse().unwrap()), Some(2), Some(1)),
                hop(Some("10.1.0.1".parse().unwrap()), Some(3), Some(1)),
            ],
        ),
        multipath_route(
            "10.9.0.0",
            24,
            vec![
                hop(Some("10.2.0.1".parse().unwrap()), Some(4), Some(1)),
                hop(Some("10.3.0.1".parse().unwrap()), Some(5), Some(1)),
            ],
        ),
    ]);
    assert_eq!(
        snapshot.routes.len(),
        2,
        "different multipath routes on the same prefix stay separate"
    );
}

#[test]
fn no_next_hop_is_silently_discarded_in_the_merge() {
    // A multipath observation arriving at a lower priority than a
    // single-path one for the same hop still replaces the representation
    // (single -> multi upgrade keeps every hop).
    let mut netlink = observation(ProviderId::Netlink, 0);
    netlink.routes_v4 = SectionResult::Observed(vec![multipath_route(
        "10.9.0.0",
        24,
        vec![
            hop(Some("10.0.0.1".parse().unwrap()), Some(2), Some(2)),
            hop(Some("10.1.0.1".parse().unwrap()), Some(3), Some(3)),
        ],
    )]);
    let snapshot = merge(vec![netlink], 1);
    match &snapshot.routes[0].next_hops {
        NextHops::Multi(hops) => assert_eq!(hops.len(), 2),
        other => panic!("both hops preserved: {other:?}"),
    }
}
