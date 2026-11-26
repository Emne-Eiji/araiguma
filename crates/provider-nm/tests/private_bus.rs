//! Wire-level tests for the NetworkManager provider.
//!
//! The tests reuse the accepted private-bus harness from `provider-dbus`
//! (private daemon, monitor, serialized address) and add a path-aware mock so
//! each active connection and device can return different approved values.

#[path = "../../provider-dbus/tests/common/mod.rs"]
mod common;

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use futures::StreamExt;
use zbus::message::Type;
use zbus::{Connection, Message, MessageStream, OwnedObjectPath, Variant};

use araiguma_core::merge::merge;
use araiguma_core::model::{ConnectionObservation, ConnectionState, ObservationSource};
use araiguma_core::provider::DiscoveryProvider;
use araiguma_core::provider::{
    ProviderId, ProviderObservation, ProviderState, SectionKind, SectionOutcome, SectionResult,
};
use araiguma_core::snapshot::parse_snapshot_document;
use araiguma_provider_dbus::policy::{
    NETWORK_MANAGER_ACTIVE_CONNECTION_INTERFACE, NETWORK_MANAGER_DEVICE_INTERFACE,
    NETWORK_MANAGER_MANAGER_INTERFACE, NETWORK_MANAGER_MANAGER_PATH,
};
use araiguma_provider_dbus::Service;
use araiguma_provider_nm::NetworkManagerProvider;

use common::{
    nm_active_connection_path, nm_device_path, no_auto_start, wait_for_service_absence, Harness,
    MockCall, WireMonitor,
};

/// The interface a real NetworkManager service exports on
/// active-connection objects. Deliberately NOT imported from the production
/// policy constant (WO-06C-C1): the original mock reply tables were keyed on
/// that constant, so mock and implementation repeated the same wrong
/// interface name and the reads succeeded tautologically.
const REAL_ACTIVE_CONNECTION_INTERFACE: &str = "org.freedesktop.NetworkManager.Connection.Active";

const WAIT: Duration = Duration::from_secs(3);
const TIMEOUT: Duration = Duration::from_secs(2);

const EXTENDED_PROHIBITED_MEMBERS: &[&str] = &[
    "GetAll",
    "Set",
    "GetManagedObjects",
    "Introspect",
    "GetSettings",
    "GetSecrets",
    "StartServiceByName",
    "ListNames",
    "ListActivatableNames",
    "GetDevices",
    "GetAllDevices",
    "ActivateConnection",
    "AddAndActivateConnection",
    "DeactivateConnection",
    "Enable",
    "RequestScan",
    "Disconnect",
    "CheckConnectivity",
    "Reload",
    "Sleep",
];

#[derive(Clone)]
enum PathReply {
    Value(Variant<'static>),
    Error(&'static str),
    Silent,
}

type PathReplies = HashMap<(String, String, String), PathReply>;

struct PathMockService {
    conn: Connection,
    calls: Arc<Mutex<Vec<MockCall>>>,
    task: tokio::task::JoinHandle<()>,
}

impl PathMockService {
    async fn start(replies: PathReplies) -> Self {
        let conn = Connection::system()
            .await
            .expect("mock service connection to private bus");
        conn.request_name("org.freedesktop.NetworkManager")
            .await
            .expect("mock service must own the NetworkManager name");

        let calls = Arc::new(Mutex::new(Vec::new()));
        let recorded = Arc::clone(&calls);
        let task_conn = conn.clone();
        let mut stream = MessageStream::from(task_conn.clone());
        let task = tokio::spawn(async move {
            while let Some(item) = stream.next().await {
                let Ok(message) = item else { break };
                if message.message_type() != Type::MethodCall {
                    continue;
                }

                let header = message.header();
                let interface = header
                    .interface()
                    .map(|value| value.as_str().to_owned())
                    .unwrap_or_default();
                let member = header
                    .member()
                    .map(|value| value.as_str().to_owned())
                    .unwrap_or_default();
                let path = header
                    .path()
                    .map(|value| value.as_str().to_owned())
                    .unwrap_or_default();
                let signature = header.signature().to_string();
                let flags = header.primary().flags().bits();
                let body = message
                    .body()
                    .deserialize::<(String, String)>()
                    .ok()
                    .map(|(interface, property)| vec![interface, property]);
                drop(header);

                recorded
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner())
                    .push(MockCall {
                        path: path.clone(),
                        interface: interface.clone(),
                        member: member.clone(),
                        signature,
                        flags,
                        body: body.clone(),
                    });

                if interface != "org.freedesktop.DBus.Properties" || member != "Get" {
                    let reply = Message::error(
                        &message.header(),
                        "org.freedesktop.DBus.Error.UnknownMethod",
                    )
                    .build(&("unsupported mock method",))
                    .expect("build mock error");
                    let _ = task_conn.send(&reply).await;
                    continue;
                }

                let Some(parts) = body else {
                    let reply =
                        Message::error(&message.header(), "org.freedesktop.DBus.Error.InvalidArgs")
                            .build(&("bad Get body",))
                            .expect("build mock error");
                    let _ = task_conn.send(&reply).await;
                    continue;
                };
                let Some((requested_interface, rest)) = parts.split_first() else {
                    continue;
                };
                let Some(property) = rest.first() else {
                    continue;
                };
                let key = (path, requested_interface.clone(), property.clone());
                let reply = replies.get(&key).unwrap_or(&PathReply::Error(
                    "org.freedesktop.DBus.Error.UnknownProperty",
                ));

                match reply {
                    PathReply::Value(value) => {
                        let reply = Message::method_return(&message.header())
                            .build(value)
                            .expect("build property reply");
                        let _ = task_conn.send(&reply).await;
                    }
                    PathReply::Error(name) => {
                        let reply = Message::error(&message.header(), *name)
                            .build(&("mock service error detail",))
                            .expect("build error reply");
                        let _ = task_conn.send(&reply).await;
                    }
                    PathReply::Silent => {}
                }
            }
        });

        Self { conn, calls, task }
    }

    fn calls(&self) -> Vec<MockCall> {
        self.calls
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone()
    }

    async fn emit_state_changed(&self, state: u32) {
        self.conn
            .emit_signal(
                None::<&str>,
                NETWORK_MANAGER_MANAGER_PATH,
                NETWORK_MANAGER_MANAGER_INTERFACE,
                "StateChanged",
                &(state,),
            )
            .await
            .expect("emit StateChanged");
    }

    async fn emit_state_changed_bad(&self) {
        self.conn
            .emit_signal(
                None::<&str>,
                NETWORK_MANAGER_MANAGER_PATH,
                NETWORK_MANAGER_MANAGER_INTERFACE,
                "StateChanged",
                &("not-a-state",),
            )
            .await
            .expect("emit malformed StateChanged");
    }

    async fn emit_device_added(&self, path: &str) {
        self.conn
            .emit_signal(
                None::<&str>,
                NETWORK_MANAGER_MANAGER_PATH,
                NETWORK_MANAGER_MANAGER_INTERFACE,
                "DeviceAdded",
                &(OwnedObjectPath::try_from(path).expect("device path"),),
            )
            .await
            .expect("emit DeviceAdded");
    }

    async fn emit_device_removed(&self, path: &str) {
        self.conn
            .emit_signal(
                None::<&str>,
                NETWORK_MANAGER_MANAGER_PATH,
                NETWORK_MANAGER_MANAGER_INTERFACE,
                "DeviceRemoved",
                &(OwnedObjectPath::try_from(path).expect("device path"),),
            )
            .await
            .expect("emit DeviceRemoved");
    }

    async fn emit_properties_changed(&self) {
        self.conn
            .emit_signal(
                None::<&str>,
                NETWORK_MANAGER_MANAGER_PATH,
                NETWORK_MANAGER_MANAGER_INTERFACE,
                "PropertiesChanged",
                &(
                    "org.freedesktop.NetworkManager",
                    HashMap::<String, Variant<'_>>::new(),
                    Vec::<String>::new(),
                ),
            )
            .await
            .expect("emit PropertiesChanged");
    }

    async fn shutdown(self) {
        self.task.abort();
        let _ = self.conn.clone().close().await;
    }
}

impl Drop for PathMockService {
    fn drop(&mut self) {
        self.task.abort();
    }
}

struct SysfsFixture {
    root: PathBuf,
}

impl SysfsFixture {
    fn new(entries: &[(&str, u32)]) -> Self {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let sequence = COUNTER.fetch_add(1, Ordering::Relaxed);
        let root = std::env::temp_dir()
            .join("opencode")
            .join("araiguma-nm-tests")
            .join(format!("{}-{sequence}", std::process::id()));
        for (name, ifindex) in entries {
            let dir = root.join("class").join("net").join(name);
            std::fs::create_dir_all(&dir).expect("create sysfs fixture interface");
            std::fs::write(dir.join("ifindex"), format!("{ifindex}\n"))
                .expect("write sysfs fixture ifindex");
        }
        Self { root }
    }
}

impl Drop for SysfsFixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

fn insert(
    replies: &mut PathReplies,
    path: &str,
    interface: &'static str,
    property: &'static str,
    value: Variant<'static>,
) {
    replies.insert(
        (path.to_owned(), interface.to_owned(), property.to_owned()),
        PathReply::Value(value),
    );
}

fn insert_error(
    replies: &mut PathReplies,
    path: &str,
    interface: &'static str,
    property: &'static str,
    name: &'static str,
) {
    replies.insert(
        (path.to_owned(), interface.to_owned(), property.to_owned()),
        PathReply::Error(name),
    );
}

fn object_paths(paths: &[String]) -> Variant<'static> {
    let paths: Vec<OwnedObjectPath> = paths
        .iter()
        .map(|path| OwnedObjectPath::try_from(path.as_str()).expect("object path"))
        .collect();
    Variant::from(paths)
}

