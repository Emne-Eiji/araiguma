//! Wire-level tests for the systemd-resolved provider.
//!
//! The tests reuse the accepted private-bus harness from `provider-dbus` and
//! add a path-aware mock for the three approved manager properties.

#[path = "../../provider-dbus/tests/common/mod.rs"]
mod common;

use std::collections::HashMap;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use futures::StreamExt;
use zbus::message::Type;
use zbus::{Connection, Message, MessageStream, Variant};

use araiguma_core::merge::merge;
use araiguma_core::model::{DnsObservation, DnsScope, DnsServer, ObservationSource};
use araiguma_core::provider::{
    DiscoveryProvider, ProviderId, ProviderState, SectionKind, SectionOutcome, SectionResult,
};
use araiguma_core::snapshot::parse_snapshot_document;
use araiguma_provider_dbus::policy::{
    SYSTEMD_RESOLVED_MANAGER_INTERFACE, SYSTEMD_RESOLVED_MANAGER_PATH, SYSTEMD_RESOLVED_SERVICE,
};
use araiguma_provider_resolved::SystemdResolvedProvider;

use common::{no_auto_start, wait_for_service_absence, Harness, MockCall, WireMonitor};

const WAIT: Duration = Duration::from_secs(3);
const TIMEOUT: Duration = Duration::from_secs(2);

const EXTENDED_PROHIBITED_MEMBERS: &[&str] = &[
    "GetAll",
    "Set",
    "GetManagedObjects",
    "Introspect",
    "GetLink",
    "ResolveHostname",
    "ResolveAddress",
    "ResolveRecord",
    "ResolveService",
    "SetLinkDNS",
    "SetLinkDNSEx",
    "SetLinkDomains",
    "SetLinkLLMNR",
    "SetLinkMulticastDNS",
    "SetLinkDNSOverTLS",
    "SetLinkDNSSEC",
    "SetLinkDNSSECNegativeTrustAnchors",
    "SetLinkLocalAddress",
    "SetDnssecTrustAnchor",
    "RevertLink",
    "FlushCaches",
    "StartServiceByName",
    "ListNames",
    "ListActivatableNames",
    "AddMatch",
    "RemoveMatch",
];

