//! sysfs/procfs provider behavior against the committed fixture trees
//! (all synthetic; see fixtures/README.md) and failure injection.

use std::path::PathBuf;

use araiguma_core::model::{
    AdminState, InterfaceKind, MacObservation, ObservationSource, OperState,
};
use araiguma_core::provider::{DiscoveryProvider, ProviderId, ProviderState, SectionResult};
use araiguma_sysfs::SysFsProvider;

fn fixture_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures")
}

fn provider(sys_scenario: &str, proc_scenario: &str) -> SysFsProvider {
    SysFsProvider::with_roots(
        fixture_root().join("sysfs").join(sys_scenario),
        fixture_root().join("procfs").join(proc_scenario),
    )
}

fn observe(provider: &mut SysFsProvider) -> araiguma_core::ProviderObservation {
    futures::executor::block_on(provider.observe())
}

#[test]
fn basic_workstation_interfaces() {
    let mut p = provider("basic-workstation", "empty");
    let observation = observe(&mut p);

    assert_eq!(observation.provider, ProviderId::SysFs);
    assert_eq!(observation.state(), ProviderState::Degraded);

    let interfaces = observation.interfaces.items();
    assert_eq!(interfaces.len(), 3);

    let lo = interfaces.iter().find(|i| i.name == "lo").unwrap();
    assert_eq!(lo.kind, InterfaceKind::Loopback);
    assert_eq!(lo.oper_state, Some(OperState::Unknown));
    assert_eq!(lo.admin_state, Some(AdminState::Up));
    assert_eq!(lo.mtu, Some(65536));
    assert_eq!(lo.mac_address.present(), Some("00:00:00:00:00:00"));

    let eth0 = interfaces.iter().find(|i| i.name == "eth0").unwrap();
    assert_eq!(eth0.kind, InterfaceKind::Ethernet);
    assert_eq!(eth0.oper_state, Some(OperState::Up));
    assert_eq!(eth0.admin_state, Some(AdminState::Up));
    assert_eq!(eth0.mtu, Some(1500));
    assert_eq!(eth0.mac_address.present(), Some("52:54:00:12:34:56"));
    // The sysfs classification caveat is recorded per interface.
    assert!(eth0
        .notes
        .iter()
        .any(|n| n.contains("sysfs cannot distinguish")));

    let wlan0 = interfaces.iter().find(|i| i.name == "wlan0").unwrap();
    assert_eq!(wlan0.kind, InterfaceKind::Wireless);
    assert_eq!(wlan0.oper_state, Some(OperState::Down));
    assert_eq!(wlan0.admin_state, Some(AdminState::Down));

    for iface in interfaces {
        assert_eq!(iface.sources, vec![ObservationSource::SysFs]);
    }
}

#[test]
fn virtual_interfaces_classification() {
    let mut p = provider("virtual-interfaces", "empty");
    let observation = observe(&mut p);
    let interfaces = observation.interfaces.items();

    let br0 = interfaces.iter().find(|i| i.name == "br0").unwrap();
    assert_eq!(br0.kind, InterfaceKind::Bridge); // via the `bridge` dir
    assert_eq!(br0.oper_state, Some(OperState::Down));

    let wg0 = interfaces.iter().find(|i| i.name == "wg0").unwrap();
    assert_eq!(wg0.kind, InterfaceKind::Virtual); // ARPHRD_NONE
    assert_eq!(wg0.mac_address, MacObservation::Absent); // empty address file = observed absence
    assert_eq!(wg0.mtu, Some(1420));
    assert!(wg0.notes.iter().any(|n| n.contains("ARPHRD_NONE")));

    let dummy0 = interfaces.iter().find(|i| i.name == "dummy0").unwrap();
    // sysfs sees ARPHRD_ETHER: classified Ethernet, with the caveat that it
    // cannot be distinguished from real Ethernet.
    assert_eq!(dummy0.kind, InterfaceKind::Ethernet);
    assert!(dummy0
        .notes
        .iter()
        .any(|n| n.contains("sysfs cannot distinguish")));
}