fn observed(result: &SectionResult<ConnectionObservation>) -> &[ConnectionObservation] {
    match result {
        SectionResult::Observed(items) => items,
        other => panic!("expected observed connections, got {other:?}"),
    }
}

fn report_section(
    report: &araiguma_core::provider::ProviderReport,
    section: SectionKind,
) -> SectionOutcome {
    report
        .sections
        .iter()
        .find(|entry| entry.section == section)
        .map(|entry| entry.outcome.clone())
        .unwrap_or_else(|| panic!("missing section {section:?}"))
}

fn assert_no_extended_prohibited(monitor: &WireMonitor) {
    for message in monitor.snapshot() {
        if let Some(member) = &message.member {
            assert!(
                !EXTENDED_PROHIBITED_MEMBERS.contains(&member.as_str()),
                "prohibited member {member} appeared on the wire"
            );
        }
    }
}

fn assert_only_approved_gets(calls: &[MockCall]) {
    for call in calls {
        assert_eq!(call.interface, "org.freedesktop.DBus.Properties");
        assert_eq!(call.member, "Get");
        assert!(call.signature_is("ss"), "signature {}", call.signature);
        assert_eq!(call.flags & no_auto_start(), no_auto_start());
        let body = call.body.as_deref().expect("Get body");
        assert_eq!(body.len(), 2);
        assert!(
            matches!(
                body[0].as_str(),
                NETWORK_MANAGER_MANAGER_INTERFACE
                    | REAL_ACTIVE_CONNECTION_INTERFACE
                    | NETWORK_MANAGER_DEVICE_INTERFACE
            ),
            "unexpected interface in Get body: {}",
            body[0]
        );
        assert!(
            matches!(
                body[1].as_str(),
                "ActiveConnections"
                    | "Devices"
                    | "State"
                    | "Type"
                    | "Vpn"
                    | "Interface"
                    | "IpInterface"
            ),
            "unexpected property in Get body: {}",
            body[1]
        );
    }
}

async fn observe_with_state(state: u32) -> ProviderObservation {
    let _harness = Harness::start();
    let fixture = SysfsFixture::new(&[]);
    let ac = nm_active_connection_path(1);
    let mut replies = PathReplies::new();
    insert(
        &mut replies,
        NETWORK_MANAGER_MANAGER_PATH,
        NETWORK_MANAGER_MANAGER_INTERFACE,
        "ActiveConnections",
        object_paths(std::slice::from_ref(&ac)),
    );
    insert(
        &mut replies,
        &ac,
        REAL_ACTIVE_CONNECTION_INTERFACE,
        "Devices",
        object_paths(&[]),
    );
    insert(
        &mut replies,
        &ac,
        REAL_ACTIVE_CONNECTION_INTERFACE,
        "State",
        Variant::from(state),
    );
    insert(
        &mut replies,
        &ac,
        REAL_ACTIVE_CONNECTION_INTERFACE,
        "Type",
        Variant::from("802-3-ethernet"),
    );
    insert(
        &mut replies,
        &ac,
        REAL_ACTIVE_CONNECTION_INTERFACE,
        "Vpn",
        Variant::from(false),
    );
    let mock = PathMockService::start(replies).await;
    let mut provider = NetworkManagerProvider::with_sys_root_and_timeout(&fixture.root, TIMEOUT);
    let observation = provider.observe().await;
    mock.shutdown().await;
    observation
}

#[test]
fn construction_is_side_effect_free_and_identity_is_stable() {
    let provider = NetworkManagerProvider::with_sys_root("/definitely/not/a/real/sys");
    assert_eq!(provider.id(), ProviderId::NetworkManager);
    assert_eq!(provider.priority(), 2);
    assert_eq!(provider.drained_signals(), 0);
    let debug = format!("{provider:?}");
    assert!(debug.contains("<redacted>"));
    assert!(!debug.contains("/definitely"));
}

