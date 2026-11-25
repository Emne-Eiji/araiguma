//! Wire-level integration tests against a private `dbus-daemon`.
//!
//! These tests exercise the foundation exactly as a provider would: through
//! `DBUS_SYSTEM_BUS_ADDRESS`, with raw mock services, and with an independent
//! monitor that records the actual messages and flags on the wire.

mod common;

use std::time::{Duration, Instant};

use araiguma_provider_dbus::policy::{
    NETWORK_MANAGER_ACTIVE_CONNECTION_INTERFACE, NETWORK_MANAGER_DEVICE_INTERFACE,
    NETWORK_MANAGER_DEVICE_PATH_PREFIX, NETWORK_MANAGER_MANAGER_INTERFACE,
    NETWORK_MANAGER_MANAGER_PATH, SYSTEMD_RESOLVED_MANAGER_INTERFACE,
    SYSTEMD_RESOLVED_MANAGER_PATH,
};
use araiguma_provider_dbus::{
    DbusError, DbusFoundation, PropertyRequest, Service, SignalEvent, SignalKind,
};
use common::{
    find_add_match_rule, find_get, nm_active_connection_path, nm_device_path, no_auto_start,
    object_paths, resolved_link_path, wait_for_service_absence, Harness, MockReplies, MockReply,
    MockService,
};
use zbus::OwnedObjectPath;

const WAIT: Duration = Duration::from_secs(3);

fn nm_request<'a>(path: &'a str, interface: &'a str, property: &'a str) -> PropertyRequest<'a> {
    PropertyRequest::new(Service::NetworkManager, path, interface, property)
}

fn resolved_request<'a>(
    path: &'a str,
    interface: &'a str,
    property: &'a str,
) -> PropertyRequest<'a> {
    PropertyRequest::new(Service::SystemdResolved, path, interface, property)
}

fn nm_replies() -> MockReplies {
    let device = nm_device_path(2);
    let active = nm_active_connection_path(1);
    let mut replies = MockReplies::new();
    replies.insert(
        (NETWORK_MANAGER_MANAGER_INTERFACE, "ActiveConnections"),
        MockReply::Value(object_paths(std::slice::from_ref(&active))),
    );
    replies.insert(
        (NETWORK_MANAGER_MANAGER_INTERFACE, "Devices"),
        MockReply::Value(object_paths(std::slice::from_ref(&device))),
    );
    replies.insert(
        (NETWORK_MANAGER_DEVICE_INTERFACE, "Interface"),
        MockReply::Value(zbus::Variant::from("eth0")),
    );
    replies.insert(
        (NETWORK_MANAGER_DEVICE_INTERFACE, "IpInterface"),
        MockReply::Value(zbus::Variant::from("eth0")),
    );
    replies.insert(
        (NETWORK_MANAGER_ACTIVE_CONNECTION_INTERFACE, "Devices"),
        MockReply::Value(object_paths(std::slice::from_ref(&device))),
    );
    replies.insert(
        (NETWORK_MANAGER_ACTIVE_CONNECTION_INTERFACE, "State"),
        MockReply::Value(zbus::Variant::from(2u32)),
    );
    replies.insert(
        (NETWORK_MANAGER_ACTIVE_CONNECTION_INTERFACE, "Type"),
        MockReply::Value(zbus::Variant::from("802-3-ethernet")),
    );
    replies.insert(
        (NETWORK_MANAGER_ACTIVE_CONNECTION_INTERFACE, "Vpn"),
        MockReply::Value(zbus::Variant::from(false)),
    );
    replies
}

