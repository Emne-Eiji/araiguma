//! Merge-layer behavior: netlink primary, sysfs fills gaps, failures
//! degrade without destroying observations.

use std::net::{IpAddr, Ipv4Addr};

use araiguma_core::merge::merge;
use araiguma_core::model::{
    Address, AddressFamily, AddressScope, AdminState, Interface, InterfaceKind, IpNet,
    ObservationSource, OperState, Route, RouteProtocol, RouteScope, RouteType, RT_TABLE_LOCAL,
    RT_TABLE_MAIN,
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
        mac_address: None,
        mtu: None,
        tx_queue_len: None,
        master_ifindex: None,
        parent_ifindex: None,
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
    eth.mac_address = Some("52:54:00:12:34:56".into());
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
    assert_eq!(merged.mac_address.as_deref(), Some("52:54:00:12:34:56"));
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