#[tokio::test]
async fn connection_state_conversion_handles_all_u32_boundaries() {
    let defined = [
        (0u32, ConnectionState::Unknown),
        (1, ConnectionState::Activating),
        (2, ConnectionState::Activated),
        (3, ConnectionState::Deactivating),
        (4, ConnectionState::Deactivated),
        (5, ConnectionState::Other(5)),
        (255, ConnectionState::Other(255)),
    ];
    for (raw, expected) in defined {
        let observation = observe_with_state(raw).await;
        let connections = observed(&observation.connections);
        assert_eq!(connections.len(), 1, "state {raw}");
        assert_eq!(connections[0].state, expected, "state {raw}");
        assert_eq!(observation.state(), ProviderState::Available, "state {raw}");
        assert!(!observation.fidelity_limited, "state {raw}");
    }

    for raw in [256u32, u32::MAX] {
        let observation = observe_with_state(raw).await;
        assert_eq!(
            observation.connections,
            SectionResult::Failed("malformedResponse".to_owned()),
            "state {raw}"
        );
        assert_eq!(observation.state(), ProviderState::Degraded, "state {raw}");
        let report = observation.summarize();
        assert_eq!(
            report_section(&report, SectionKind::Connections),
            SectionOutcome::Failed {
                error: "malformedResponse".to_owned()
            },
            "state {raw}"
        );
        let snapshot = merge(vec![observation], u64::from(raw));
        assert!(snapshot.connections.is_none(), "state {raw}");
        snapshot
            .validate_v4_reports()
            .expect("malformed state report is still a valid v4 report");
    }
}

#[tokio::test]
async fn malformed_state_on_one_connection_does_not_fabricate_a_valid_observation() {
    let harness = Harness::start();
    let monitor = harness.monitor().await;
    let fixture = SysfsFixture::new(&[("eth0", 2)]);
    let ac1 = nm_active_connection_path(1);
    let ac2 = nm_active_connection_path(2);
    let dev = nm_device_path(2);
    let mut replies = PathReplies::new();
    insert(
        &mut replies,
        NETWORK_MANAGER_MANAGER_PATH,
        NETWORK_MANAGER_MANAGER_INTERFACE,
        "ActiveConnections",
        object_paths(&[ac1.clone(), ac2.clone()]),
    );
    insert(
        &mut replies,
        &ac1,
        REAL_ACTIVE_CONNECTION_INTERFACE,
        "Devices",
        object_paths(std::slice::from_ref(&dev)),
    );
    insert(
        &mut replies,
        &ac1,
        REAL_ACTIVE_CONNECTION_INTERFACE,
        "State",
        Variant::from(2u32),
    );
    insert(
        &mut replies,
        &ac1,
        REAL_ACTIVE_CONNECTION_INTERFACE,
        "Type",
        Variant::from("802-3-ethernet"),
    );
    insert(
        &mut replies,
        &ac1,
        REAL_ACTIVE_CONNECTION_INTERFACE,
        "Vpn",
        Variant::from(false),
    );
    insert(
        &mut replies,
        &dev,
        NETWORK_MANAGER_DEVICE_INTERFACE,
        "Interface",
        Variant::from("eth0"),
    );
    insert(
        &mut replies,
        &ac2,
        REAL_ACTIVE_CONNECTION_INTERFACE,
        "Devices",
        object_paths(&[]),
    );
    insert(
        &mut replies,
        &ac2,
        REAL_ACTIVE_CONNECTION_INTERFACE,
        "State",
        Variant::from(256u32),
    );
    insert(
        &mut replies,
        &ac2,
        REAL_ACTIVE_CONNECTION_INTERFACE,
        "Type",
        Variant::from("802-3-ethernet"),
    );
    insert(
        &mut replies,
        &ac2,
        REAL_ACTIVE_CONNECTION_INTERFACE,
        "Vpn",
        Variant::from(false),
    );
    let mock = PathMockService::start(replies).await;
    monitor.clear();

    let mut provider = NetworkManagerProvider::with_sys_root_and_timeout(&fixture.root, TIMEOUT);
    let observation = provider.observe().await;
    let connections = observed(&observation.connections);
    assert_eq!(connections.len(), 1, "only the valid connection is emitted");
    assert_eq!(connections[0].state, ConnectionState::Activated);
    assert_eq!(connections[0].devices, vec![2]);
    assert!(connections[0]
        .notes
        .iter()
        .any(|note| note == "partialConnectionRead"));
    assert!(observation
        .notes
        .iter()
        .any(|note| note == "partialConnectionRead"));
    assert!(observation.fidelity_limited);
    assert_eq!(observation.state(), ProviderState::Degraded);
    let snapshot = merge(vec![observation], 1);
    assert_eq!(snapshot.connections.as_ref().map(Vec::len), Some(1));
    snapshot
        .validate_v4_reports()
        .expect("partial malformed-state report is valid v4");
    monitor.assert_clean(8);
    mock.shutdown().await;
}