#[tokio::test]
async fn preflight_and_approved_reads_have_exact_wire_shape() {
    let harness = Harness::start();
    let monitor = harness.monitor().await;
    let mock = MockService::start(Service::NetworkManager, nm_replies()).await;
    monitor.clear();

    let foundation = DbusFoundation::connect(Service::NetworkManager)
        .await
        .expect("preflight must succeed");

    assert!(
        monitor
            .wait_for(WAIT, |message| message.is_bus_driver_call()
                && message.member_is("GetNameOwner"))
            .await,
        "preflight did not reach GetNameOwner"
    );
    assert!(
        monitor.application_calls().is_empty(),
        "preflight must not send an application message"
    );

    let driver_calls = monitor.bus_driver_calls();
    let client_name = driver_calls
        .iter()
        .find(|message| message.member_is("NameHasOwner"))
        .and_then(|message| message.sender.clone())
        .expect("foundation unique name");
    let client_driver_members: Vec<&str> = driver_calls
        .iter()
        .filter(|message| message.sender.as_deref() == Some(client_name.as_str()))
        .filter_map(|message| message.member.as_deref())
        .collect();
    assert_eq!(
        client_driver_members,
        vec!["Hello", "NameHasOwner", "GetNameOwner"],
        "setup must be exactly Hello, NameHasOwner, GetNameOwner"
    );
    for message in &driver_calls {
        if message.member_is("NameHasOwner") || message.member_is("GetNameOwner") {
            assert_eq!(message.flags & no_auto_start(), no_auto_start());
            assert_eq!(
                message.body.as_deref(),
                Some(&["org.freedesktop.NetworkManager".to_owned()][..])
            );
        }
    }

    let manager_path = NETWORK_MANAGER_MANAGER_PATH;
    let device = nm_device_path(2);
    let active = nm_active_connection_path(1);

    let active_connections = foundation
        .read_property(nm_request(
            manager_path,
            NETWORK_MANAGER_MANAGER_INTERFACE,
            "ActiveConnections",
        ))
        .await
        .expect("manager ActiveConnections");
    assert_eq!(active_connections.value_signature().to_string(), "ao");
    let paths: Vec<OwnedObjectPath> = active_connections.try_into().expect("decode ao");
    assert_eq!(
        paths.iter().map(|path| path.as_str()).collect::<Vec<_>>(),
        vec![active.as_str()]
    );

    let devices = foundation
        .read_property(nm_request(
            manager_path,
            NETWORK_MANAGER_MANAGER_INTERFACE,
            "Devices",
        ))
        .await
        .expect("manager Devices");
    assert_eq!(devices.value_signature().to_string(), "ao");

    let interface = foundation
        .read_property(nm_request(
            &device,
            NETWORK_MANAGER_DEVICE_INTERFACE,
            "Interface",
        ))
        .await
        .expect("device Interface");
    assert_eq!(String::try_from(interface).expect("decode s"), "eth0");

    let ip_interface = foundation
        .read_property(nm_request(
            &device,
            NETWORK_MANAGER_DEVICE_INTERFACE,
            "IpInterface",
        ))
        .await
        .expect("device IpInterface");
    assert_eq!(String::try_from(ip_interface).expect("decode s"), "eth0");

    let connection_devices = foundation
        .read_property(nm_request(
            &active,
            NETWORK_MANAGER_ACTIVE_CONNECTION_INTERFACE,
            "Devices",
        ))
        .await
        .expect("active-connection Devices");
    assert_eq!(connection_devices.value_signature().to_string(), "ao");

    let state = foundation
        .read_property(nm_request(
            &active,
            NETWORK_MANAGER_ACTIVE_CONNECTION_INTERFACE,
            "State",
        ))
        .await
        .expect("active-connection State");
    assert_eq!(u32::try_from(state).expect("decode u"), 2);

    let connection_type = foundation
        .read_property(nm_request(
            &active,
            NETWORK_MANAGER_ACTIVE_CONNECTION_INTERFACE,
            "Type",
        ))
        .await
        .expect("active-connection Type");
    assert_eq!(
        String::try_from(connection_type).expect("decode s"),
        "802-3-ethernet"
    );

    let vpn = foundation
        .read_property(nm_request(
            &active,
            NETWORK_MANAGER_ACTIVE_CONNECTION_INTERFACE,
            "Vpn",
        ))
        .await
        .expect("active-connection Vpn");
    assert!(!bool::try_from(vpn).expect("decode b"));

    let calls = mock.calls();
    assert_eq!(calls.len(), 8, "exactly eight property reads");
    for call in &calls {
        assert_eq!(call.interface, "org.freedesktop.DBus.Properties");
        assert_eq!(call.member, "Get");
        assert!(
            call.signature_is("ss"),
            "unexpected signature {}",
            call.signature
        );
        assert_eq!(
            call.flags & no_auto_start(),
            no_auto_start(),
            "every application call must set NO_AUTO_START"
        );
    }
    assert_eq!(
        find_get(&calls, "ActiveConnections")
            .body
            .as_deref()
            .map(|body| body[0].as_str()),
        Some(NETWORK_MANAGER_MANAGER_INTERFACE)
    );
    assert_eq!(
        find_get(&calls, "Interface")
            .body
            .as_deref()
            .map(|body| body[1].as_str()),
        Some("Interface")
    );

    assert!(
        monitor
            .wait_for_count(WAIT, 8, |message| message.is_application_call())
            .await,
        "monitor did not observe all eight application calls"
    );
    monitor.assert_clean(8);

    let application_before_shutdown = monitor.application_calls().len();
    foundation.shutdown().await.expect("shutdown");
    assert!(
        monitor
            .wait_for(WAIT, |message| {
                message.kind == "Signal"
                    && message.member_is("NameOwnerChanged")
                    && message.body.as_ref().map(|body| {
                        body.first().map(String::as_str) == Some(client_name.as_str())
                            && body.get(2).map(String::as_str) == Some("")
                    }) == Some(true)
            })
            .await,
        "shutdown did not close the client connection"
    );
    tokio::time::sleep(Duration::from_millis(50)).await;
    assert_eq!(
        monitor.application_calls().len(),
        application_before_shutdown,
        "application traffic continued after shutdown"
    );

    mock.shutdown().await;
}