#[test]
fn routerish_procfs_addresses_and_routes() {
    let mut p = provider("empty", "router-ish");
    let observation = observe(&mut p);

    let addresses = observation.addresses_v6.items();
    assert_eq!(addresses.len(), 3);
    let lo_addr = addresses
        .iter()
        .find(|a| a.address.to_string() == "::1")
        .unwrap();
    assert_eq!(lo_addr.ifindex, 1);
    assert_eq!(
        lo_addr.scope,
        Some(araiguma_core::model::AddressScope::Host)
    );
    assert_eq!(lo_addr.flags, Some(vec!["permanent".to_string()]));
    let global = addresses
        .iter()
        .find(|a| a.address.to_string() == "2001:db8::42")
        .unwrap();
    assert_eq!(
        global.scope,
        Some(araiguma_core::model::AddressScope::Global)
    );

    let routes = observation.routes_v4.items();
    assert_eq!(routes.len(), 4);
    let default_route = routes
        .iter()
        .find(|r| r.destination.unwrap().is_default())
        .unwrap();
    assert_eq!(
        default_route.gateway.unwrap().to_string(),
        "192.168.0.1",
        "0100A8C0 is little-endian for 192.168.0.1"
    );
    assert_eq!(default_route.metric, Some(600));
    assert_eq!(default_route.table, Some(254));
    assert_eq!(default_route.output_interface_name.as_deref(), Some("eth0"));
    assert_eq!(
        default_route.output_interface, None,
        "no sysfs interfaces to resolve"
    );
    assert!(default_route
        .notes
        .iter()
        .any(|n| n.contains("table implied")));
    // procfs cannot observe these — they stay None, never fabricated.
    assert_eq!(default_route.protocol, None);
    assert_eq!(default_route.scope, None);
    assert_eq!(default_route.route_type, None);

    let onlink = routes
        .iter()
        .find(|r| r.destination.unwrap().to_string() == "192.168.0.0/24")
        .unwrap();
    assert_eq!(onlink.gateway, None, "zero gateway means on-link");
}

/// WO-03 M4: /proc/net/ipv6_route parsing against the synthetic fixture
/// (whose format was verified live against kernel 5.10 and cross-checked
/// with `ip -6 route`).
#[test]
fn routerish_ipv6_routes() {
    let mut p = provider("empty", "router-ish");
    let observation = observe(&mut p);

    let routes = observation.routes_v6.items();
    assert_eq!(routes.len(), 6);

    // Gateway route: 2001:db8:9::/64 via 2001:db8::2 on eth0, metric
    // 0x400 = 1024, flags up|gateway.
    let gateway = routes
        .iter()
        .find(|r| r.destination.unwrap().to_string() == "2001:db8:9::/64")
        .unwrap();
    assert_eq!(
        gateway.gateway.unwrap().to_string(),
        "2001:db8::2",
        "nexthop column is big-endian hex, unlike /proc/net/route"
    );
    assert_eq!(gateway.metric, Some(1024));
    assert_eq!(
        gateway.flags,
        Some(vec!["up".to_string(), "gateway".to_string()])
    );
    assert_eq!(gateway.output_interface_name.as_deref(), Some("eth0"));
    assert_eq!(
        gateway.output_interface, None,
        "no sysfs interfaces to resolve"
    );
    assert_eq!(gateway.next_hops, araiguma_core::model::NextHops::Single);

    // The file exposes no table id — not observed, never fabricated, with
    // a note (its rows span main and local tables).
    assert_eq!(gateway.table, None);
    assert!(gateway
        .notes
        .iter()
        .any(|n| n.contains("no routing-table id")));

    // The local-table ::1/128 keeps its observed-absence gateway and the
    // RTF_LOCAL flag mapping (0x80200001 = local|nonexthop|up).
    let local = routes
        .iter()
        .find(|r| r.destination.unwrap().to_string() == "::1/128")
        .unwrap();
    assert_eq!(local.gateway, None, "all-zero nexthop = on-link (observed)");
    assert_eq!(local.metric, Some(0));
    assert_eq!(
        local.flags,
        Some(
            vec!["up", "nonexthop", "local"]
                .into_iter()
                .map(String::from)
                .collect()
        )
    );

    // The kernel's unreachable-default sentinel (metric ffffffff,
    // reject|nonexthop) parses like any other row.
    let unreachable = routes
        .iter()
        .find(|r| r.destination.unwrap().is_default())
        .unwrap();
    assert_eq!(unreachable.metric, Some(u32::MAX));
    assert_eq!(
        unreachable.flags,
        Some(vec!["reject".to_string(), "nonexthop".to_string()])
    );

    // procfs cannot observe these — they stay None, never fabricated.
    assert_eq!(gateway.protocol, None);
    assert_eq!(gateway.scope, None);
    assert_eq!(gateway.route_type, None);
    for route in routes {
        assert_eq!(route.sources, vec![ObservationSource::ProcFs]);
        assert_eq!(route.family, araiguma_core::model::AddressFamily::Ipv6);
    }
}

