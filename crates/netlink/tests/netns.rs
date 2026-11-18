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
use araiguma_core::provider::{DiscoveryProvider, ProviderId, ProviderState};
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
    let defaults: Vec<_> = snapshot.routes.iter().filter(|r| r.is_default()).collect();
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
}