#[tokio::test]
async fn prohibited_requests_are_rejected_without_wire_traffic() {
    let harness = Harness::start();
    let monitor = harness.monitor().await;
    let mock = MockService::start(Service::NetworkManager, nm_replies()).await;
    let foundation = DbusFoundation::connect(Service::NetworkManager)
        .await
        .expect("preflight");
    mock.clear_calls();
    monitor.clear();

    let manager = NETWORK_MANAGER_MANAGER_PATH;
    let manager_interface = NETWORK_MANAGER_MANAGER_INTERFACE;
    for property in [
        "GetAll",
        "Set",
        "GetManagedObjects",
        "Introspect",
        "GetSettings",
        "GetSecrets",
        "Get",
        "State",
        "Version",
        "WirelessEnabled",
        "AccessPoints",
    ] {
        assert_eq!(
            foundation
                .read_property(nm_request(manager, manager_interface, property))
                .await,
            Err(DbusError::UnsupportedRequest),
            "property {property}"
        );
    }

    let device = nm_device_path(2);
    for request in [
        nm_request(&device, manager_interface, "Interface"),
        nm_request(&device, NETWORK_MANAGER_DEVICE_INTERFACE, "State"),
        nm_request(&device, NETWORK_MANAGER_DEVICE_INTERFACE, "HwAddress"),
        nm_request(
            &format!("{NETWORK_MANAGER_DEVICE_PATH_PREFIX}03"),
            NETWORK_MANAGER_DEVICE_INTERFACE,
            "Interface",
        ),
        nm_request(
            NETWORK_MANAGER_DEVICE_PATH_PREFIX,
            NETWORK_MANAGER_DEVICE_INTERFACE,
            "Interface",
        ),
        nm_request(
            &format!("{NETWORK_MANAGER_DEVICE_PATH_PREFIX}2/extra"),
            NETWORK_MANAGER_DEVICE_INTERFACE,
            "Interface",
        ),
        nm_request(
            &resolved_link_path(2),
            "org.freedesktop.resolve1.Link",
            "DNSEx",
        ),
        resolved_request(
            SYSTEMD_RESOLVED_MANAGER_PATH,
            SYSTEMD_RESOLVED_MANAGER_INTERFACE,
            "DNSEx",
        ),
        nm_request("", manager_interface, "Devices"),
    ] {
        assert_eq!(
            foundation.read_property(request).await,
            Err(DbusError::UnsupportedRequest)
        );
    }

    tokio::time::sleep(Duration::from_millis(50)).await;
    assert!(
        mock.calls().is_empty(),
        "a rejected request reached the mock service"
    );
    assert!(
        monitor.application_calls().is_empty(),
        "a rejected request reached the wire"
    );
    monitor.assert_clean(0);

    foundation.shutdown().await.expect("shutdown");
    mock.shutdown().await;
}