#[tokio::test]
async fn provider_reads_active_connections_through_the_real_interface() {
    let harness = Harness::start();
    let monitor = harness.monitor().await;
    let fixture = SysfsFixture::new(&[("eth0", 2)]);
    let ac = nm_active_connection_path(1);
    let dev = nm_device_path(2);
    let mut replies = PathReplies::new();
    insert(
        &mut replies,
        NETWORK_MANAGER_MANAGER_PATH,
        NETWORK_MANAGER_MANAGER_INTERFACE,
        "ActiveConnections",
        object_paths(std::slice::from_ref(&ac)),
    );
    insert(
        &mut replies,
        &ac,
        REAL_ACTIVE_CONNECTION_INTERFACE,
        "Devices",
        object_paths(std::slice::from_ref(&dev)),
    );
    insert(
        &mut replies,
        &ac,
        REAL_ACTIVE_CONNECTION_INTERFACE,
        "State",
        Variant::from(2u32),
    );
    insert(
        &mut replies,
        &ac,
        REAL_ACTIVE_CONNECTION_INTERFACE,
        "Type",
        Variant::from("802-3-ethernet"),
    );
    insert(
        &mut replies,
        &ac,
        REAL_ACTIVE_CONNECTION_INTERFACE,
        "Vpn",
        Variant::from(false),
    );
    insert(
        &mut replies,
        &dev,
        NETWORK_MANAGER_DEVICE_INTERFACE,
        "Interface",
        Variant::from("eth0"),
    );
    // The mock answers ONLY the real interface; a request naming the legacy
    // interface would find no entry and reply UnknownProperty.
    let mock = PathMockService::start(replies).await;
    monitor.clear();

    let mut provider = NetworkManagerProvider::with_sys_root_and_timeout(&fixture.root, TIMEOUT);
    let observation = provider.observe().await;
    let connections = observed(&observation.connections);
    assert_eq!(connections.len(), 1);
    assert_eq!(connections[0].connection_type, "802-3-ethernet");
    assert_eq!(connections[0].state, ConnectionState::Activated);
    assert_eq!(connections[0].devices, vec![2]);
    assert!(!connections[0].vpn);
    assert_eq!(observation.state(), ProviderState::Available);

    // The production constant must be exactly the independently written
    // real-interface literal; the provider's own requests therefore use it.
    let production_interface = NETWORK_MANAGER_ACTIVE_CONNECTION_INTERFACE;
    assert_eq!(production_interface, REAL_ACTIVE_CONNECTION_INTERFACE);

    // Every active-connection Get that reached the service carried the real
    // interface name, character for character.
    let calls = mock.calls();
    for property in ["Devices", "State", "Type", "Vpn"] {
        let body = calls
            .iter()
            .find(|call| {
                call.member == "Get"
                    && call.path == ac
                    && call.body.as_ref().map(|body| body[1].as_str()) == Some(property)
            })
            .unwrap_or_else(|| panic!("no Get for {property}"))
            .body
            .as_deref()
            .expect("Get body");
        assert_eq!(
            body[0].as_str(),
            "org.freedesktop.NetworkManager.Connection.Active"
        );
    }
    assert!(calls.iter().all(|call| {
        call.body.as_deref().map(|body| body[0].as_str())
            != Some("org.freedesktop.NetworkManager.ActiveConnection")
    }));

    // The independent monitor saw the real interface on the actual wire and
    // never saw the legacy one.
    let application = monitor.application_calls();
    assert!(application.iter().any(|message| {
        message.body.as_deref().map(|body| body[0].as_str())
            == Some("org.freedesktop.NetworkManager.Connection.Active")
    }));
    assert!(application.iter().all(|message| {
        message.body.as_deref().map(|body| body[0].as_str())
            != Some("org.freedesktop.NetworkManager.ActiveConnection")
    }));

    assert_only_approved_gets(&calls);
    monitor.assert_clean(6);

    mock.shutdown().await;
}

#[tokio::test]
async fn available_service_discovers_connections_devices_vpn_and_state() {
    let harness = Harness::start();
    let monitor = harness.monitor().await;
    let fixture = SysfsFixture::new(&[("eth0", 2), ("wlan0", 3)]);
    let ac1 = nm_active_connection_path(1);
    let ac2 = nm_active_connection_path(2);
    let dev2 = nm_device_path(2);
    let dev3 = nm_device_path(3);
    let mut replies = PathReplies::new();
    insert(
        &mut replies,
        NETWORK_MANAGER_MANAGER_PATH,
        NETWORK_MANAGER_MANAGER_INTERFACE,
        "ActiveConnections",
        object_paths(&[ac1.clone(), ac2.clone()]),
    );
    insert(
        &mut replies,
        &ac1,
        REAL_ACTIVE_CONNECTION_INTERFACE,
        "Devices",
        object_paths(std::slice::from_ref(&dev2)),
    );
    insert(
        &mut replies,
        &ac1,
        REAL_ACTIVE_CONNECTION_INTERFACE,
        "State",
        Variant::from(2u32),
    );
    insert(
        &mut replies,
        &ac1,
        REAL_ACTIVE_CONNECTION_INTERFACE,
        "Type",
        Variant::from("802-3-ethernet"),
    );
    insert(
        &mut replies,
        &ac1,
        REAL_ACTIVE_CONNECTION_INTERFACE,
        "Vpn",
        Variant::from(false),
    );
    insert(
        &mut replies,
        &dev2,
        NETWORK_MANAGER_DEVICE_INTERFACE,
        "Interface",
        Variant::from("eth0"),
    );
    insert(
        &mut replies,
        &ac2,
        REAL_ACTIVE_CONNECTION_INTERFACE,
        "Devices",
        object_paths(std::slice::from_ref(&dev3)),
    );
    insert(
        &mut replies,
        &ac2,
        REAL_ACTIVE_CONNECTION_INTERFACE,
        "State",
        Variant::from(1u32),
    );
    insert(
        &mut replies,
        &ac2,
        REAL_ACTIVE_CONNECTION_INTERFACE,
        "Type",
        Variant::from("vpn"),
    );
    insert(
        &mut replies,
        &ac2,
        REAL_ACTIVE_CONNECTION_INTERFACE,
        "Vpn",
        Variant::from(true),
    );
    insert(
        &mut replies,
        &dev3,
        NETWORK_MANAGER_DEVICE_INTERFACE,
        "Interface",
        Variant::from("wlan0"),
    );
    let mock = PathMockService::start(replies).await;
    monitor.clear();

    let mut provider = NetworkManagerProvider::with_sys_root_and_timeout(&fixture.root, TIMEOUT);
    let observation = provider.observe().await;
    let connections = observed(&observation.connections);
    assert_eq!(connections.len(), 2);
    assert_eq!(observation.state(), ProviderState::Available);
    assert!(!observation.fidelity_limited);
    assert!(observation
        .notes
        .iter()
        .any(|note| note == "pollingAndApprovedSignals"));

    let first = connections
        .iter()
        .find(|item| item.connection_type == "802-3-ethernet")
        .expect("ethernet connection");
    assert_eq!(first.state, ConnectionState::Activated);
    assert_eq!(first.devices, vec![2]);
    assert!(!first.vpn);
    assert_eq!(first.sources, vec![ObservationSource::NetworkManager]);
    let second = connections
        .iter()
        .find(|item| item.connection_type == "vpn")
        .expect("vpn connection");
    assert_eq!(second.state, ConnectionState::Activating);
    assert_eq!(second.devices, vec![3]);
    assert!(second.vpn);

    let calls = mock.calls();
    assert_eq!(
        calls.len(),
        11,
        "1 manager + 2*4 connection + 2 device reads"
    );
    assert_only_approved_gets(&calls);
    assert!(!calls
        .iter()
        .any(|call| call.body.as_deref().map(|body| body[1].as_str()) == Some("Connection")));
    assert!(!calls
        .iter()
        .any(|call| call.body.as_deref().map(|body| body[1].as_str()) == Some("Uuid")));

    assert!(
        monitor
            .wait_for_count(WAIT, 11, |message| message.is_application_call())
            .await
    );
    monitor.assert_clean(11);
    assert_no_extended_prohibited(&monitor);

    let snapshot = merge(vec![observation], 1_700_000_000_000);
    snapshot.validate_v4_reports().expect("v4 report contract");
    assert_eq!(snapshot.connections.as_ref().map(Vec::len), Some(2));
    let json = serde_json::to_string(&snapshot).expect("serialize snapshot");
    assert!(!json.contains("/org/freedesktop"));
    assert!(!json.contains("hwAddress"));
    assert!(!json.contains("uuid"));
    assert!(!json.contains("ssid"));

    drop(provider);
    mock.shutdown().await;
}

