//! Integration tests inside transient unprivileged network namespaces.
//!
//! Each test re-executes the test binary under `unshare -Urn` (new user +
//! network namespace, unprivileged), then runs its assertions inside the
//! isolated namespace where it holds CAP_NET_ADMIN. The namespace and
//! everything created inside it are destroyed when the process exits; the
//! host network namespace is never modified. `ip` (iproute2) is used only
//! for test setup and independent cross-validation — the application code
//! itself never shells out.

use std::process::Command;
use std::time::Duration;

use araiguma_core::model::{
    AddressScope, AdminState, InterfaceKind, ObservationSource, OperState, RouteType,
};
use araiguma_core::provider::{DiscoveryProvider, ProviderId, ProviderState, SectionResult};
use araiguma_netlink::{Monitor, NetlinkProvider, Signal};

/// Re-exec the test binary inside a fresh netns, or return when already
/// inside one.
///
/// Pattern (WO-02 correction §9.5). Returns `true` inside the re-exec'd
/// child (run the real test body); `false` in the parent — where the child
/// has already run inside its own transient namespace and its exit status
/// has been ASSERTED, so the caller must skip the body immediately:
///
///     if !ensure_netns("my_test") { return; }
///
/// This replaces the previous pattern, which called `process::exit` after
/// the first child completed: it orphaned sibling children (their results
/// were never checked — a failing test could be silently swallowed) and
/// prevented the harness from printing a proper summary. The guard keeps
/// the safety property (a body never runs outside a namespace) while every
/// child's result is asserted and accounted.
fn ensure_netns(test: &str) -> bool {
    if std::env::var("ARIGUMA_NETNS_ACTIVE").is_ok() {
        return true;
    }
    let exe = std::env::current_exe().expect("test binary path");
    let status = Command::new("unshare")
        .arg("-Urn")
        .arg(exe)
        .args(["--exact", test, "--nocapture", "--test-threads=1"])
        .env("ARIGUMA_NETNS_ACTIVE", "1")
        .status()
        .expect("unshare must be available for netns integration tests");
    assert!(
        status.success(),
        "netns child for {test} failed with {status}; its output is above"
    );
    false
}