#[tokio::test]
async fn absent_services_are_reported_without_activation() {
    let harness = Harness::start();
    let monitor = harness.monitor().await;

    for service in [Service::NetworkManager, Service::SystemdResolved] {
        let error = DbusFoundation::connect(service)
            .await
            .expect_err("absent service must not connect");
        assert_eq!(error, DbusError::ServiceUnavailable);
    }

    assert!(
        monitor
            .wait_for_count(WAIT, 2, |message| message.is_bus_driver_call()
                && message.member_is("NameHasOwner"))
            .await,
        "absence checks did not reach the bus driver"
    );
    let messages = monitor.snapshot();
    assert!(
        !messages
            .iter()
            .any(|message| message.member_is("StartServiceByName")),
        "service activation was attempted"
    );
    assert!(
        !messages
            .iter()
            .any(|message| message.member_is("GetNameOwner")),
        "GetNameOwner must not run after NameHasOwner returned false"
    );
    assert!(
        monitor.application_calls().is_empty(),
        "an absent service received an application message"
    );
    monitor.assert_clean(0);
}

#[tokio::test]
async fn signals_are_exact_and_sender_pinned() {
    let harness = Harness::start();
    let monitor = harness.monitor().await;
    let mock = MockService::start(Service::NetworkManager, MockReplies::new()).await;
    let foundation = DbusFoundation::connect(Service::NetworkManager)
        .await
        .expect("preflight");
    monitor.clear();

    assert_eq!(
        foundation.subscribe(&[]).await.err(),
        Some(DbusError::UnsupportedRequest)
    );
    assert_eq!(
        foundation
            .subscribe(&[SignalKind::StateChanged, SignalKind::StateChanged])
            .await
            .err(),
        Some(DbusError::UnsupportedRequest)
    );

    let mut subscription = foundation
        .subscribe(&[
            SignalKind::StateChanged,
            SignalKind::DeviceAdded,
            SignalKind::DeviceRemoved,
        ])
        .await
        .expect("subscribe");

    assert!(
        monitor
            .wait_for_count(WAIT, 3, |message| message.is_bus_driver_call()
                && message.member_is("AddMatch"))
            .await,
        "signal setup did not register three match rules"
    );
    let messages = monitor.snapshot();
    let add_matches = messages
        .iter()
        .filter(|message| message.is_bus_driver_call() && message.member_is("AddMatch"))
        .count();
    assert_eq!(
        add_matches, 3,
        "exactly three match rules may be registered"
    );
    for kind in [
        SignalKind::StateChanged,
        SignalKind::DeviceAdded,
        SignalKind::DeviceRemoved,
    ] {
        let rule = find_add_match_rule(&messages, kind.member())
            .unwrap_or_else(|| panic!("missing AddMatch for {}", kind.member()));
        assert!(rule.contains("type='signal'"), "rule {rule}");
        assert!(
            rule.contains("sender='org.freedesktop.NetworkManager'"),
            "rule {rule}"
        );
        assert!(
            rule.contains("interface='org.freedesktop.NetworkManager'"),
            "rule {rule}"
        );
        assert!(
            rule.contains("path='/org/freedesktop/NetworkManager'"),
            "rule {rule}"
        );
        assert!(!rule.contains("PropertiesChanged"), "rule {rule}");
        assert!(!rule.contains("NameOwnerChanged"), "rule {rule}");
    }
    assert!(
        monitor.application_calls().is_empty(),
        "signal setup must not call the application service"
    );

    // A different sender on the same path/interface/member must not be
    // delivered, and PropertiesChanged has no subscription at all.
    let imposter = zbus::Connection::system()
        .await
        .expect("imposter connection");
    imposter
        .request_name("org.example.Imposter")
        .await
        .expect("imposter name");
    imposter
        .emit_signal(
            None::<&str>,
            NETWORK_MANAGER_MANAGER_PATH,
            NETWORK_MANAGER_MANAGER_INTERFACE,
            "StateChanged",
            &(999u32,),
        )
        .await
        .expect("imposter signal");
    mock.emit_properties_changed().await;
    mock.emit_state_changed(70).await;

    let event = tokio::time::timeout(WAIT, subscription.next())
        .await
        .expect("signal deadline")
        .expect("signal");
    assert_eq!(event, SignalEvent::StateChanged(70));
    assert_eq!(event.kind(), SignalKind::StateChanged);
    assert_eq!(event.state(), Some(70));

    let device = nm_device_path(3);
    mock.emit_device_added(&device).await;
    let event = tokio::time::timeout(WAIT, subscription.next())
        .await
        .expect("device-added deadline")
        .expect("device-added signal");
    assert_eq!(event.kind(), SignalKind::DeviceAdded);
    assert_eq!(event.device_path(), Some(device.as_str()));

    mock.emit_device_removed(&device).await;
    let event = tokio::time::timeout(WAIT, subscription.next())
        .await
        .expect("device-removed deadline")
        .expect("device-removed signal");
    assert_eq!(event.kind(), SignalKind::DeviceRemoved);
    assert_eq!(event.device_path(), Some(device.as_str()));

    // The payload path must not leak through Debug.
    let debug = format!("{event:?}");
    assert!(!debug.contains(&device));
    assert!(debug.contains("<redacted>"));

    drop(subscription);
    foundation.shutdown().await.expect("shutdown");
    drop(imposter);
    mock.shutdown().await;
}