#[tokio::test]
async fn empty_active_connections_is_an_observed_empty_collection() {
    let harness = Harness::start();
    let monitor = harness.monitor().await;
    let mut replies = PathReplies::new();
    insert(
        &mut replies,
        NETWORK_MANAGER_MANAGER_PATH,
        NETWORK_MANAGER_MANAGER_INTERFACE,
        "ActiveConnections",
        object_paths(&[]),
    );
    let mock = PathMockService::start(replies).await;
    monitor.clear();

    let mut provider = NetworkManagerProvider::with_sys_root_and_timeout("/sys", TIMEOUT);
    let observation = provider.observe().await;
    assert!(matches!(
        observation.connections,
        SectionResult::Observed(ref items) if items.is_empty()
    ));
    assert_eq!(observation.state(), ProviderState::Available);
    let report = observation.summarize();
    assert_eq!(
        report_section(&report, SectionKind::Connections),
        SectionOutcome::Observed { count: 0 }
    );
    monitor.assert_clean(1);
    mock.shutdown().await;
}

#[tokio::test]
async fn multiple_and_duplicate_content_observations_are_preserved() {
    let harness = Harness::start();
    let monitor = harness.monitor().await;
    let fixture = SysfsFixture::new(&[("eth0", 2)]);
    let ac1 = nm_active_connection_path(1);
    let ac2 = nm_active_connection_path(2);
    let dev = nm_device_path(2);
    let mut replies = PathReplies::new();
    insert(
        &mut replies,
        NETWORK_MANAGER_MANAGER_PATH,
        NETWORK_MANAGER_MANAGER_INTERFACE,
        "ActiveConnections",
        object_paths(&[ac1.clone(), ac2.clone()]),
    );
    for path in [&ac1, &ac2] {
        insert(
            &mut replies,
            path,
            REAL_ACTIVE_CONNECTION_INTERFACE,
            "Devices",
            object_paths(std::slice::from_ref(&dev)),
        );
        insert(
            &mut replies,
            path,
            REAL_ACTIVE_CONNECTION_INTERFACE,
            "State",
            Variant::from(2u32),
        );
        insert(
            &mut replies,
            path,
            REAL_ACTIVE_CONNECTION_INTERFACE,
            "Type",
            Variant::from("802-3-ethernet"),
        );
        insert(
            &mut replies,
            path,
            REAL_ACTIVE_CONNECTION_INTERFACE,
            "Vpn",
            Variant::from(false),
        );
    }
    insert(
        &mut replies,
        &dev,
        NETWORK_MANAGER_DEVICE_INTERFACE,
        "Interface",
        Variant::from("eth0"),
    );
    let mock = PathMockService::start(replies).await;
    monitor.clear();

    let mut provider = NetworkManagerProvider::with_sys_root_and_timeout(&fixture.root, TIMEOUT);
    let observation = provider.observe().await;
    let connections = observed(&observation.connections);
    assert_eq!(connections.len(), 2);
    assert_eq!(
        connections[0].connection_type,
        connections[1].connection_type
    );
    assert_eq!(connections[0].state, connections[1].state);
    assert_eq!(connections[0].devices, connections[1].devices);
    assert_eq!(connections[0].vpn, connections[1].vpn);

    let snapshot = merge(vec![observation], 1);
    assert_eq!(snapshot.connections.as_ref().map(Vec::len), Some(2));
    monitor.assert_clean(11);
    mock.shutdown().await;
}

#[tokio::test]
async fn approved_signals_are_subscribed_and_never_replace_fresh_reads() {
    let harness = Harness::start();
    let monitor = harness.monitor().await;
    let mut replies = PathReplies::new();
    insert(
        &mut replies,
        NETWORK_MANAGER_MANAGER_PATH,
        NETWORK_MANAGER_MANAGER_INTERFACE,
        "ActiveConnections",
        object_paths(&[]),
    );
    let mock = PathMockService::start(replies).await;
    monitor.clear();

    let mut provider = NetworkManagerProvider::with_sys_root_and_timeout("/sys", TIMEOUT);
    let _ = provider.observe().await;
    assert!(
        monitor
            .wait_for_count(WAIT, 3, |message| {
                message.is_bus_driver_call() && message.member_is("AddMatch")
            })
            .await,
        "three approved match rules were not registered"
    );
    let setup = monitor.snapshot();
    for kind in ["StateChanged", "DeviceAdded", "DeviceRemoved"] {
        let rule = setup
            .iter()
            .filter(|message| message.member_is("AddMatch"))
            .filter_map(|message| message.body.as_ref())
            .filter_map(|body| body.first())
            .find(|rule| rule.contains(&format!("member='{kind}'")))
            .unwrap_or_else(|| panic!("missing AddMatch for {kind}"));
        assert!(rule.contains("sender='org.freedesktop.NetworkManager'"));
        assert!(rule.contains("path='/org/freedesktop/NetworkManager'"));
        assert!(!rule.contains("PropertiesChanged"));
        assert!(!rule.contains("NameOwnerChanged"));
    }

    monitor.clear();
    mock.emit_state_changed(70).await;
    mock.emit_device_added(&nm_device_path(3)).await;
    mock.emit_device_removed(&nm_device_path(3)).await;
    mock.emit_properties_changed().await;
    mock.emit_state_changed_bad().await;

    let imposter = Connection::system().await.expect("imposter connection");
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

    let observation = provider.observe().await;
    assert!(matches!(
        observation.connections,
        SectionResult::Observed(ref items) if items.is_empty()
    ));
    assert_eq!(
        provider.drained_signals(),
        4,
        "three valid signals plus one malformed signal are consumed"
    );
    assert!(
        monitor.application_calls().len() == 1,
        "the signal batch must cause exactly one fresh property read"
    );
    let approved_match_rules: Vec<_> = monitor
        .snapshot()
        .into_iter()
        .filter(|message| message.member_is("AddMatch"))
        .filter_map(|message| message.body)
        .flatten()
        .filter(|rule| {
            ["StateChanged", "DeviceAdded", "DeviceRemoved"]
                .iter()
                .any(|member| rule.contains(&format!("member='{member}'")))
        })
        .collect();
    assert!(
        approved_match_rules.is_empty(),
        "the second observation must not re-subscribe: {approved_match_rules:#?}"
    );
    monitor.assert_clean(1);

    drop(imposter);
    drop(provider);
    mock.shutdown().await;
}