#[test]
fn combined_laptop_full_run_and_merge() {
    let mut p = provider("combined-laptop", "combined-laptop");
    let observation = observe(&mut p);
    assert_eq!(observation.state(), ProviderState::Degraded);
    assert_eq!(observation.interfaces.items().len(), 3);

    // Routes resolve oif names to ifindexes through the sysfs listing.
    let snapshot = araiguma_core::merge::merge(vec![observation], 42);
    let default_route = snapshot
        .routes
        .iter()
        .find(|r| r.is_default())
        .expect("default route");
    assert_eq!(default_route.output_interface, Some(2));
    assert_eq!(default_route.output_interface_name.as_deref(), Some("eth0"));
    assert_eq!(snapshot.timestamp_ms, 42);
    assert_eq!(snapshot.providers.len(), 1);
    assert_eq!(snapshot.providers[0].state, ProviderState::Degraded);

    // IPv6 addresses landed on the right interfaces.
    let eth0_addrs = snapshot.addresses_on(2);
    assert_eq!(eth0_addrs.len(), 2);

    // IPv6 routes (WO-03 M4) resolve oif names through the sysfs listing
    // and land in the merged snapshot with observed-absence semantics.
    let v6: Vec<_> = snapshot
        .routes
        .iter()
        .filter(|r| r.family == araiguma_core::model::AddressFamily::Ipv6)
        .collect();
    assert_eq!(v6.len(), 5, "combined-laptop ipv6_route has 5 rows");
    let fe80 = v6
        .iter()
        .find(|r| r.destination.unwrap().to_string() == "fe80::/64")
        .unwrap();
    assert_eq!(fe80.output_interface, Some(2), "eth0 resolves via sysfs");
    assert_eq!(fe80.gateway, None);
    let unreachable = v6
        .iter()
        .find(|r| r.destination.unwrap().is_default())
        .expect("the kernel sentinel default is in the file");
    assert_eq!(unreachable.metric, Some(u32::MAX));
    assert_eq!(unreachable.table, None, "no table column in the file");
}

#[test]
fn empty_observation_is_valid() {
    let mut p = provider("empty", "empty");
    let observation = observe(&mut p);
    assert!(matches!(
        observation.interfaces,
        SectionResult::Observed(ref items) if items.is_empty()
    ));
    assert!(matches!(
        observation.addresses_v6,
        SectionResult::Observed(ref items) if items.is_empty()
    ));
    assert!(matches!(
        observation.routes_v4,
        SectionResult::Observed(ref items) if items.is_empty()
    ));
    // An EXISTING empty ipv6_route file is an observed empty route table —
    // evidence, unlike a missing file (see missing_roots_fail_without_panicking).
    assert!(matches!(
        observation.routes_v6,
        SectionResult::Observed(ref items) if items.is_empty()
    ));
    assert_eq!(observation.state(), ProviderState::Degraded);
}

#[test]
fn missing_roots_fail_without_panicking() {
    let mut p =
        SysFsProvider::with_roots("/nonexistent-araiguma-sys", "/nonexistent-araiguma-proc");
    let observation = observe(&mut p);
    assert!(matches!(observation.interfaces, SectionResult::Failed(_)));
    assert!(matches!(observation.addresses_v6, SectionResult::Failed(_)));
    assert!(matches!(observation.routes_v4, SectionResult::Failed(_)));
    // A MISSING /proc/net/ipv6_route is a failed section, never an
    // observed "no IPv6 routes": absence of the file is not evidence
    // about the route table (WO-03 M4).
    assert!(matches!(
        observation.routes_v6,
        SectionResult::Failed(ref error) if error.contains("cannot read")
    ));
    assert_eq!(observation.state(), ProviderState::Unavailable);
}