#[tokio::test]
async fn resolved_has_no_signal_surface() {
    let harness = Harness::start();
    let monitor = harness.monitor().await;
    let mock = MockService::start(Service::SystemdResolved, MockReplies::new()).await;
    let foundation = DbusFoundation::connect(Service::SystemdResolved)
        .await
        .expect("preflight");
    monitor.clear();

    assert_eq!(
        foundation
            .subscribe(&[SignalKind::StateChanged])
            .await
            .err(),
        Some(DbusError::UnsupportedRequest)
    );
    tokio::time::sleep(Duration::from_millis(50)).await;
    assert!(
        !monitor
            .snapshot()
            .iter()
            .any(|message| message.member_is("AddMatch")),
        "resolved must not register signal match rules"
    );
    assert!(monitor.application_calls().is_empty());

    foundation.shutdown().await.expect("shutdown");
    mock.shutdown().await;
}

#[tokio::test]
async fn dns_ex_with_legacy_three_field_signature_is_rejected() {
    let harness = Harness::start();
    let _monitor = harness.monitor().await;
    let mut replies = MockReplies::new();
    replies.insert(
        (SYSTEMD_RESOLVED_MANAGER_INTERFACE, "DNSEx"),
        MockReply::Value(zbus::Variant::from(vec![(
            2i32,
            2i32,
            vec![192u8, 0, 2, 53],
        )])),
    );
    let mock = MockService::start(Service::SystemdResolved, replies).await;

    let foundation = DbusFoundation::connect(Service::SystemdResolved)
        .await
        .expect("preflight");
    let error = foundation
        .read_property(resolved_request(
            SYSTEMD_RESOLVED_MANAGER_PATH,
            SYSTEMD_RESOLVED_MANAGER_INTERFACE,
            "DNSEx",
        ))
        .await
        .expect_err("legacy three-field DNSEx must be rejected");
    assert_eq!(error, DbusError::MalformedResponse);

    drop(foundation);
    mock.shutdown().await;
}

#[tokio::test]
async fn resolved_reads_use_the_manager_allow_list_and_nested_signatures() {
    let harness = Harness::start();
    let monitor = harness.monitor().await;
    let mut replies = MockReplies::new();
    replies.insert(
        (SYSTEMD_RESOLVED_MANAGER_INTERFACE, "DNSEx"),
        MockReply::Value(zbus::Variant::from(vec![(
            2i32,
            2i32,
            vec![192u8, 0, 2, 53],
            5353u16,
            "sni.invalid".to_owned(),
        )])),
    );
    replies.insert(
        (SYSTEMD_RESOLVED_MANAGER_INTERFACE, "Domains"),
        MockReply::Value(zbus::Variant::from(vec![(
            2i32,
            "example.test".to_owned(),
            false,
        )])),
    );
    replies.insert(
        (SYSTEMD_RESOLVED_MANAGER_INTERFACE, "ResolvConfMode"),
        MockReply::Value(zbus::Variant::from("stub")),
    );
    let mock = MockService::start(Service::SystemdResolved, replies).await;
    monitor.clear();

    let foundation = DbusFoundation::connect(Service::SystemdResolved)
        .await
        .expect("preflight");

    let dns = foundation
        .read_property(resolved_request(
            SYSTEMD_RESOLVED_MANAGER_PATH,
            SYSTEMD_RESOLVED_MANAGER_INTERFACE,
            "DNSEx",
        ))
        .await
        .expect("DNSEx");
    assert_eq!(dns.value_signature().to_string(), "a(iiayqs)");
    let servers: Vec<(i32, i32, Vec<u8>, u16, String)> = dns.try_into().expect("decode DNSEx");
    assert_eq!(
        servers,
        vec![(2, 2, vec![192, 0, 2, 53], 5353, "sni.invalid".to_owned(),)]
    );

    let domains = foundation
        .read_property(resolved_request(
            SYSTEMD_RESOLVED_MANAGER_PATH,
            SYSTEMD_RESOLVED_MANAGER_INTERFACE,
            "Domains",
        ))
        .await
        .expect("Domains");
    assert_eq!(domains.value_signature().to_string(), "a(isb)");
    let domains: Vec<(i32, String, bool)> = domains.try_into().expect("decode Domains");
    assert_eq!(domains, vec![(2, "example.test".to_owned(), false)]);

    let mode = foundation
        .read_property(resolved_request(
            SYSTEMD_RESOLVED_MANAGER_PATH,
            SYSTEMD_RESOLVED_MANAGER_INTERFACE,
            "ResolvConfMode",
        ))
        .await
        .expect("ResolvConfMode");
    assert_eq!(String::try_from(mode).expect("decode s"), "stub");

    assert_eq!(
        foundation
            .read_property(nm_request(
                NETWORK_MANAGER_MANAGER_PATH,
                NETWORK_MANAGER_MANAGER_INTERFACE,
                "Devices",
            ))
            .await,
        Err(DbusError::UnsupportedRequest),
        "an NM request on a resolved connection must be rejected locally"
    );

    let calls = mock.calls();
    assert_eq!(calls.len(), 3);
    for call in &calls {
        assert_eq!(call.flags & no_auto_start(), no_auto_start());
    }
    monitor.assert_clean(3);

    foundation.shutdown().await.expect("shutdown");
    mock.shutdown().await;
}