#[tokio::test]
async fn permission_denial_is_explicit_and_bounded() {
    let harness = Harness::start();
    let monitor = harness.monitor().await;
    let mut replies = PathReplies::new();
    insert_error(
        &mut replies,
        NETWORK_MANAGER_MANAGER_PATH,
        NETWORK_MANAGER_MANAGER_INTERFACE,
        "ActiveConnections",
        "org.freedesktop.DBus.Error.AccessDenied",
    );
    let mock = PathMockService::start(replies).await;
    monitor.clear();

    let mut provider = NetworkManagerProvider::with_sys_root_and_timeout("/sys", TIMEOUT);
    let observation = provider.observe().await;
    assert_eq!(observation.connections, SectionResult::PermissionDenied);
    assert_eq!(observation.state(), ProviderState::Degraded);
    let report = observation.summarize();
    assert_eq!(
        report_section(&report, SectionKind::Connections),
        SectionOutcome::PermissionDenied
    );
    let snapshot = merge(vec![observation], 1);
    assert!(snapshot.connections.is_none());
    monitor.assert_clean(1);
    mock.shutdown().await;
}

#[tokio::test]
async fn absent_and_disappeared_service_never_leaves_stale_observations() {
    let harness = Harness::start();
    let monitor = harness.monitor().await;

    let mut provider = NetworkManagerProvider::with_sys_root_and_timeout("/sys", TIMEOUT);
    let absent = provider.observe().await;
    assert_eq!(absent.connections, SectionResult::Unavailable);
    assert_eq!(absent.state(), ProviderState::Unavailable);
    let absent_snapshot = merge(vec![absent], 1);
    assert!(absent_snapshot.connections.is_none());
    absent_snapshot
        .validate_v4_reports()
        .expect("unavailable provider report is still a valid v4 report");
    assert!(
        monitor
            .wait_for_count(WAIT, 1, |message| {
                message.is_bus_driver_call() && message.member_is("NameHasOwner")
            })
            .await
    );
    assert!(
        !monitor
            .snapshot()
            .iter()
            .any(|message| message.member_is("GetNameOwner")),
        "GetNameOwner must not run when NameHasOwner is false"
    );
    assert!(monitor.application_calls().is_empty());

    let mut replies = PathReplies::new();
    insert(
        &mut replies,
        NETWORK_MANAGER_MANAGER_PATH,
        NETWORK_MANAGER_MANAGER_INTERFACE,
        "ActiveConnections",
        object_paths(&[]),
    );
    let mock = PathMockService::start(replies).await;
    let present = provider.observe().await;
    assert!(matches!(
        present.connections,
        SectionResult::Observed(ref items) if items.is_empty()
    ));

    mock.shutdown().await;
    wait_for_service_absence(Service::NetworkManager).await;
    let vanished = provider.observe().await;
    assert_eq!(vanished.connections, SectionResult::Unavailable);
    let vanished_snapshot = merge(vec![vanished], 2);
    assert!(vanished_snapshot.connections.is_none());
    vanished_snapshot
        .validate_v4_reports()
        .expect("disappeared provider report is still a valid v4 report");
    assert!(
        !monitor
            .snapshot()
            .iter()
            .any(|message| message.member_is("StartServiceByName")),
        "the absent service must never be activated"
    );
    assert!(
        monitor
            .wait_for_count(WAIT, 2, |message| message.is_application_call())
            .await,
        "the present and vanished observations must both reach the wire"
    );
    monitor.assert_clean(2);
}

#[tokio::test]
async fn timeout_is_bounded_and_sanitized() {
    let harness = Harness::start();
    let monitor = harness.monitor().await;
    let mut replies = PathReplies::new();
    replies.insert(
        (
            NETWORK_MANAGER_MANAGER_PATH.to_owned(),
            NETWORK_MANAGER_MANAGER_INTERFACE.to_owned(),
            "ActiveConnections".to_owned(),
        ),
        PathReply::Silent,
    );
    let mock = PathMockService::start(replies).await;
    monitor.clear();

    let mut provider =
        NetworkManagerProvider::with_sys_root_and_timeout("/sys", Duration::from_millis(200));
    let started = Instant::now();
    let observation = provider.observe().await;
    assert_eq!(
        observation.connections,
        SectionResult::Failed("timeout".to_owned())
    );
    assert!(started.elapsed() < Duration::from_secs(2));
    assert_eq!(observation.state(), ProviderState::Degraded);
    let debug = format!("{:?}", observation.connections);
    assert!(!debug.contains("mock service error detail"));
    assert!(!debug.contains("org.freedesktop"));
    monitor.assert_clean(1);
    mock.shutdown().await;
}

#[tokio::test]
async fn malformed_manager_reply_is_a_bounded_failure() {
    let harness = Harness::start();
    let monitor = harness.monitor().await;
    let mut replies = PathReplies::new();
    insert(
        &mut replies,
        NETWORK_MANAGER_MANAGER_PATH,
        NETWORK_MANAGER_MANAGER_INTERFACE,
        "ActiveConnections",
        Variant::from(42u32),
    );
    let mock = PathMockService::start(replies).await;
    monitor.clear();

    let mut provider = NetworkManagerProvider::with_sys_root_and_timeout("/sys", TIMEOUT);
    let observation = provider.observe().await;
    assert_eq!(
        observation.connections,
        SectionResult::Failed("malformedResponse".to_owned())
    );
    assert_eq!(observation.state(), ProviderState::Degraded);
    monitor.assert_clean(1);
    mock.shutdown().await;
}