/// Run `ip` inside the test namespace; panic with stderr on failure.
fn ip(args: &[&str]) {
    let output = Command::new("ip")
        .args(args)
        .output()
        .expect("iproute2 must be available");
    assert!(
        output.status.success(),
        "ip {args:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

async fn wait_for_kind(monitor: &mut Monitor, wanted: &str) {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
    while tokio::time::Instant::now() < deadline {
        let signal = tokio::time::timeout_at(deadline, monitor.next_signal())
            .await
            .unwrap_or_else(|_| panic!("no signal before deadline (waiting for {wanted})"));
        match signal {
            Signal::Changed { kind } if kind == wanted => return,
            other => {
                // Unrelated notifications (e.g. setLink noise) are skipped.
                eprintln!("skipping signal {other:?} while waiting for {wanted}");
            }
        }
    }
    panic!("no {wanted} signal before deadline");
}

#[tokio::test]
async fn netns_dump_observes_dummy_addresses_and_routes() {
    if !ensure_netns("netns_dump_observes_dummy_addresses_and_routes") {
        return; // parent: the child already ran and was asserted
    }

    // Test setup inside the isolated namespace (never the host).
    ip(&["link", "set", "lo", "up"]);
    ip(&["link", "add", "ar-dummy0", "type", "dummy"]);
    ip(&["link", "set", "ar-dummy0", "up"]);
    ip(&["address", "add", "192.0.2.10/24", "dev", "ar-dummy0"]);
    // nodad keeps the IPv6 address immediately usable (no DAD delay).
    ip(&[
        "address",
        "add",
        "fd00:d0:0:1::10/64",
        "dev",
        "ar-dummy0",
        "nodad",
    ]);
    ip(&[
        "route",
        "add",
        "default",
        "via",
        "192.0.2.1",
        "dev",
        "ar-dummy0",
    ]);

    let mut provider = NetlinkProvider::new();
    let observation = provider.observe().await;

    assert_eq!(observation.provider, ProviderId::Netlink);
    assert_eq!(
        observation.state(),
        ProviderState::Available,
        "all sections must succeed inside a netns"
    );

    // Interfaces: netlink classifies dummy via IFLA_INFO_KIND.
    let interfaces = observation.interfaces.items();
    let dummy = interfaces
        .iter()
        .find(|i| i.name == "ar-dummy0")
        .expect("dummy interface observed");
    assert_eq!(dummy.kind, InterfaceKind::Dummy);
    assert_eq!(dummy.kind_detail.as_deref(), Some("dummy"));
    assert_eq!(dummy.admin_state, Some(AdminState::Up));
    // Documented kernel behavior: dummy devices report operstate UNKNOWN
    // even when up.
    assert_eq!(dummy.oper_state, Some(OperState::Unknown));
    assert_eq!(dummy.sources, vec![ObservationSource::Netlink]);

    // Addresses.
    let v4 = observation.addresses_v4.items();
    assert!(
        v4.iter().any(|a| a.address.to_string() == "192.0.2.10"
            && a.prefix_len == 24
            && a.scope == Some(AddressScope::Global)),
        "IPv4 address observed: {v4:?}"
    );
    let v6 = observation.addresses_v6.items();
    assert!(
        v6.iter()
            .any(|a| a.address.to_string() == "fd00:d0:0:1::10" && a.prefix_len == 64),
        "IPv6 address observed: {v6:?}"
    );

    // Routes: default route in the main table.
    let routes = observation.routes_v4.items();
    let default_route = routes
        .iter()
        .find(|r| r.is_default())
        .expect("default route observed");
    assert_eq!(default_route.gateway.unwrap().to_string(), "192.0.2.1");
    assert_eq!(default_route.table, Some(254));
    assert_eq!(default_route.output_interface, Some(dummy.ifindex));

    // Documented kernel behavior: a full route dump includes the local
    // table (255) with the address's local entry.
    let local: Vec<_> = routes.iter().filter(|r| r.table == Some(255)).collect();
    assert!(
        local.iter().any(|r| r.route_type == Some(RouteType::Local)
            && r.destination
                .is_some_and(|d| d.to_string() == "192.0.2.10/32")),
        "local table entry present: {local:?}"
    );

    // Independent cross-validation against iproute2 in the same namespace.
    let output = Command::new("ip")
        .args(["-4", "-j", "route"])
        .output()
        .expect("ip -j route");
    let ip_routes: serde_json::Value = serde_json::from_slice(&output.stdout).expect("json");
    assert!(
        ip_routes
            .as_array()
            .expect("array")
            .iter()
            .any(|r| r["dst"] == "default" && r["gateway"] == "192.0.2.1"),
        "iproute2 agrees about the default route: {ip_routes}"
    );
}

#[tokio::test]
async fn netns_monitor_detects_changes_and_resynchronizes() {
    if !ensure_netns("netns_monitor_detects_changes_and_resynchronizes") {
        return; // parent: the child already ran and was asserted
    }

    let mut monitor = Monitor::start().expect("multicast socket inside netns");

    ip(&["link", "add", "ar-mon0", "type", "dummy"]);
    // Bring the link up BEFORE assigning the address: this kernel does not
    // install the connected route for an address on a DOWN interface, so
    // the default route via 192.0.2.1 below would be rejected with
    // "Nexthop has invalid gateway". (This test's failure was hidden for
    // the entire WO-02 cycle by the old re-exec pattern, which orphaned
    // this child and never checked its exit status — the 52-vs-53 test
    // count reconciliation exposed it.)
    ip(&["link", "set", "ar-mon0", "up"]);
    wait_for_kind(&mut monitor, "newLink").await;

    ip(&["address", "add", "192.0.2.20/24", "dev", "ar-mon0"]);
    wait_for_kind(&mut monitor, "newAddress").await;

    ip(&[
        "route",
        "add",
        "default",
        "via",
        "192.0.2.1",
        "dev",
        "ar-mon0",
    ]);
    wait_for_kind(&mut monitor, "newRoute").await;

    ip(&["link", "del", "ar-mon0"]);
    wait_for_kind(&mut monitor, "delLink").await;

    // Forced resynchronization reports Resynchronized and the subscription
    // keeps working afterwards.
    monitor.force_resync();
    let signal = tokio::time::timeout(Duration::from_secs(5), monitor.next_signal())
        .await
        .expect("resync signal");
    assert_eq!(signal, Signal::Resynchronized);

    ip(&["link", "add", "ar-mon1", "type", "dummy"]);
    wait_for_kind(&mut monitor, "newLink").await;
}

#[tokio::test]
async fn netns_monitor_recovers_from_receive_buffer_overrun() {
    // WO-02 correction §9.5: a PRACTICAL socket failure. The kernel drops
    // netlink notifications (ENOBUFS → NLMSG_OVERRUN) once the socket's
    // receive queue is full. Inside this transient namespace we starve the
    // monitor's reader (the synchronous ip batch below occupies the test's
    // single-threaded runtime, so the connection task cannot drain the
    // socket) while a large batch of link add/remove operations floods the
    // queue far beyond its default ~212 KiB. The host network namespace is
    // never touched.
    if !ensure_netns("netns_monitor_recovers_from_receive_buffer_overrun") {
        return; // parent: the child already ran and was asserted
    }

    let mut monitor = Monitor::start().expect("multicast socket inside netns");

    // ~2000 add/remove pairs generate several times more notifications
    // than the receive buffer can hold.
    let batch = std::env::temp_dir().join(format!("araiguma-flood-{}.batch", std::process::id()));
    let mut lines = String::new();
    for i in 0..2000 {
        lines.push_str(&format!("link add ar-flood-{i} type dummy\n"));
        lines.push_str(&format!("link del ar-flood-{i}\n"));
    }
    std::fs::write(&batch, lines).expect("write batch");
    ip(&["-batch", batch.to_str().expect("utf8 path")]);

    // The overrun must surface as Resynchronized — dropped events are
    // unobserved, and the monitor never claims the stream is current.
    let signal = tokio::time::timeout(Duration::from_secs(10), monitor.next_signal())
        .await
        .expect("a signal after the flood");
    assert_eq!(
        signal,
        Signal::Resynchronized,
        "buffer overrun must resynchronize, not pretend completeness"
    );

    // Drain whatever else the kernel buffered during the flood, then prove
    // the subscription still delivers FRESH events afterwards.
    let drain_deadline = tokio::time::Instant::now() + Duration::from_secs(2);
    while let Ok(_stale) = tokio::time::timeout_at(drain_deadline, monitor.next_signal()).await {
        // Discard buffered notifications; the loop ends at the quiet window.
    }
    ip(&["link", "add", "ar-after-flood", "type", "dummy"]);
    wait_for_kind(&mut monitor, "newLink").await;
}

#[tokio::test]
async fn netns_merge_with_sysfs_fallback() {
    use araiguma_core::merge::merge;
    use araiguma_sysfs::SysFsProvider;

    if !ensure_netns("netns_merge_with_sysfs_fallback") {
        return; // parent: the child already ran and was asserted
    }

    ip(&["link", "add", "ar-both0", "type", "dummy"]);
    ip(&["link", "set", "ar-both0", "up"]);
    ip(&["address", "add", "198.51.100.30/24", "dev", "ar-both0"]);

    let mut netlink = NetlinkProvider::new();
    let mut sysfs = SysFsProvider::new();
    let observations = vec![netlink.observe().await, sysfs.observe().await];
    let snapshot = merge(observations, 7);

    // Netlink classification (Dummy) wins over sysfs's ambiguous
    // Ethernet-with-caveat, and both sources are recorded.
    let dummy = snapshot
        .interfaces
        .iter()
        .find(|i| i.name == "ar-both0")
        .expect("dummy interface in merged snapshot");
    assert_eq!(dummy.kind, InterfaceKind::Dummy);
    assert_eq!(
        dummy.sources,
        vec![ObservationSource::Netlink, ObservationSource::SysFs]
    );

    // The sysfs provider runs degraded (known classification limits), the
    // netlink provider available.
    assert_eq!(snapshot.providers[0].state, ProviderState::Available);
    assert_eq!(snapshot.providers[1].state, ProviderState::Degraded);

    // Same default-route observation from both sources deduplicates.
    ip(&[
        "route",
        "add",
        "default",
        "via",
        "198.51.100.1",
        "dev",
        "ar-both0",
    ]);
    let mut netlink = NetlinkProvider::new();
    let mut sysfs = SysFsProvider::new();
    let observations = vec![netlink.observe().await, sysfs.observe().await];
    let snapshot = merge(observations, 8);
    // v4 defaults only — since WO-03 M4 the snapshot also carries the
    // kernel's IPv6 unreachable-default sentinel from
    // /proc/net/ipv6_route (see below), which is_default() matches too.
    let defaults: Vec<_> = snapshot
        .routes
        .iter()
        .filter(|r| r.is_default() && r.family == araiguma_core::model::AddressFamily::Ipv4)
        .collect();
    assert_eq!(
        defaults.len(),
        1,
        "the same kernel default route must appear once: {defaults:?}"
    );
    assert_eq!(
        defaults[0].sources,
        vec![ObservationSource::Netlink, ObservationSource::ProcFs]
    );
    assert_eq!(defaults[0].gateway.unwrap().to_string(), "198.51.100.1");

    // The IPv6 side (WO-03 M4; multiplicity corrected by WO-03-F / AUD-03):
    // /proc/net/ipv6_route carries the kernel's unreachable-default
    // sentinel (::/0, metric 0xffffffff, reject), which netlink does NOT
    // dump. The file prints it TWICE (one per table) with NO table column,
    // so the copies may be distinct routes in different tables: BOTH are
    // preserved as honest procfs-only observations with an unknown table —
    // never dropped, never fabricated into a table id, never silently
    // deduplicated.
    let v6_defaults: Vec<_> = snapshot
        .routes
        .iter()
        .filter(|r| r.is_default() && r.family == araiguma_core::model::AddressFamily::Ipv6)
        .collect();
    assert_eq!(
        v6_defaults.len(),
        2,
        "both sentinel rows survive (the file cannot prove they are one route): {v6_defaults:?}"
    );
    for sentinel in &v6_defaults {
        assert_eq!(sentinel.metric, Some(u32::MAX));
        assert_eq!(sentinel.table, None);
        assert_eq!(sentinel.sources, vec![ObservationSource::ProcFs]);
        assert!(sentinel
            .flags
            .as_ref()
            .is_some_and(|f| f.contains(&"reject".to_string())));
        assert!(
            sentinel.notes.iter().any(|n| n.contains("appears 2 times")),
            "the preserved multiplicity is explicit: {:?}",
            sentinel.notes
        );
    }
}

#[tokio::test]
async fn netns_veth_pair_observed_with_peer_relationship() {
    // WO-03 M2: a real veth pair inside the transient namespace. The
    // kernel classifies both endpoints as veth and reports each peer's
    // ifindex (via IFLA_LINK on this kernel); the merged snapshot carries
    // the peers, and the topology projects exactly one VethPeer edge.
    if !ensure_netns("netns_veth_pair_observed_with_peer_relationship") {
        return; // parent: the child already ran and was asserted
    }

    ip(&[
        "link", "add", "ar-veth0", "type", "veth", "peer", "name", "ar-veth1",
    ]);

    let mut netlink = NetlinkProvider::new();
    let observation = netlink.observe().await;
    let interfaces: Vec<_> = observation.interfaces.items().to_vec();

    let veth0 = interfaces
        .iter()
        .find(|i| i.name == "ar-veth0")
        .expect("veth0 observed")
        .clone();
    let veth1 = interfaces
        .iter()
        .find(|i| i.name == "ar-veth1")
        .expect("veth1 observed")
        .clone();
    assert_eq!(veth0.kind, InterfaceKind::Veth);
    assert_eq!(veth1.kind, InterfaceKind::Veth);
    assert_eq!(veth0.peer_ifindex, Some(veth1.ifindex));
    assert_eq!(veth1.peer_ifindex, Some(veth0.ifindex));
    // A veth's link is its peer — never a parent/underlay relationship.
    assert_eq!(veth0.parent_ifindex, None);
    assert_eq!(veth1.parent_ifindex, None);

    // The topology projection: exactly one deduplicated VethPeer edge.
    let mut sysfs = araiguma_sysfs::SysFsProvider::new();
    let snapshot = araiguma_core::merge::merge(vec![observation, sysfs.observe().await], 42);
    let doc = araiguma_core::build_topology(&snapshot);
    let peer_edges: Vec<_> = doc
        .edges
        .iter()
        .filter(|e| e.edge_kind == araiguma_core::topology::EdgeKind::VethPeer)
        .collect();
    assert_eq!(
        peer_edges.len(),
        1,
        "one deduplicated veth peer edge: {peer_edges:?}"
    );
    let (from, to) = (peer_edges[0].from, peer_edges[0].to);
    let idx = |id| match id {
        araiguma_core::topology::NodeId::Interface { ifindex } => ifindex,
        _ => unreachable!("veth peers are interface nodes"),
    };
    assert!(
        (idx(from), idx(to)) == (veth0.ifindex, veth1.ifindex)
            || (idx(from), idx(to)) == (veth1.ifindex, veth0.ifindex)
    );

    // Cross-validation: iproute2 agrees these are peers.
    let output = Command::new("ip")
        .args(["-d", "link", "show", "ar-veth0"])
        .output()
        .expect("ip -d link");
    let text = String::from_utf8_lossy(&output.stdout);
    assert!(
        text.contains("veth"),
        "iproute2 reports the veth kind: {text}"
    );
}

#[tokio::test]
async fn netns_multipath_route_observed_with_all_hops() {
    // WO-03 M3: a real multipath route inside the transient namespace —
    // every hop with gateway, output interface, and weight; procfs's
    // first-hop view deduplicates against it; nothing claims a
    // first-hop-only representation anymore.
    if !ensure_netns("netns_multipath_route_observed_with_all_hops") {
        return; // parent: the child already ran and was asserted
    }

    ip(&["link", "set", "lo", "up"]);
    ip(&["link", "add", "ar-mp0", "type", "dummy"]);
    ip(&["link", "set", "ar-mp0", "up"]);
    ip(&["address", "add", "10.0.0.5/24", "dev", "ar-mp0"]);
    ip(&["link", "add", "ar-mp1", "type", "dummy"]);
    ip(&["link", "set", "ar-mp1", "up"]);
    ip(&["address", "add", "10.1.0.5/24", "dev", "ar-mp1"]);
    ip(&[
        "route",
        "add",
        "10.9.0.0/24",
        "nexthop",
        "via",
        "10.0.0.1",
        "dev",
        "ar-mp0",
        "weight",
        "2",
        "nexthop",
        "via",
        "10.1.0.1",
        "dev",
        "ar-mp1",
        "weight",
        "3",
    ]);

    let mut netlink = NetlinkProvider::new();
    let netlink_observation = netlink.observe().await;
    let mut sysfs = araiguma_sysfs::SysFsProvider::new();
    let sysfs_observation = sysfs.observe().await;
    let snapshot = araiguma_core::merge::merge(vec![netlink_observation, sysfs_observation], 9);

    // Exactly one route for the prefix — procfs's first-hop view merged in.
    let routes: Vec<_> = snapshot
        .routes
        .iter()
        .filter(|r| {
            r.destination
                .as_ref()
                .is_some_and(|d| d.to_string() == "10.9.0.0/24")
        })
        .collect();
    assert_eq!(routes.len(), 1, "one kernel route: {routes:?}");
    let route = routes[0];
    assert_eq!(route.gateway, None, "hops carry the gateways");
    assert!(route.sources.contains(&ObservationSource::ProcFs));

    use araiguma_core::model::{NextHop, NextHops};
    let NextHops::Multi(hops) = &route.next_hops else {
        panic!("expected the full hop set, got {:?}", route.next_hops);
    };
    assert_eq!(hops.len(), 2, "no hop silently discarded: {hops:?}");
    let sorted: Vec<&NextHop> = {
        let mut copy: Vec<&NextHop> = hops.iter().collect();
        copy.sort_by_key(|hop| hop.output_interface);
        copy
    };
    assert_eq!(sorted[0].gateway.unwrap().to_string(), "10.0.0.1");
    assert_eq!(sorted[0].weight, Some(2), "weight 2 = kernel rtnh_hops 1");
    assert_eq!(sorted[0].output_interface_name.as_deref(), Some("ar-mp0"));
    assert_eq!(sorted[1].gateway.unwrap().to_string(), "10.1.0.1");
    assert_eq!(sorted[1].weight, Some(3), "weight 3 = kernel rtnh_hops 2");
    assert_eq!(sorted[1].output_interface_name.as_deref(), Some("ar-mp1"));
    assert!(
        !route.notes.iter().any(|n| n.contains("only the first")),
        "no first-hop-only claim remains"
    );

    // Cross-validation: iproute2 agrees about both next hops.
    let output = Command::new("ip")
        .args(["route", "show", "10.9.0.0/24"])
        .output()
        .expect("ip route show");
    let text = String::from_utf8_lossy(&output.stdout);
    assert!(
        text.contains("nexthop via 10.0.0.1 dev ar-mp0 weight 2"),
        "{text}"
    );
    assert!(
        text.contains("nexthop via 10.1.0.1 dev ar-mp1 weight 3"),
        "{text}"
    );
}

#[tokio::test]
async fn netns_ipv6_procfs_routes_merge_and_fallback() {
    // WO-03 M4: the real /proc/net/ipv6_route inside the transient
    // namespace. The same kernel route observed by netlink (table known)
    // and procfs (no table column) must appear ONCE with united provenance;
    // a procfs-only fallback keeps the table honestly unknown; a missing
    // file is a failed section, never "no IPv6 routes".
    if !ensure_netns("netns_ipv6_procfs_routes_merge_and_fallback") {
        return; // parent: the child already ran and was asserted
    }

    ip(&["link", "set", "lo", "up"]);
    ip(&["link", "add", "ar-v6", "type", "dummy"]);
    ip(&["link", "set", "ar-v6", "up"]);
    ip(&["address", "add", "2001:db8::1/64", "dev", "ar-v6", "nodad"]);
    ip(&[
        "route",
        "add",
        "2001:db8:9::/64",
        "via",
        "2001:db8::2",
        "dev",
        "ar-v6",
    ]);

    // The real file, through the real provider.
    let mut sysfs = araiguma_sysfs::SysFsProvider::new();
    let sysfs_observation = sysfs.observe().await;
    let proc_routes = sysfs_observation
        .routes_v6
        .items()
        .iter()
        .filter(|r| r.destination.is_some())
        .cloned()
        .collect::<Vec<_>>();
    assert!(
        matches!(sysfs_observation.routes_v6, SectionResult::Observed(_)),
        "the section is observed, not unsupported"
    );
    assert!(!proc_routes.is_empty(), "a live namespace has IPv6 routes");

    // The gateway route from the FILE alone: gateway parsed, metric parsed
    // (hex), table not observed, procfs provenance.
    let file_gateway = proc_routes
        .iter()
        .find(|r| r.destination.as_ref().unwrap().to_string() == "2001:db8:9::/64")
        .expect("the route we added is in /proc/net/ipv6_route");
    assert_eq!(
        file_gateway.gateway.unwrap().to_string(),
        "2001:db8::2",
        "nexthop column parses big-endian hex"
    );
    assert_eq!(file_gateway.table, None, "no table column in the file");
    assert_eq!(
        file_gateway.metric,
        Some(1024),
        "metric 0x400 = IP6_RT_PRIO_USER, matching the netlink RTA_PRIORITY"
    );
    assert_eq!(file_gateway.output_interface_name.as_deref(), Some("ar-v6"));
    assert!(file_gateway
        .flags
        .as_ref()
        .is_some_and(|f| f.contains(&"gateway".to_string())));

    // Full merge: netlink (priority 0) + sysfs (priority 1).
    let mut netlink = NetlinkProvider::new();
    let netlink_observation = netlink.observe().await;
    let snapshot = araiguma_core::merge::merge(vec![netlink_observation, sysfs_observation], 7);

    // The gateway route appears exactly once with united provenance and
    // netlink's richer fields standing.
    let gateway_routes: Vec<_> = snapshot
        .routes
        .iter()
        .filter(|r| {
            r.destination
                .as_ref()
                .is_some_and(|d| d.to_string() == "2001:db8:9::/64")
        })
        .collect();
    assert_eq!(
        gateway_routes.len(),
        1,
        "one kernel route: {gateway_routes:?}"
    );
    let gateway = gateway_routes[0];
    assert_eq!(gateway.gateway.unwrap().to_string(), "2001:db8::2");
    assert!(
        gateway.sources.contains(&ObservationSource::Netlink)
            && gateway.sources.contains(&ObservationSource::ProcFs),
        "both providers observed it: {:?}",
        gateway.sources
    );
    assert_eq!(
        gateway.protocol,
        Some(araiguma_core::model::RouteProtocol::Boot),
        "iproute2 adds routes as RTPROT_BOOT; netlink's protocol stands over procfs's not-observed"
    );

    // The local-table ::1/128 (the wildcard case): netlink knows table 255,
    // the file does not — still exactly one route, table attributed by the
    // observation that could see it.
    let loopback_routes: Vec<_> = snapshot
        .routes
        .iter()
        .filter(|r| {
            r.destination
                .as_ref()
                .is_some_and(|d| d.to_string() == "::1/128")
        })
        .collect();
    assert_eq!(
        loopback_routes.len(),
        1,
        "no duplicate ::1/128: {loopback_routes:?}"
    );
    assert_eq!(loopback_routes[0].table, Some(255));
    assert!(loopback_routes[0]
        .sources
        .contains(&ObservationSource::ProcFs));

    // Cross-validation: iproute2 agrees about the gateway route.
    let output = Command::new("ip")
        .args(["-6", "route", "show", "2001:db8:9::/64"])
        .output()
        .expect("ip -6 route show");
    let text = String::from_utf8_lossy(&output.stdout);
    assert!(
        text.contains("via 2001:db8::2 dev ar-v6"),
        "iproute2 agrees: {text}"
    );
    assert!(
        text.contains("metric 1024"),
        "iproute2 confirms the metric observation: {text}"
    );
    // (iproute2 omits "proto boot" from the display — boot is the default
    // display protocol — so the protocol cross-check stays at the model
    // level above.)

    // The fallback semantics: against an INJECTED root without the file,
    // the section is Failed — a missing file is not evidence of zero
    // routes (the transient namespace itself is never modified).
    let mut broken =
        araiguma_sysfs::SysFsProvider::with_roots("/sys", "/nonexistent-araiguma-proc");
    let broken_observation = broken.observe().await;
    assert!(
        matches!(broken_observation.routes_v6, SectionResult::Failed(_)),
        "a missing /proc/net/ipv6_route fails the section instead of claiming zero routes"
    );
}

#[tokio::test]
async fn netns_combined_observation_integrates_all_wo03_features() {
    // WO-03 integration: one namespace, one observation run, every
    // milestone together — a veth pair (M2), a multipath route (M3), and
    // IPv6 routes from /proc/net/ipv6_route (M4) — through both providers,
    // the merge, and the topology projection. The point is that these are
    // one consistent application, not isolated demos.
    if !ensure_netns("netns_combined_observation_integrates_all_wo03_features") {
        return; // parent: the child already ran and was asserted
    }

    ip(&["link", "set", "lo", "up"]);
    // A veth pair (M2) and a dummy (for the multipath route's second hop).
    ip(&[
        "link",
        "add",
        "ar-c-veth0",
        "type",
        "veth",
        "peer",
        "name",
        "ar-c-veth1",
    ]);
    ip(&["link", "set", "ar-c-veth0", "up"]);
    ip(&["link", "set", "ar-c-veth1", "up"]);
    ip(&[
        "address",
        "add",
        "10.20.0.1/24",
        "dev",
        "ar-c-veth0",
        "nodad",
    ]);
    ip(&[
        "address",
        "add",
        "10.21.0.1/24",
        "dev",
        "ar-c-veth1",
        "nodad",
    ]);
    ip(&["link", "add", "ar-c-dummy0", "type", "dummy"]);
    ip(&["link", "set", "ar-c-dummy0", "up"]);
    ip(&["address", "add", "10.22.0.1/24", "dev", "ar-c-dummy0"]);
    // A multipath route across the veth and the dummy (M3).
    ip(&[
        "route",
        "add",
        "10.99.0.0/24",
        "nexthop",
        "via",
        "10.20.0.2",
        "dev",
        "ar-c-veth0",
        "weight",
        "2",
        "nexthop",
        "via",
        "10.22.0.2",
        "dev",
        "ar-c-dummy0",
        "weight",
        "5",
    ]);
    // An IPv6 gateway route (M4).
    ip(&[
        "address",
        "add",
        "2001:db8:20::1/64",
        "dev",
        "ar-c-veth0",
        "nodad",
    ]);
    ip(&[
        "route",
        "add",
        "2001:db8:99::/64",
        "via",
        "2001:db8:20::2",
        "dev",
        "ar-c-veth0",
    ]);

    let mut netlink = NetlinkProvider::new();
    let netlink_observation = netlink.observe().await;
    let mut sysfs = araiguma_sysfs::SysFsProvider::new();
    let sysfs_observation = sysfs.observe().await;
    let snapshot = araiguma_core::merge::merge(vec![netlink_observation, sysfs_observation], 11);

    // M2: the veth pair carries mutual peers and exactly one deduplicated
    // topology edge (checked in depth by its own test; here the pair must
    // simply survive the combined observation).
    let veth0 = snapshot
        .interfaces
        .iter()
        .find(|i| i.name == "ar-c-veth0")
        .expect("veth end observed");
    let veth1 = snapshot
        .interfaces
        .iter()
        .find(|i| i.name == "ar-c-veth1")
        .expect("veth end observed");
    assert_eq!(veth0.kind, InterfaceKind::Veth);
    assert!(veth0.peer_ifindex.is_some() && veth1.peer_ifindex.is_some());

    // M3: the multipath route with its FULL hop set; procfs's first-hop
    // view merged in rather than duplicating.
    let multipath: Vec<_> = snapshot
        .routes
        .iter()
        .filter(|r| {
            r.destination
                .as_ref()
                .is_some_and(|d| d.to_string() == "10.99.0.0/24")
        })
        .collect();
    assert_eq!(multipath.len(), 1, "one kernel route: {multipath:?}");
    let route = multipath[0];
    assert!(route.sources.contains(&ObservationSource::ProcFs));
    use araiguma_core::model::NextHops;
    let NextHops::Multi(hops) = &route.next_hops else {
        panic!("full hop set expected, got {:?}", route.next_hops);
    };
    assert_eq!(hops.len(), 2);
    let weights: std::collections::BTreeSet<_> = hops.iter().map(|h| h.weight).collect();
    assert!(weights.contains(&Some(2)) && weights.contains(&Some(5)));

    // M4: the IPv6 gateway route appears once with united provenance and
    // netlink's table attribution; the v6 local-table routes deduplicated
    // by the table-less wildcard.
    let v6_gateway: Vec<_> = snapshot
        .routes
        .iter()
        .filter(|r| {
            r.destination
                .as_ref()
                .is_some_and(|d| d.to_string() == "2001:db8:99::/64")
        })
        .collect();
    assert_eq!(v6_gateway.len(), 1, "one kernel v6 route: {v6_gateway:?}");
    assert_eq!(v6_gateway[0].gateway.unwrap().to_string(), "2001:db8:20::2");
    assert!(
        v6_gateway[0].sources.contains(&ObservationSource::Netlink)
            && v6_gateway[0].sources.contains(&ObservationSource::ProcFs)
    );
    let loopback: Vec<_> = snapshot
        .routes
        .iter()
        .filter(|r| {
            r.destination
                .as_ref()
                .is_some_and(|d| d.to_string() == "::1/128")
        })
        .collect();
    assert_eq!(
        loopback.len(),
        1,
        "no ::1/128 duplicate in the combined run"
    );

    // The topology projection integrates everything: the veth peer edge,
    // gateway nodes for both families, and a schema-v3 document.
    let topology = araiguma_core::topology::build_topology(&snapshot);
    assert_eq!(topology.schema_version, 3);
    let peer_edges: Vec<_> = topology
        .edges
        .iter()
        .filter(|e| e.edge_kind == araiguma_core::topology::EdgeKind::VethPeer)
        .collect();
    assert_eq!(peer_edges.len(), 1, "one deduplicated veth edge");
    let v6_gw_node = topology
        .nodes
        .iter()
        .any(|n| matches!(n.id, araiguma_core::topology::NodeId::Gateway { address } if address.to_string() == "2001:db8:20::2"));
    assert!(v6_gw_node, "the v6 gateway is a node");
    let v4_gw_node = topology
        .nodes
        .iter()
        .any(|n| matches!(n.id, araiguma_core::topology::NodeId::Gateway { address } if address.to_string() == "10.20.0.2"));
    assert!(v4_gw_node, "a multipath hop's gateway is a node");

    // Cross-validation: iproute2 agrees about all three.
    let show = |args: &[&str]| {
        let output = Command::new("ip").args(args).output().expect("ip");
        String::from_utf8_lossy(&output.stdout).into_owned()
    };
    let mp = show(&["route", "show", "10.99.0.0/24"]);
    assert!(
        mp.contains("nexthop via 10.20.0.2 dev ar-c-veth0 weight 2"),
        "{mp}"
    );
    assert!(
        mp.contains("nexthop via 10.22.0.2 dev ar-c-dummy0 weight 5"),
        "{mp}"
    );
    let v6 = show(&["-6", "route", "show", "2001:db8:99::/64"]);
    assert!(v6.contains("via 2001:db8:20::2 dev ar-c-veth0"), "{v6}");
    let peers = show(&["-d", "link", "show", "ar-c-veth0"]);
    assert!(peers.contains("veth"), "{peers}");
}

/// Run `ip` (or another command) and panic with stderr on failure.
fn run_ok(program: &str, args: &[&str]) {
    let output = Command::new(program).args(args).output().expect(program);
    assert!(
        output.status.success(),
        "{program} {args:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

/// Spawn a child process in a NEW network namespace (owned by this user
/// namespace, so we keep CAP_NET_ADMIN over it) and hold it open. Returns
/// the child's pid; the child dies with the test process, so the namespace
/// and everything in it are cleaned up with the transient test namespace.
fn spawn_child_netns() -> std::process::Child {
    let child = Command::new("unshare")
        .arg("-n")
        .arg("sleep")
        .arg("300")
        .spawn()
        .expect("spawn child netns");
    // Wait until the child's netns link exists (it appears at exec time).
    let netns = format!("/proc/{}/ns/net", child.id());
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    while !std::path::Path::new(&netns).exists() {
        assert!(
            std::time::Instant::now() < deadline,
            "child netns never appeared"
        );
        std::thread::sleep(Duration::from_millis(20));
    }
    child
}

#[tokio::test]
async fn netns_cross_namespace_veth_peer_never_draws_a_local_edge() {
    // WO-03-F / AUD-01 — the audit's exact adversarial scenario, live: a
    // real veth peer lives in ANOTHER network namespace while an unrelated
    // LOCAL interface carries the same ifindex. The kernel marks the
    // cross-namespace peer with IFLA_LINK_NETNSID; the observation must
    // keep the peer unresolved and NEVER connect the veth to the colliding
    // local interface.
    if !ensure_netns("netns_cross_namespace_veth_peer_never_draws_a_local_edge") {
        return; // parent: the child already ran and was asserted
    }

    // Occupy local ifindexes 2 and 3 with dummies (lo is 1): `ar-coll`
    // (ifindex 2) will collide with the moved peer's namespace-scoped
    // ifindex below.
    ip(&["link", "add", "ar-coll", "type", "dummy"]);
    ip(&["link", "add", "ar-d2", "type", "dummy"]);

    // A veth pair inside a child network namespace. On this kernel the
    // pair lands on child ifindexes 2 and 3; `ar-y0` (child ifindex 3,
    // whose peer `ar-y1` holds child ifindex 2) is then moved into THIS
    // namespace, where ifindex 2 is taken by the unrelated dummy — the
    // kernel re-allocates a fresh local ifindex and reports the peer's
    // namespace-scoped ifindex (2) plus IFLA_LINK_NETNSID.
    let mut child = spawn_child_netns();
    let child_netns = format!("/proc/{}/ns/net", child.id());
    run_ok(
        "nsenter",
        &[
            &format!("--net={child_netns}"),
            "ip",
            "link",
            "add",
            "ar-y0",
            "type",
            "veth",
            "peer",
            "name",
            "ar-y1",
        ],
    );
    let our_pid = std::process::id().to_string();
    run_ok(
        "nsenter",
        &[
            &format!("--net={child_netns}"),
            "ip",
            "link",
            "set",
            "ar-y0",
            "netns",
            &our_pid,
        ],
    );

    let mut netlink = NetlinkProvider::new();
    let observation = netlink.observe().await;
    let mut sysfs = araiguma_sysfs::SysFsProvider::new();
    let snapshot = araiguma_core::merge::merge(vec![observation, sysfs.observe().await], 21);

    let veth = snapshot
        .interfaces
        .iter()
        .find(|i| i.name == "ar-y0")
        .expect("the moved veth end is observed locally")
        .clone();
    let colliding = snapshot
        .interfaces
        .iter()
        .find(|i| i.name == "ar-coll")
        .expect("the colliding dummy is observed")
        .clone();

    assert_eq!(veth.kind, InterfaceKind::Veth);
    assert!(
        veth.link_netnsid.is_some(),
        "the kernel reports the peer's namespace (IFLA_LINK_NETNSID): {:?}",
        veth.link_netnsid
    );
    assert_eq!(
        veth.peer_ifindex,
        Some(colliding.ifindex),
        "the adversarial setup: the cross-namespace peer's ifindex equals the LOCAL dummy's ifindex"
    );

    // The projection must not draw any vethPeer edge — the colliding dummy
    // is not the peer — and the annotation states the namespace fact.
    let doc = araiguma_core::build_topology(&snapshot);
    assert!(
        !doc.edges
            .iter()
            .any(|e| e.edge_kind == araiguma_core::topology::EdgeKind::VethPeer),
        "no veth edge may exist: the only local ifindex match is the unrelated dummy"
    );
    let node = doc
        .nodes
        .iter()
        .find(|n| n.label == "ar-y0")
        .expect("the veth node exists");
    assert!(node
        .annotations
        .iter()
        .any(|a| a.contains("lives in another network namespace")));

    // Cleanup: kill the child; its namespace (and ar-y1 inside it) dies
    // with it. The transient test namespace disappears on process exit.
    let _ = child.kill();
    let _ = child.wait();
}

#[tokio::test]
async fn netns_cross_namespace_peer_without_collision_stays_unresolved() {
    // The non-adversarial cross-namespace case: the peer's ifindex matches
    // no local interface at all. The peer observation (ifindex + netnsid)
    // is preserved, and no edge is invented.
    if !ensure_netns("netns_cross_namespace_peer_without_collision_stays_unresolved") {
        return; // parent: the child already ran and was asserted
    }

    let mut child = spawn_child_netns();
    ip(&[
        "link", "add", "ar-x0", "type", "veth", "peer", "name", "ar-x1",
    ]);
    // Move one end into the child namespace (its ifindex there is
    // namespace-scoped; the kernel reports IFLA_LINK_NETNSID on ar-x0).
    let child_pid = child.id().to_string();
    ip(&["link", "set", "ar-x1", "netns", &child_pid]);

    let mut netlink = NetlinkProvider::new();
    let observation = netlink.observe().await;
    let mut sysfs = araiguma_sysfs::SysFsProvider::new();
    let snapshot = araiguma_core::merge::merge(vec![observation, sysfs.observe().await], 22);

    let veth = snapshot
        .interfaces
        .iter()
        .find(|i| i.name == "ar-x0")
        .expect("the local veth end is observed")
        .clone();
    assert_eq!(veth.kind, InterfaceKind::Veth);
    assert!(veth.peer_ifindex.is_some(), "the peer ifindex is observed");
    assert!(
        veth.link_netnsid.is_some(),
        "the peer's namespace is reported"
    );
    // The peer itself is NOT part of this observation.
    assert!(snapshot.interfaces.iter().all(|i| i.name != "ar-x1"));

    let doc = araiguma_core::build_topology(&snapshot);
    assert!(
        !doc.edges
            .iter()
            .any(|e| e.edge_kind == araiguma_core::topology::EdgeKind::VethPeer),
        "a peer in another namespace never becomes a local edge"
    );
    let node = doc
        .nodes
        .iter()
        .find(|n| n.label == "ar-x0")
        .expect("the veth node exists");
    assert!(node
        .annotations
        .iter()
        .any(|a| a.contains("lives in another network namespace")));

    let _ = child.kill();
    let _ = child.wait();
}

#[tokio::test]
async fn netns_appended_same_key_ecmp_routes_all_survive_the_merge() {
    // WO-03-F / AUD-02 live reproduction: `ip route append` holds TWO
    // distinct kernel routes with the same destination, table, and metric
    // whose next-hop sets OVERLAP on the first hop. Before the fix the
    // merge collapsed them and silently lost the second route's unique
    // hop (reproduced with the real binary during the audit and again in
    // WO-03-F remediation). Both routes, and both of /proc/net/route's
    // rows for them, must survive.
    if !ensure_netns("netns_appended_same_key_ecmp_routes_all_survive_the_merge") {
        return; // parent: the child already ran and was asserted
    }

    ip(&["link", "set", "lo", "up"]);
    for (name, addr) in [
        ("ar-ec0", "10.0.0.5/24"),
        ("ar-ec1", "10.1.0.5/24"),
        ("ar-ec2", "10.2.0.5/24"),
    ] {
        ip(&["link", "add", name, "type", "dummy"]);
        ip(&["link", "set", name, "up"]);
        ip(&["address", "add", addr, "dev", name]);
    }
    ip(&[
        "route",
        "add",
        "10.9.0.0/24",
        "nexthop",
        "via",
        "10.0.0.1",
        "dev",
        "ar-ec0",
        "weight",
        "2",
        "nexthop",
        "via",
        "10.1.0.1",
        "dev",
        "ar-ec1",
        "weight",
        "2",
    ]);
    ip(&[
        "route",
        "append",
        "10.9.0.0/24",
        "nexthop",
        "via",
        "10.0.0.1",
        "dev",
        "ar-ec0",
        "weight",
        "3",
        "nexthop",
        "via",
        "10.2.0.1",
        "dev",
        "ar-ec2",
        "weight",
        "3",
    ]);

    // The kernel genuinely holds two routes (independent cross-check).
    let output = Command::new("ip")
        .args(["-j", "route", "show", "10.9.0.0/24"])
        .output()
        .expect("ip -j route");
    let kernel_routes: serde_json::Value = serde_json::from_slice(&output.stdout).expect("json");
    assert_eq!(
        kernel_routes.as_array().map(Vec::len),
        Some(2),
        "the kernel holds two appended routes: {kernel_routes}"
    );

    let mut netlink = NetlinkProvider::new();
    let netlink_observation = netlink.observe().await;
    let mut sysfs = araiguma_sysfs::SysFsProvider::new();
    let sysfs_observation = sysfs.observe().await;
    let snapshot = araiguma_core::merge::merge(vec![netlink_observation, sysfs_observation], 23);

    let routes: Vec<_> = snapshot
        .routes
        .iter()
        .filter(|r| {
            r.destination
                .is_some_and(|d| d.to_string() == "10.9.0.0/24")
        })
        .collect();

    // The two netlink routes survive with their complete hop sets —
    // including the OVERLAPPING hop (via 10.0.0.1 on ar-ec0), which both
    // routes legitimately carry.
    let netlink_routes: Vec<_> = routes
        .iter()
        .filter(|r| r.sources.first() == Some(&ObservationSource::Netlink))
        .collect();
    assert_eq!(
        netlink_routes.len(),
        2,
        "the appended pair never collapses: {netlink_routes:#?}"
    );
    let name_of = |ifindex: u32| -> String {
        snapshot
            .interfaces
            .iter()
            .find(|i| i.ifindex == ifindex)
            .map(|i| i.name.clone())
            .unwrap_or_else(|| panic!("interface {ifindex} observed"))
    };
    let mut all_hops: Vec<String> = netlink_routes
        .iter()
        .flat_map(|r| match &r.next_hops {
            araiguma_core::model::NextHops::Multi(hops) => hops
                .iter()
                .map(|h| {
                    format!(
                        "{} on {}",
                        h.gateway.unwrap(),
                        name_of(h.output_interface.unwrap())
                    )
                })
                .collect::<Vec<_>>(),
            _ => vec![],
        })
        .collect();
    all_hops.sort();
    assert_eq!(
        all_hops,
        vec![
            "10.0.0.1 on ar-ec0".to_string(),
            "10.0.0.1 on ar-ec0".to_string(), // the overlap, kept on both sides
            "10.1.0.1 on ar-ec1".to_string(),
            "10.2.0.1 on ar-ec2".to_string(),
        ],
        "every observed next hop of both routes survives: {all_hops:?}"
    );

    // /proc/net/route prints one row per kernel route — two rows, both
    // first-hop 10.0.0.1 on ar-ec0, both ambiguous between the two routes.
    // Neither row is attributed to either route (AUD-05); both rows
    // survive as independent observations with the ambiguity note.
    let procfs_rows: Vec<_> = routes
        .iter()
        .filter(|r| r.sources.first() == Some(&ObservationSource::ProcFs))
        .collect();
    assert_eq!(
        procfs_rows.len(),
        2,
        "both file rows survive: {procfs_rows:#?}"
    );
    for row in &procfs_rows {
        assert!(row
            .notes
            .iter()
            .any(|n| n.contains("matches 2 netlink-observed routes")));
    }
    for netlink_route in &netlink_routes {
        assert_eq!(
            netlink_route.sources,
            vec![ObservationSource::Netlink],
            "no arbitrary attribution to either appended route"
        );
    }
}