#[tokio::test]
async fn timeouts_are_bounded_and_sanitized() {
    let harness = Harness::start();
    let monitor = harness.monitor().await;
    let mut replies = MockReplies::new();
    replies.insert(
        (NETWORK_MANAGER_MANAGER_INTERFACE, "Devices"),
        MockReply::Silent,
    );
    let mock = MockService::start(Service::NetworkManager, replies).await;
    monitor.clear();

    let timeout = Duration::from_millis(200);
    let foundation = DbusFoundation::connect_with_timeout(Service::NetworkManager, timeout)
        .await
        .expect("preflight");

    let started = Instant::now();
    let error = foundation
        .read_property(nm_request(
            NETWORK_MANAGER_MANAGER_PATH,
            NETWORK_MANAGER_MANAGER_INTERFACE,
            "Devices",
        ))
        .await
        .expect_err("silent service must time out");
    assert_eq!(error, DbusError::Timeout);
    assert!(
        started.elapsed() < Duration::from_secs(2),
        "timeout was not bounded"
    );
    assert_eq!(error.to_string(), "D-Bus operation timed out");
    assert!(!format!("{error:?}").contains("mock"));

    foundation.shutdown().await.expect("shutdown");
    monitor.assert_clean(1);
    mock.shutdown().await;
}

#[tokio::test]
async fn malformed_and_error_replies_map_to_bounded_categories() {
    let harness = Harness::start();
    let monitor = harness.monitor().await;
    let device = nm_device_path(2);
    let active = nm_active_connection_path(1);
    let mut replies = MockReplies::new();
    replies.insert(
        (NETWORK_MANAGER_DEVICE_INTERFACE, "Interface"),
        MockReply::Value(zbus::Variant::from(42u32)),
    );
    replies.insert(
        (NETWORK_MANAGER_DEVICE_INTERFACE, "IpInterface"),
        MockReply::Error("org.freedesktop.DBus.Error.AccessDenied"),
    );
    replies.insert(
        (NETWORK_MANAGER_ACTIVE_CONNECTION_INTERFACE, "State"),
        MockReply::Error("org.freedesktop.DBus.Error.ServiceUnknown"),
    );
    replies.insert(
        (NETWORK_MANAGER_ACTIVE_CONNECTION_INTERFACE, "Type"),
        MockReply::Error("org.freedesktop.DBus.Error.UnknownProperty"),
    );
    replies.insert(
        (NETWORK_MANAGER_ACTIVE_CONNECTION_INTERFACE, "Vpn"),
        MockReply::Error("org.example.Custom.Error"),
    );
    let mock = MockService::start(Service::NetworkManager, replies).await;
    monitor.clear();
    let foundation = DbusFoundation::connect(Service::NetworkManager)
        .await
        .expect("preflight");

    let cases = [
        (
            nm_request(&device, NETWORK_MANAGER_DEVICE_INTERFACE, "Interface"),
            DbusError::MalformedResponse,
        ),
        (
            nm_request(&device, NETWORK_MANAGER_DEVICE_INTERFACE, "IpInterface"),
            DbusError::PermissionDenied,
        ),
        (
            nm_request(
                &active,
                NETWORK_MANAGER_ACTIVE_CONNECTION_INTERFACE,
                "State",
            ),
            DbusError::ServiceUnavailable,
        ),
        (
            nm_request(&active, NETWORK_MANAGER_ACTIVE_CONNECTION_INTERFACE, "Type"),
            DbusError::MalformedResponse,
        ),
        (
            nm_request(&active, NETWORK_MANAGER_ACTIVE_CONNECTION_INTERFACE, "Vpn"),
            DbusError::Protocol,
        ),
    ];

    for (request, expected) in cases {
        let error = foundation
            .read_property(request)
            .await
            .expect_err("reply must be rejected");
        assert_eq!(error, expected);
        for text in [error.to_string(), format!("{error:?}")] {
            assert!(!text.contains("mock service error detail"), "text {text}");
            assert!(!text.contains("org.freedesktop"), "text {text}");
            assert!(!text.contains("/org/"), "text {text}");
        }
    }

    assert_eq!(mock.calls().len(), 5);
    monitor.assert_clean(5);
    foundation.shutdown().await.expect("shutdown");
    mock.shutdown().await;
}