#[tokio::test]
async fn partial_connection_read_keeps_successful_observations() {
    let harness = Harness::start();
    let monitor = harness.monitor().await;
    let fixture = SysfsFixture::new(&[("eth0", 2)]);
    let ac1 = nm_active_connection_path(1);
    let ac2 = nm_active_connection_path(2);
    let dev = nm_device_path(2);
    let mut replies = PathReplies::new();
    insert(
        &mut replies,
        NETWORK_MANAGER_MANAGER_PATH,
        NETWORK_MANAGER_MANAGER_INTERFACE,
        "ActiveConnections",
        object_paths(&[ac1.clone(), ac2.clone()]),
    );
    insert(
        &mut replies,
        &ac1,
        REAL_ACTIVE_CONNECTION_INTERFACE,
        "Devices",
        object_paths(std::slice::from_ref(&dev)),
    );
    insert(
        &mut replies,
        &ac1,
        REAL_ACTIVE_CONNECTION_INTERFACE,
        "State",
        Variant::from(2u32),
    );
    insert(
        &mut replies,
        &ac1,
        REAL_ACTIVE_CONNECTION_INTERFACE,
        "Type",
        Variant::from("802-3-ethernet"),
    );
    insert(
        &mut replies,
        &ac1,
        REAL_ACTIVE_CONNECTION_INTERFACE,
        "Vpn",
        Variant::from(false),
    );
    insert(
        &mut replies,
        &dev,
        NETWORK_MANAGER_DEVICE_INTERFACE,
        "Interface",
        Variant::from("eth0"),
    );
    insert(
        &mut replies,
        &ac2,
        REAL_ACTIVE_CONNECTION_INTERFACE,
        "Devices",
        object_paths(&[]),
    );
    insert(
        &mut replies,
        &ac2,
        REAL_ACTIVE_CONNECTION_INTERFACE,
        "State",
        Variant::from(2u32),
    );
    insert(
        &mut replies,
        &ac2,
        REAL_ACTIVE_CONNECTION_INTERFACE,
        "Type",
        Variant::from("802-3-ethernet"),
    );
    // Vpn is deliberately absent for ac2.
    let mock = PathMockService::start(replies).await;
    monitor.clear();

    let mut provider = NetworkManagerProvider::with_sys_root_and_timeout(&fixture.root, TIMEOUT);
    let observation = provider.observe().await;
    let connections = observed(&observation.connections);
    assert_eq!(connections.len(), 1);
    assert_eq!(connections[0].devices, vec![2]);
    assert!(observation.fidelity_limited);
    assert_eq!(observation.state(), ProviderState::Degraded);
    assert!(observation
        .notes
        .iter()
        .any(|note| note == "partialConnectionRead"));
    assert!(connections[0]
        .notes
        .iter()
        .any(|note| note == "partialConnectionRead"));
    let snapshot = merge(vec![observation], 1);
    assert_eq!(snapshot.connections.as_ref().map(Vec::len), Some(1));
    monitor.assert_clean(10);
    mock.shutdown().await;
}

#[tokio::test]
async fn unresolved_device_is_reported_without_fabricating_an_ifindex() {
    let harness = Harness::start();
    let monitor = harness.monitor().await;
    let fixture = SysfsFixture::new(&[]);
    let ac = nm_active_connection_path(1);
    let dev = nm_device_path(2);
    let mut replies = PathReplies::new();
    insert(
        &mut replies,
        NETWORK_MANAGER_MANAGER_PATH,
        NETWORK_MANAGER_MANAGER_INTERFACE,
        "ActiveConnections",
        object_paths(std::slice::from_ref(&ac)),
    );
    insert(
        &mut replies,
        &ac,
        REAL_ACTIVE_CONNECTION_INTERFACE,
        "Devices",
        object_paths(std::slice::from_ref(&dev)),
    );
    insert(
        &mut replies,
        &ac,
        REAL_ACTIVE_CONNECTION_INTERFACE,
        "State",
        Variant::from(2u32),
    );
    insert(
        &mut replies,
        &ac,
        REAL_ACTIVE_CONNECTION_INTERFACE,
        "Type",
        Variant::from("802-3-ethernet"),
    );
    insert(
        &mut replies,
        &ac,
        REAL_ACTIVE_CONNECTION_INTERFACE,
        "Vpn",
        Variant::from(false),
    );
    insert(
        &mut replies,
        &dev,
        NETWORK_MANAGER_DEVICE_INTERFACE,
        "Interface",
        Variant::from("missing0"),
    );
    let mock = PathMockService::start(replies).await;
    monitor.clear();

    let mut provider = NetworkManagerProvider::with_sys_root_and_timeout(&fixture.root, TIMEOUT);
    let observation = provider.observe().await;
    let connections = observed(&observation.connections);
    assert_eq!(connections.len(), 1);
    assert!(connections[0].devices.is_empty());
    assert!(connections[0]
        .notes
        .iter()
        .any(|note| note == "unresolvedDevice"));
    assert!(observation
        .notes
        .iter()
        .any(|note| note == "unresolvedDevice"));
    assert_eq!(observation.state(), ProviderState::Degraded);
    monitor.assert_clean(6);
    mock.shutdown().await;
}

#[tokio::test]
async fn ip_interface_is_a_fallback_only_when_interface_is_absent() {
    let harness = Harness::start();
    let monitor = harness.monitor().await;
    let fixture = SysfsFixture::new(&[("eth0", 2)]);
    let ac = nm_active_connection_path(1);
    let dev = nm_device_path(2);
    let mut replies = PathReplies::new();
    insert(
        &mut replies,
        NETWORK_MANAGER_MANAGER_PATH,
        NETWORK_MANAGER_MANAGER_INTERFACE,
        "ActiveConnections",
        object_paths(std::slice::from_ref(&ac)),
    );
    insert(
        &mut replies,
        &ac,
        REAL_ACTIVE_CONNECTION_INTERFACE,
        "Devices",
        object_paths(std::slice::from_ref(&dev)),
    );
    insert(
        &mut replies,
        &ac,
        REAL_ACTIVE_CONNECTION_INTERFACE,
        "State",
        Variant::from(2u32),
    );
    insert(
        &mut replies,
        &ac,
        REAL_ACTIVE_CONNECTION_INTERFACE,
        "Type",
        Variant::from("802-3-ethernet"),
    );
    insert(
        &mut replies,
        &ac,
        REAL_ACTIVE_CONNECTION_INTERFACE,
        "Vpn",
        Variant::from(false),
    );
    insert(
        &mut replies,
        &dev,
        NETWORK_MANAGER_DEVICE_INTERFACE,
        "Interface",
        Variant::from(""),
    );
    insert(
        &mut replies,
        &dev,
        NETWORK_MANAGER_DEVICE_INTERFACE,
        "IpInterface",
        Variant::from("eth0"),
    );
    let mock = PathMockService::start(replies).await;
    monitor.clear();

    let mut provider = NetworkManagerProvider::with_sys_root_and_timeout(&fixture.root, TIMEOUT);
    let observation = provider.observe().await;
    let connections = observed(&observation.connections);
    assert_eq!(connections[0].devices, vec![2]);
    assert!(connections[0].notes.is_empty());
    monitor.assert_clean(7);
    mock.shutdown().await;
}