#[derive(Clone)]
enum Reply {
    Value(Variant<'static>),
    Error(&'static str),
    Silent,
}

type Replies = HashMap<(String, String, String), Reply>;

struct ResolvedMock {
    conn: Connection,
    calls: Arc<Mutex<Vec<MockCall>>>,
    task: tokio::task::JoinHandle<()>,
}

impl ResolvedMock {
    async fn start(replies: Replies) -> Self {
        let conn = Connection::system()
            .await
            .expect("mock resolved connection to private bus");
        conn.request_name(SYSTEMD_RESOLVED_SERVICE)
            .await
            .expect("mock resolved must own its name");

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
                let reply = replies
                    .get(&key)
                    .unwrap_or(&Reply::Error("org.freedesktop.DBus.Error.UnknownProperty"));

                match reply {
                    Reply::Value(value) => {
                        let reply = Message::method_return(&message.header())
                            .build(value)
                            .expect("build property reply");
                        let _ = task_conn.send(&reply).await;
                    }
                    Reply::Error(name) => {
                        let reply = Message::error(&message.header(), *name)
                            .build(&("mock resolved error detail",))
                            .expect("build error reply");
                        let _ = task_conn.send(&reply).await;
                    }
                    Reply::Silent => {}
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

    async fn shutdown(self) {
        self.task.abort();
        let _ = self.conn.clone().close().await;
    }
}

impl Drop for ResolvedMock {
    fn drop(&mut self) {
        self.task.abort();
    }
}

fn insert(replies: &mut Replies, property: &'static str, value: Variant<'static>) {
    replies.insert(
        (
            SYSTEMD_RESOLVED_MANAGER_PATH.to_owned(),
            SYSTEMD_RESOLVED_MANAGER_INTERFACE.to_owned(),
            property.to_owned(),
        ),
        Reply::Value(value),
    );
}

fn insert_error(replies: &mut Replies, property: &'static str, name: &'static str) {
    replies.insert(
        (
            SYSTEMD_RESOLVED_MANAGER_PATH.to_owned(),
            SYSTEMD_RESOLVED_MANAGER_INTERFACE.to_owned(),
            property.to_owned(),
        ),
        Reply::Error(name),
    );
}

fn insert_silent(replies: &mut Replies, property: &'static str) {
    replies.insert(
        (
            SYSTEMD_RESOLVED_MANAGER_PATH.to_owned(),
            SYSTEMD_RESOLVED_MANAGER_INTERFACE.to_owned(),
            property.to_owned(),
        ),
        Reply::Silent,
    );
}

fn empty_dns_ex() -> Variant<'static> {
    Variant::from(Vec::<(i32, i32, Vec<u8>, u16, String)>::new())
}

fn empty_domains() -> Variant<'static> {
    Variant::from(Vec::<(i32, String, bool)>::new())
}

fn ipv6_bytes(address: Ipv6Addr) -> Vec<u8> {
    address.octets().to_vec()
}

fn observed_dns(result: &SectionResult<DnsObservation>) -> &[DnsObservation] {
    match result {
        SectionResult::Observed(items) => items,
        other => panic!("expected observed DNS, got {other:?}"),
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

fn assert_only_approved_gets(calls: &[MockCall]) {
    for call in calls {
        assert_eq!(call.path, SYSTEMD_RESOLVED_MANAGER_PATH);
        assert_eq!(call.interface, "org.freedesktop.DBus.Properties");
        assert_eq!(call.member, "Get");
        assert!(call.signature_is("ss"), "signature {}", call.signature);
        assert_eq!(call.flags & no_auto_start(), no_auto_start());
        let body = call.body.as_deref().expect("Get body");
        assert_eq!(body.len(), 2);
        assert_eq!(body[0], SYSTEMD_RESOLVED_MANAGER_INTERFACE);
        assert!(
            matches!(body[1].as_str(), "DNSEx" | "Domains" | "ResolvConfMode"),
            "unexpected property {}",
            body[1]
        );
    }
}

fn assert_no_resolved_prohibited(monitor: &WireMonitor) {
    let messages = monitor.snapshot();
    for message in &messages {
        if let Some(member) = &message.member {
            assert!(
                !EXTENDED_PROHIBITED_MEMBERS.contains(&member.as_str()),
                "prohibited member {member} appeared on the wire"
            );
        }
        assert!(
            !message.member_is("AddMatch"),
            "resolved must not install a signal match rule"
        );
    }
    assert!(
        !messages
            .iter()
            .any(|message| message.member_is("StartServiceByName")),
        "service activation was attempted"
    );
}

#[test]
fn construction_is_side_effect_free_and_identity_is_stable() {
    let provider = SystemdResolvedProvider::new();
    assert_eq!(provider.id(), ProviderId::SystemdResolved);
    assert_eq!(provider.priority(), 3);
    let debug = format!("{provider:?}");
    assert!(debug.contains("SystemdResolvedProvider"));
    assert!(!debug.contains("dns"));
}

#[tokio::test]
async fn normal_configuration_maps_global_and_link_scopes_deterministically() {
    let harness = Harness::start();
    let monitor = harness.monitor().await;
    let v6 = Ipv6Addr::new(0x2001, 0xdb8, 0, 0, 0, 0, 0, 0x53);
    let mut replies = Replies::new();
    insert(
        &mut replies,
        "DNSEx",
        Variant::from(vec![
            (
                0i32,
                2i32,
                vec![192u8, 0, 2, 53],
                53u16,
                "sni.invalid".to_owned(),
            ),
            (0, 2, vec![192, 0, 2, 53], 53, "sni.invalid".to_owned()),
            (2, 2, vec![198, 51, 100, 53], 5353, String::new()),
            (3, 10, ipv6_bytes(v6), 53, String::new()),
        ]),
    );
    insert(
        &mut replies,
        "Domains",
        Variant::from(vec![
            (0i32, "example.test".to_owned(), false),
            (0, "corp.test".to_owned(), true),
            (2, "lan.test".to_owned(), false),
            (2, "route.test".to_owned(), true),
            (3, "v6.test".to_owned(), false),
        ]),
    );
    insert(&mut replies, "ResolvConfMode", Variant::from("stub"));
    let mock = ResolvedMock::start(replies).await;
    monitor.clear();

    let mut provider = SystemdResolvedProvider::with_timeout(TIMEOUT);
    let observation = provider.observe().await;
    assert_eq!(observation.state(), ProviderState::Available);
    assert!(!observation.fidelity_limited);
    assert_eq!(observation.notes, vec!["pollOnly".to_owned()]);
    let dns = observed_dns(&observation.dns);
    assert_eq!(dns.len(), 3);
    assert_eq!(dns[0].scope, DnsScope::Global);
    assert_eq!(
        dns[0].servers,
        vec![
            DnsServer {
                address: IpAddr::V4(Ipv4Addr::new(192, 0, 2, 53)),
                port: Some(53),
            },
            DnsServer {
                address: IpAddr::V4(Ipv4Addr::new(192, 0, 2, 53)),
                port: Some(53),
            },
        ]
    );
    assert_eq!(dns[0].search_domains, vec!["example.test".to_owned()]);
    assert_eq!(dns[0].routing_domains, vec!["corp.test".to_owned()]);
    assert_eq!(dns[0].mode.as_deref(), Some("stub"));
    assert_eq!(dns[0].sources, vec![ObservationSource::SystemdResolved]);
    assert!(dns[0]
        .notes
        .iter()
        .any(|note| note == "derivedFromManagerAggregate"));

    assert_eq!(dns[1].scope, DnsScope::Link { ifindex: 2 });
    assert_eq!(
        dns[1].servers,
        vec![DnsServer {
            address: IpAddr::V4(Ipv4Addr::new(198, 51, 100, 53)),
            port: Some(5353),
        }]
    );
    assert_eq!(dns[1].search_domains, vec!["lan.test".to_owned()]);
    assert_eq!(dns[1].routing_domains, vec!["route.test".to_owned()]);
    assert_eq!(dns[1].mode, None);

    assert_eq!(dns[2].scope, DnsScope::Link { ifindex: 3 });
    assert_eq!(
        dns[2].servers,
        vec![DnsServer {
            address: IpAddr::V6(v6),
            port: Some(53),
        }]
    );
    assert_eq!(dns[2].search_domains, vec!["v6.test".to_owned()]);
    assert!(dns[2].routing_domains.is_empty());

    let calls = mock.calls();
    assert_eq!(calls.len(), 3);
    assert_only_approved_gets(&calls);
    assert!(
        monitor
            .wait_for_count(WAIT, 3, |message| message.is_application_call())
            .await
    );
    monitor.assert_clean(3);
    assert_no_resolved_prohibited(&monitor);

    let report_json = serde_json::to_string(&observation.summarize()).expect("report json");
    assert!(!report_json.contains("192.0.2.53"));
    assert!(!report_json.contains("example.test"));
    let application_before_shutdown = monitor.application_calls().len();
    drop(provider);
    tokio::time::sleep(Duration::from_millis(50)).await;
    assert_eq!(
        monitor.application_calls().len(),
        application_before_shutdown,
        "provider shutdown must not make application calls"
    );

    let snapshot = merge(vec![observation], 1_700_000_000_000);
    snapshot.validate_v4_reports().expect("v4 report contract");
    assert_eq!(snapshot.dns.as_ref().map(Vec::len), Some(3));
    let value = serde_json::to_value(&snapshot).expect("snapshot value");
    let parsed = parse_snapshot_document(value).expect("snapshot round trip");
    assert_eq!(parsed.dns, snapshot.dns);

    mock.shutdown().await;
}

#[tokio::test]
async fn dns_ex_ports_are_retained_and_zero_is_some_zero() {
    let harness = Harness::start();
    let monitor = harness.monitor().await;
    let mut replies = Replies::new();
    insert(
        &mut replies,
        "DNSEx",
        Variant::from(vec![
            (0i32, 2i32, vec![192u8, 0, 2, 53], 0u16, String::new()),
            (0, 2, vec![192, 0, 2, 53], 5353, String::new()),
            (0, 2, vec![192, 0, 2, 53], u16::MAX, String::new()),
        ]),
    );
    insert(&mut replies, "Domains", empty_domains());
    insert(&mut replies, "ResolvConfMode", Variant::from("stub"));
    let mock = ResolvedMock::start(replies).await;
    monitor.clear();

    let mut provider = SystemdResolvedProvider::with_timeout(TIMEOUT);
    let observation = provider.observe().await;
    let dns = observed_dns(&observation.dns);
    assert_eq!(dns.len(), 1);
    assert_eq!(
        dns[0].servers,
        vec![
            DnsServer {
                address: IpAddr::V4(Ipv4Addr::new(192, 0, 2, 53)),
                port: Some(0),
            },
            DnsServer {
                address: IpAddr::V4(Ipv4Addr::new(192, 0, 2, 53)),
                port: Some(5353),
            },
            DnsServer {
                address: IpAddr::V4(Ipv4Addr::new(192, 0, 2, 53)),
                port: Some(u16::MAX),
            },
        ]
    );
    assert_ne!(dns[0].servers[0], dns[0].servers[1]);
    assert_ne!(dns[0].servers[1], dns[0].servers[2]);
    let snapshot = merge(vec![observation], 1);
    snapshot.validate_v4_reports().expect("v4 report contract");
    assert_eq!(
        snapshot.dns.as_ref().map(|dns| dns[0].servers.len()),
        Some(3)
    );
    monitor.assert_clean(3);
    mock.shutdown().await;
}

#[tokio::test]
async fn sni_is_discarded_from_observations_reports_and_debug() {
    let harness = Harness::start();
    let monitor = harness.monitor().await;
    let mut replies = Replies::new();
    insert(
        &mut replies,
        "DNSEx",
        Variant::from(vec![(
            0i32,
            2i32,
            vec![192u8, 0, 2, 53],
            53u16,
            "not a valid sni".to_owned(),
        )]),
    );
    insert(&mut replies, "Domains", empty_domains());
    insert(&mut replies, "ResolvConfMode", Variant::from("stub"));
    let mock = ResolvedMock::start(replies).await;
    monitor.clear();

    let mut provider = SystemdResolvedProvider::with_timeout(TIMEOUT);
    let observation = provider.observe().await;
    let dns = observed_dns(&observation.dns);
    assert_eq!(dns[0].servers[0].port, Some(53));
    assert!(!format!("{observation:?}").contains("not a valid sni"));
    assert!(!format!("{:?}", observation.dns).contains("not a valid sni"));
    let report_json = serde_json::to_string(&observation.summarize()).expect("report json");
    assert!(!report_json.contains("not a valid sni"));
    let snapshot = merge(vec![observation], 1);
    let snapshot_json = serde_json::to_string(&snapshot).expect("snapshot json");
    assert!(!snapshot_json.contains("not a valid sni"));
    assert!(!format!("{provider:?}").contains("not a valid sni"));
    monitor.assert_clean(3);
    mock.shutdown().await;
}

#[tokio::test]
async fn observed_empty_dns_is_not_unobserved() {
    let harness = Harness::start();
    let monitor = harness.monitor().await;
    let mut replies = Replies::new();
    insert(&mut replies, "DNSEx", empty_dns_ex());
    insert(&mut replies, "Domains", empty_domains());
    insert(&mut replies, "ResolvConfMode", Variant::from("stub"));
    let mock = ResolvedMock::start(replies).await;
    monitor.clear();

    let mut provider = SystemdResolvedProvider::with_timeout(TIMEOUT);
    let observation = provider.observe().await;
    assert!(matches!(
        observation.dns,
        SectionResult::Observed(ref items) if items.is_empty()
    ));
    assert_eq!(observation.state(), ProviderState::Available);
    let report = observation.summarize();
    assert_eq!(
        report_section(&report, SectionKind::Dns),
        SectionOutcome::Observed { count: 0 }
    );
    let snapshot = merge(vec![observation], 1);
    assert_eq!(snapshot.dns, Some(Vec::new()));
    monitor.assert_clean(3);
    assert_no_resolved_prohibited(&monitor);
    mock.shutdown().await;
}

#[tokio::test]
async fn mode_alone_does_not_fabricate_a_global_scope() {
    let harness = Harness::start();
    let monitor = harness.monitor().await;
    let mut replies = Replies::new();
    insert(&mut replies, "DNSEx", empty_dns_ex());
    insert(&mut replies, "Domains", empty_domains());
    insert(&mut replies, "ResolvConfMode", Variant::from("uplink"));
    let mock = ResolvedMock::start(replies).await;
    monitor.clear();

    let mut provider = SystemdResolvedProvider::with_timeout(TIMEOUT);
    let observation = provider.observe().await;
    assert!(matches!(
        observation.dns,
        SectionResult::Observed(ref items) if items.is_empty()
    ));
    assert_eq!(observation.state(), ProviderState::Available);
    monitor.assert_clean(3);
    mock.shutdown().await;
}

#[tokio::test]
async fn empty_dns_ex_with_link_domains_creates_only_the_link_scope() {
    let harness = Harness::start();
    let monitor = harness.monitor().await;
    let mut replies = Replies::new();
    insert(&mut replies, "DNSEx", empty_dns_ex());
    insert(
        &mut replies,
        "Domains",
        Variant::from(vec![(2i32, "lan.test".to_owned(), false)]),
    );
    insert(&mut replies, "ResolvConfMode", Variant::from("stub"));
    let mock = ResolvedMock::start(replies).await;
    monitor.clear();

    let mut provider = SystemdResolvedProvider::with_timeout(TIMEOUT);
    let observation = provider.observe().await;
    let dns = observed_dns(&observation.dns);
    assert_eq!(dns.len(), 1);
    assert_eq!(dns[0].scope, DnsScope::Link { ifindex: 2 });
    assert_eq!(dns[0].mode, None);
    assert_eq!(dns[0].search_domains, vec!["lan.test".to_owned()]);
    monitor.assert_clean(3);
    mock.shutdown().await;
}

#[tokio::test]
async fn global_dns_ex_with_empty_domains_keeps_global_mode() {
    let harness = Harness::start();
    let monitor = harness.monitor().await;
    let mut replies = Replies::new();
    insert(
        &mut replies,
        "DNSEx",
        Variant::from(vec![(
            0i32,
            2i32,
            vec![192u8, 0, 2, 53],
            53u16,
            String::new(),
        )]),
    );
    insert(&mut replies, "Domains", empty_domains());
    insert(&mut replies, "ResolvConfMode", Variant::from("stub"));
    let mock = ResolvedMock::start(replies).await;
    monitor.clear();

    let mut provider = SystemdResolvedProvider::with_timeout(TIMEOUT);
    let observation = provider.observe().await;
    let dns = observed_dns(&observation.dns);
    assert_eq!(dns.len(), 1);
    assert_eq!(dns[0].scope, DnsScope::Global);
    assert_eq!(dns[0].mode.as_deref(), Some("stub"));
    assert!(dns[0].search_domains.is_empty());
    monitor.assert_clean(3);
    mock.shutdown().await;
}

#[tokio::test]
async fn absent_service_does_not_activate_or_call_the_application() {
    let harness = Harness::start();
    let monitor = harness.monitor().await;

    let mut provider = SystemdResolvedProvider::with_timeout(TIMEOUT);
    let observation = provider.observe().await;
    assert_eq!(observation.dns, SectionResult::Unavailable);
    assert_eq!(observation.state(), ProviderState::Unavailable);
    let snapshot = merge(vec![observation], 1);
    assert!(snapshot.dns.is_none());
    snapshot.validate_v4_reports().expect("v4 report contract");

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
    monitor.assert_clean(0);
    assert_no_resolved_prohibited(&monitor);
}

#[tokio::test]
async fn service_disappearance_does_not_leave_stale_dns() {
    let harness = Harness::start();
    let monitor = harness.monitor().await;
    let mut replies = Replies::new();
    insert(
        &mut replies,
        "DNSEx",
        Variant::from(vec![(
            0i32,
            2i32,
            vec![192u8, 0, 2, 53],
            53u16,
            String::new(),
        )]),
    );
    insert(&mut replies, "Domains", empty_domains());
    insert(&mut replies, "ResolvConfMode", Variant::from("stub"));
    let mock = ResolvedMock::start(replies).await;
    monitor.clear();

    let mut provider = SystemdResolvedProvider::with_timeout(TIMEOUT);
    let present = provider.observe().await;
    let present_dns = observed_dns(&present.dns);
    assert_eq!(present_dns.len(), 1);
    assert_eq!(present_dns[0].scope, DnsScope::Global);

    mock.shutdown().await;
    wait_for_service_absence(araiguma_provider_dbus::Service::SystemdResolved).await;
    let vanished = provider.observe().await;
    assert_eq!(vanished.dns, SectionResult::Unavailable);
    let snapshot = merge(vec![vanished], 2);
    assert!(snapshot.dns.is_none());
    assert!(
        monitor
            .wait_for_count(WAIT, 6, |message| message.is_application_call())
            .await
    );
    monitor.assert_clean(6);
    assert_no_resolved_prohibited(&monitor);
}

#[tokio::test]
async fn permission_denial_is_explicit() {
    let harness = Harness::start();
    let monitor = harness.monitor().await;
    let mut replies = Replies::new();
    insert_error(
        &mut replies,
        "DNSEx",
        "org.freedesktop.DBus.Error.AccessDenied",
    );
    insert(&mut replies, "Domains", empty_domains());
    insert(&mut replies, "ResolvConfMode", Variant::from("stub"));
    let mock = ResolvedMock::start(replies).await;
    monitor.clear();

    let mut provider = SystemdResolvedProvider::with_timeout(TIMEOUT);
    let observation = provider.observe().await;
    assert_eq!(observation.dns, SectionResult::PermissionDenied);
    assert_eq!(observation.state(), ProviderState::Degraded);
    let snapshot = merge(vec![observation], 1);
    assert!(snapshot.dns.is_none());
    monitor.assert_clean(3);
    assert_no_resolved_prohibited(&monitor);
    mock.shutdown().await;
}

#[tokio::test]
async fn timeout_is_bounded_and_sanitized() {
    let harness = Harness::start();
    let monitor = harness.monitor().await;
    let mut replies = Replies::new();
    insert_silent(&mut replies, "DNSEx");
    insert_silent(&mut replies, "Domains");
    insert_silent(&mut replies, "ResolvConfMode");
    let mock = ResolvedMock::start(replies).await;
    monitor.clear();

    let mut provider = SystemdResolvedProvider::with_timeout(Duration::from_millis(200));
    let started = Instant::now();
    let observation = provider.observe().await;
    assert_eq!(observation.dns, SectionResult::Failed("timeout".to_owned()));
    assert!(started.elapsed() < Duration::from_secs(2));
    assert_eq!(observation.state(), ProviderState::Degraded);
    let debug = format!("{:?}", observation.dns);
    assert!(!debug.contains("mock resolved error detail"));
    monitor.assert_clean(3);
    mock.shutdown().await;
}

#[tokio::test]
async fn malformed_property_signature_is_a_bounded_failure() {
    let harness = Harness::start();
    let monitor = harness.monitor().await;
    let mut replies = Replies::new();
    insert(&mut replies, "DNSEx", Variant::from(42u32));
    insert(&mut replies, "Domains", empty_domains());
    insert(&mut replies, "ResolvConfMode", Variant::from("stub"));
    let mock = ResolvedMock::start(replies).await;
    monitor.clear();

    let mut provider = SystemdResolvedProvider::with_timeout(TIMEOUT);
    let observation = provider.observe().await;
    assert_eq!(
        observation.dns,
        SectionResult::Failed("malformedResponse".to_owned())
    );
    assert_eq!(observation.state(), ProviderState::Degraded);
    monitor.assert_clean(3);
    mock.shutdown().await;
}

#[tokio::test]
async fn invalid_dns_ex_entries_are_skipped_without_fabricating_global() {
    let harness = Harness::start();
    let monitor = harness.monitor().await;
    let v6 = Ipv6Addr::new(0x2001, 0xdb8, 0, 0, 0, 0, 0, 0x53);
    let mut replies = Replies::new();
    insert(
        &mut replies,
        "DNSEx",
        Variant::from(vec![
            (0i32, 5i32, vec![1u8, 2, 3, 4], 0u16, String::new()),
            (0, 2, vec![1, 2, 3], 0, String::new()),
            (-1, 2, vec![1, 2, 3, 4], 0, String::new()),
            (2, 10, ipv6_bytes(v6), 53, String::new()),
        ]),
    );
    insert(&mut replies, "Domains", empty_domains());
    insert(&mut replies, "ResolvConfMode", Variant::from("stub"));
    let mock = ResolvedMock::start(replies).await;
    monitor.clear();

    let mut provider = SystemdResolvedProvider::with_timeout(TIMEOUT);
    let observation = provider.observe().await;
    let dns = observed_dns(&observation.dns);
    assert_eq!(dns.len(), 1);
    assert_eq!(dns[0].scope, DnsScope::Link { ifindex: 2 });
    assert_eq!(dns[0].servers[0].address, IpAddr::V6(v6));
    assert!(observation.fidelity_limited);
    assert_eq!(observation.state(), ProviderState::Degraded);
    monitor.assert_clean(3);
    mock.shutdown().await;
}

#[tokio::test]
async fn malformed_domains_entries_are_skipped_safely() {
    let harness = Harness::start();
    let monitor = harness.monitor().await;
    let mut replies = Replies::new();
    insert(&mut replies, "DNSEx", empty_dns_ex());
    insert(
        &mut replies,
        "Domains",
        Variant::from(vec![
            (0i32, String::new(), false),
            (-1, "bad.test".to_owned(), false),
            (2, "ok.test".to_owned(), false),
        ]),
    );
    insert(&mut replies, "ResolvConfMode", Variant::from("stub"));
    let mock = ResolvedMock::start(replies).await;
    monitor.clear();

    let mut provider = SystemdResolvedProvider::with_timeout(TIMEOUT);
    let observation = provider.observe().await;
    let dns = observed_dns(&observation.dns);
    assert_eq!(dns.len(), 1);
    assert_eq!(dns[0].scope, DnsScope::Link { ifindex: 2 });
    assert_eq!(dns[0].search_domains, vec!["ok.test".to_owned()]);
    assert!(observation.fidelity_limited);
    monitor.assert_clean(3);
    mock.shutdown().await;
}

#[tokio::test]
async fn invalid_mode_is_not_exposed_and_marks_global_partial() {
    let harness = Harness::start();
    let monitor = harness.monitor().await;
    let mut replies = Replies::new();
    insert(
        &mut replies,
        "DNSEx",
        Variant::from(vec![(
            0i32,
            2i32,
            vec![192u8, 0, 2, 53],
            53u16,
            String::new(),
        )]),
    );
    insert(&mut replies, "Domains", empty_domains());
    insert(&mut replies, "ResolvConfMode", Variant::from("bad mode!"));
    let mock = ResolvedMock::start(replies).await;
    monitor.clear();

    let mut provider = SystemdResolvedProvider::with_timeout(TIMEOUT);
    let observation = provider.observe().await;
    let dns = observed_dns(&observation.dns);
    assert_eq!(dns.len(), 1);
    assert_eq!(dns[0].scope, DnsScope::Global);
    assert_eq!(dns[0].mode, None);
    assert!(dns[0]
        .notes
        .iter()
        .any(|note| note == "resolvConfModeUnavailable"));
    assert!(!format!("{:?}", dns[0].notes).contains("bad mode"));
    assert!(observation.fidelity_limited);
    assert_eq!(observation.state(), ProviderState::Degraded);
    monitor.assert_clean(3);
    mock.shutdown().await;
}

#[tokio::test]
async fn one_property_failure_preserves_other_observed_data() {
    let harness = Harness::start();
    let monitor = harness.monitor().await;
    let mut replies = Replies::new();
    insert_error(
        &mut replies,
        "DNSEx",
        "org.freedesktop.DBus.Error.AccessDenied",
    );
    insert(
        &mut replies,
        "Domains",
        Variant::from(vec![(0i32, "example.test".to_owned(), false)]),
    );
    insert(&mut replies, "ResolvConfMode", Variant::from("stub"));
    let mock = ResolvedMock::start(replies).await;
    monitor.clear();

    let mut provider = SystemdResolvedProvider::with_timeout(TIMEOUT);
    let observation = provider.observe().await;
    let dns = observed_dns(&observation.dns);
    assert_eq!(dns.len(), 1);
    assert_eq!(dns[0].scope, DnsScope::Global);
    assert_eq!(dns[0].search_domains, vec!["example.test".to_owned()]);
    assert!(dns[0].servers.is_empty());
    assert!(observation.fidelity_limited);
    assert_eq!(observation.state(), ProviderState::Degraded);
    monitor.assert_clean(3);
    mock.shutdown().await;
}

#[tokio::test]
async fn all_properties_failing_is_not_an_observed_empty_result() {
    let harness = Harness::start();
    let monitor = harness.monitor().await;
    let mut replies = Replies::new();
    insert_error(
        &mut replies,
        "DNSEx",
        "org.freedesktop.DBus.Error.AccessDenied",
    );
    insert_error(
        &mut replies,
        "Domains",
        "org.freedesktop.DBus.Error.AccessDenied",
    );
    insert_error(
        &mut replies,
        "ResolvConfMode",
        "org.freedesktop.DBus.Error.AccessDenied",
    );
    let mock = ResolvedMock::start(replies).await;
    monitor.clear();

    let mut provider = SystemdResolvedProvider::with_timeout(TIMEOUT);
    let observation = provider.observe().await;
    assert_eq!(observation.dns, SectionResult::PermissionDenied);
    assert_eq!(observation.state(), ProviderState::Degraded);
    let snapshot = merge(vec![observation], 1);
    assert!(snapshot.dns.is_none());
    monitor.assert_clean(3);
    mock.shutdown().await;
}

#[tokio::test]
async fn invalid_mode_without_entries_is_not_an_observed_empty_result() {
    let harness = Harness::start();
    let monitor = harness.monitor().await;
    let mut replies = Replies::new();
    insert(&mut replies, "DNSEx", empty_dns_ex());
    insert(&mut replies, "Domains", empty_domains());
    insert(&mut replies, "ResolvConfMode", Variant::from("bad mode!"));
    let mock = ResolvedMock::start(replies).await;
    monitor.clear();

    let mut provider = SystemdResolvedProvider::with_timeout(TIMEOUT);
    let observation = provider.observe().await;
    assert_eq!(
        observation.dns,
        SectionResult::Failed("malformedResponse".to_owned())
    );
    assert_eq!(observation.state(), ProviderState::Degraded);
    let snapshot = merge(vec![observation], 1);
    assert!(snapshot.dns.is_none());
    monitor.assert_clean(3);
    mock.shutdown().await;
}

#[tokio::test]
async fn sensitive_service_text_never_reaches_diagnostics() {
    let harness = Harness::start();
    let monitor = harness.monitor().await;
    let mut replies = Replies::new();
    insert_error(&mut replies, "DNSEx", "org.example.Custom.Error");
    insert(&mut replies, "Domains", empty_domains());
    insert(&mut replies, "ResolvConfMode", Variant::from("stub"));
    let mock = ResolvedMock::start(replies).await;
    monitor.clear();

    let mut provider = SystemdResolvedProvider::with_timeout(TIMEOUT);
    let observation = provider.observe().await;
    let text = format!("{:?}", observation.dns);
    assert!(!text.contains("mock resolved error detail"));
    assert!(!text.contains("org.example"));
    assert!(!text.contains("/org/"));
    assert_eq!(
        observation.dns,
        SectionResult::Failed("internalError".to_owned())
    );
    let report = observation.summarize();
    let report_json = serde_json::to_string(&report).expect("report json");
    assert!(!report_json.contains("mock resolved error detail"));
    monitor.assert_clean(3);
    mock.shutdown().await;
}