#[tokio::test]
async fn repeated_setup_and_shutdown_leave_no_traffic_behind() {
    let harness = Harness::start();
    let monitor = harness.monitor().await;
    let mock = MockService::start(Service::NetworkManager, nm_replies()).await;

    for _ in 0..3 {
        let foundation = DbusFoundation::connect(Service::NetworkManager)
            .await
            .expect("preflight");
        let value = foundation
            .read_property(nm_request(
                NETWORK_MANAGER_MANAGER_PATH,
                NETWORK_MANAGER_MANAGER_INTERFACE,
                "Devices",
            ))
            .await
            .expect("Devices");
        assert_eq!(value.value_signature().to_string(), "ao");
        foundation.shutdown().await.expect("shutdown");
    }

    assert!(
        monitor
            .wait_for_count(WAIT, 3, |message| message.is_application_call())
            .await,
        "not all reads reached the wire"
    );
    let application = monitor.application_calls();
    assert_eq!(application.len(), 3);
    for message in &application {
        assert!(message.member_is("Get"));
        assert_eq!(message.flags & no_auto_start(), no_auto_start());
    }
    monitor.assert_clean(3);

    mock.shutdown().await;
    let deadline = Instant::now() + WAIT;
    loop {
        match DbusFoundation::connect(Service::NetworkManager).await {
            Err(DbusError::ServiceUnavailable) => break,
            Err(other) => panic!("unexpected error after service shutdown: {other}"),
            Ok(foundation) => {
                foundation.shutdown().await.expect("shutdown");
                assert!(Instant::now() < deadline, "service name was never released");
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
        }
    }
    assert!(
        !monitor
            .snapshot()
            .iter()
            .any(|message| message.member_is("StartServiceByName")),
        "the absent service was activated"
    );
}

#[tokio::test]
async fn service_disappearance_fails_closed_without_activation() {
    let harness = Harness::start();
    let monitor = harness.monitor().await;
    let mock = MockService::start(Service::NetworkManager, nm_replies()).await;
    let foundation = DbusFoundation::connect(Service::NetworkManager)
        .await
        .expect("preflight");

    mock.shutdown().await;
    wait_for_service_absence(Service::NetworkManager).await;

    let error = foundation
        .read_property(nm_request(
            NETWORK_MANAGER_MANAGER_PATH,
            NETWORK_MANAGER_MANAGER_INTERFACE,
            "Devices",
        ))
        .await
        .expect_err("a vanished service must fail the read");
    assert_eq!(error, DbusError::ServiceUnavailable);

    tokio::time::sleep(Duration::from_millis(50)).await;
    let messages = monitor.snapshot();
    assert!(
        !messages
            .iter()
            .any(|message| message.member_is("StartServiceByName")),
        "the vanished service was activated"
    );
    let application_calls = monitor.application_calls();
    assert!(!application_calls.is_empty());
    for message in &application_calls {
        assert!(message.member_is("Get"));
        assert_eq!(
            message.flags & no_auto_start(),
            no_auto_start(),
            "the failed call must still carry NO_AUTO_START"
        );
    }

    foundation.shutdown().await.expect("shutdown");
}