#[tokio::test]
async fn empty_connection_type_is_a_partial_read_not_an_observed_item() {
    let harness = Harness::start();
    let monitor = harness.monitor().await;
    let ac = nm_active_connection_path(1);
    let mut replies = PathReplies::new();
    insert(
        &mut replies,
        NETWORK_MANAGER_MANAGER_PATH,
        NETWORK_MANAGER_MANAGER_INTERFACE,
        "ActiveConnections",
        object_paths(std::slice::from_ref(&ac)),
    );
    insert(
        &mut replies,
        &ac,
        REAL_ACTIVE_CONNECTION_INTERFACE,
        "Devices",
        object_paths(&[]),
    );
    insert(
        &mut replies,
        &ac,
        REAL_ACTIVE_CONNECTION_INTERFACE,
        "State",
        Variant::from(2u32),
    );
    insert(
        &mut replies,
        &ac,
        REAL_ACTIVE_CONNECTION_INTERFACE,
        "Type",
        Variant::from(""),
    );
    insert(
        &mut replies,
        &ac,
        REAL_ACTIVE_CONNECTION_INTERFACE,
        "Vpn",
        Variant::from(false),
    );
    let mock = PathMockService::start(replies).await;
    monitor.clear();

    let mut provider = NetworkManagerProvider::with_sys_root_and_timeout("/sys", TIMEOUT);
    let observation = provider.observe().await;
    assert_eq!(
        observation.connections,
        SectionResult::Failed("partialConnectionRead".to_owned())
    );
    assert_eq!(observation.state(), ProviderState::Degraded);
    monitor.assert_clean(4);
    mock.shutdown().await;
}

#[tokio::test]
async fn incorrect_connection_signature_is_a_partial_read() {
    let harness = Harness::start();
    let monitor = harness.monitor().await;
    let fixture = SysfsFixture::new(&[("eth0", 2)]);
    let ac1 = nm_active_connection_path(1);
    let ac2 = nm_active_connection_path(2);
    let dev = nm_device_path(2);
    let mut replies = PathReplies::new();
    insert(
        &mut replies,
        NETWORK_MANAGER_MANAGER_PATH,
        NETWORK_MANAGER_MANAGER_INTERFACE,
        "ActiveConnections",
        object_paths(&[ac1.clone(), ac2.clone()]),
    );
    insert(
        &mut replies,
        &ac1,
        REAL_ACTIVE_CONNECTION_INTERFACE,
        "Devices",
        object_paths(std::slice::from_ref(&dev)),
    );
    insert(
        &mut replies,
        &ac1,
        REAL_ACTIVE_CONNECTION_INTERFACE,
        "State",
        Variant::from(2u32),
    );
    insert(
        &mut replies,
        &ac1,
        REAL_ACTIVE_CONNECTION_INTERFACE,
        "Type",
        Variant::from("802-3-ethernet"),
    );
    insert(
        &mut replies,
        &ac1,
        REAL_ACTIVE_CONNECTION_INTERFACE,
        "Vpn",
        Variant::from(false),
    );
    insert(
        &mut replies,
        &dev,
        NETWORK_MANAGER_DEVICE_INTERFACE,
        "Interface",
        Variant::from("eth0"),
    );
    insert(
        &mut replies,
        &ac2,
        REAL_ACTIVE_CONNECTION_INTERFACE,
        "Devices",
        object_paths(&[]),
    );
    insert(
        &mut replies,
        &ac2,
        REAL_ACTIVE_CONNECTION_INTERFACE,
        "State",
        Variant::from("activated"),
    );
    let mock = PathMockService::start(replies).await;
    monitor.clear();

    let mut provider = NetworkManagerProvider::with_sys_root_and_timeout(&fixture.root, TIMEOUT);
    let observation = provider.observe().await;
    let connections = observed(&observation.connections);
    assert_eq!(connections.len(), 1);
    assert_eq!(connections[0].connection_type, "802-3-ethernet");
    assert!(observation
        .notes
        .iter()
        .any(|note| note == "partialConnectionRead"));
    assert_eq!(observation.state(), ProviderState::Degraded);
    monitor.assert_clean(8);
    mock.shutdown().await;
}

#[tokio::test]
async fn v4_serialization_round_trips_the_provider_observation() {
    let harness = Harness::start();
    let _monitor = harness.monitor().await;
    let fixture = SysfsFixture::new(&[("eth0", 2)]);
    let ac = nm_active_connection_path(1);
    let dev = nm_device_path(2);
    let mut replies = PathReplies::new();
    insert(
        &mut replies,
        NETWORK_MANAGER_MANAGER_PATH,
        NETWORK_MANAGER_MANAGER_INTERFACE,
        "ActiveConnections",
        object_paths(std::slice::from_ref(&ac)),
    );
    insert(
        &mut replies,
        &ac,
        REAL_ACTIVE_CONNECTION_INTERFACE,
        "Devices",
        object_paths(std::slice::from_ref(&dev)),
    );
    insert(
        &mut replies,
        &ac,
        REAL_ACTIVE_CONNECTION_INTERFACE,
        "State",
        Variant::from(4u32),
    );
    insert(
        &mut replies,
        &ac,
        REAL_ACTIVE_CONNECTION_INTERFACE,
        "Type",
        Variant::from("vpn"),
    );
    insert(
        &mut replies,
        &ac,
        REAL_ACTIVE_CONNECTION_INTERFACE,
        "Vpn",
        Variant::from(true),
    );
    insert(
        &mut replies,
        &dev,
        NETWORK_MANAGER_DEVICE_INTERFACE,
        "Interface",
        Variant::from("eth0"),
    );
    let mock = PathMockService::start(replies).await;

    let mut provider = NetworkManagerProvider::with_sys_root_and_timeout(&fixture.root, TIMEOUT);
    let observation = provider.observe().await;
    let snapshot = merge(vec![observation], 1_700_000_000_123);
    snapshot.validate_v4_reports().expect("v4 report contract");
    let value = serde_json::to_value(&snapshot).expect("snapshot value");
    let parsed = parse_snapshot_document(value).expect("snapshot round trip");
    assert_eq!(parsed.schema_version, 4);
    assert_eq!(parsed.connections, snapshot.connections);
    assert_eq!(
        parsed.connections.as_ref().map(Vec::len),
        Some(1),
        "the observed connection must survive serialization"
    );

    drop(provider);
    mock.shutdown().await;
}

#[tokio::test]
async fn sensitive_service_text_never_reaches_diagnostics() {
    let harness = Harness::start();
    let monitor = harness.monitor().await;
    let mut replies = PathReplies::new();
    insert_error(
        &mut replies,
        NETWORK_MANAGER_MANAGER_PATH,
        NETWORK_MANAGER_MANAGER_INTERFACE,
        "ActiveConnections",
        "org.example.Custom.Error",
    );
    let mock = PathMockService::start(replies).await;
    monitor.clear();

    let mut provider = NetworkManagerProvider::with_sys_root_and_timeout("/sys", TIMEOUT);
    let observation = provider.observe().await;
    let text = format!("{:?}", observation.connections);
    assert!(!text.contains("mock service error detail"));
    assert!(!text.contains("org.example"));
    assert!(!text.contains("/org/"));
    assert_eq!(
        observation.connections,
        SectionResult::Failed("internalError".to_owned())
    );
    monitor.assert_clean(1);
    mock.shutdown().await;
}