#[test]
fn bridge_master_symlink_resolves_to_ifindex() {
    // Built in a temp dir: br0 (bridge) with enslaved port eth1.
    let dir = std::env::temp_dir().join(format!("araiguma-sysfs-master-{}", std::process::id()));
    let net = dir.join("class/net");
    let br0 = net.join("br0");
    let eth1 = net.join("eth1");
    std::fs::create_dir_all(br0.join("bridge")).unwrap();
    std::fs::create_dir_all(&eth1).unwrap();
    for (path, content) in [
        (br0.join("ifindex"), "5"),
        (br0.join("type"), "1"),
        (br0.join("flags"), "0x1002"),
        (eth1.join("ifindex"), "6"),
        (eth1.join("type"), "1"),
        (eth1.join("flags"), "0x1003"),
    ] {
        std::fs::write(&path, content).unwrap();
    }
    std::os::unix::fs::symlink("../../br0", eth1.join("master")).unwrap();

    let mut p = SysFsProvider::with_roots(&dir, "/nonexistent-araiguma-proc");
    let observation = observe(&mut p);
    let eth1_observed = observation
        .interfaces
        .items()
        .iter()
        .find(|i| i.name == "eth1")
        .unwrap();
    assert_eq!(eth1_observed.master_ifindex, Some(5));
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn mac_observation_three_states_from_sysfs() {
    // WO-02 correction §5: a present address, an existing-but-empty
    // address file (observed absence), and a missing address file (not
    // observed) are three distinct states.
    let tmp = std::env::temp_dir().join(format!("araiguma-sysfs-mac-{}", std::process::id()));
    let sys = tmp.join("sys");
    let net = sys.join("class/net");
    std::fs::create_dir_all(net.join("eth-present")).unwrap();
    std::fs::create_dir_all(net.join("wg-absent")).unwrap();
    std::fs::create_dir_all(net.join("weird-missing")).unwrap();
    for (dir, file, content) in [
        (net.join("eth-present"), "ifindex", "2\n"),
        (net.join("eth-present"), "address", "52:54:00:12:34:56\n"),
        (net.join("eth-present"), "operstate", "up\n"),
        (net.join("eth-present"), "type", "1\n"),
        (net.join("wg-absent"), "ifindex", "3\n"),
        (net.join("wg-absent"), "address", "\n"), // empty: observed absence
        (net.join("wg-absent"), "operstate", "unknown\n"),
        (net.join("wg-absent"), "type", "65534\n"),
        (net.join("weird-missing"), "ifindex", "4\n"),
        // no address file at all: not observed
        (net.join("weird-missing"), "operstate", "down\n"),
        (net.join("weird-missing"), "type", "1\n"),
    ] {
        std::fs::write(dir.join(file), content).unwrap();
    }
    // An empty procfs root so the provider run stays valid.
    let proc = tmp.join("proc/net");
    std::fs::create_dir_all(&proc).unwrap();
    std::fs::write(proc.join("route"), "").unwrap();
    std::fs::write(proc.join("if_inet6"), "").unwrap();
    std::fs::write(proc.join("ipv6_route"), "").unwrap();

    let mut p = SysFsProvider::with_roots(&sys, tmp.join("proc"));
    let observation = observe(&mut p);
    let interfaces = observation.interfaces.items();

    let present = interfaces.iter().find(|i| i.name == "eth-present").unwrap();
    assert_eq!(
        present.mac_address,
        MacObservation::Present("52:54:00:12:34:56".into())
    );
    let absent = interfaces.iter().find(|i| i.name == "wg-absent").unwrap();
    assert_eq!(
        absent.mac_address,
        MacObservation::Absent,
        "an empty address file is an observed absence"
    );
    let missing = interfaces
        .iter()
        .find(|i| i.name == "weird-missing")
        .unwrap();
    assert_eq!(
        missing.mac_address,
        MacObservation::NotObserved,
        "a missing address file means the address was not observed"
    );

    std::fs::remove_dir_all(&tmp).ok();
}
