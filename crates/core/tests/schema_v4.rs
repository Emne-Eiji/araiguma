//! WO-06 v4 schema, identity, merge, and diff contract tests.

use std::net::IpAddr;

use araiguma_core::merge::merge;
use araiguma_core::model::{
    ConnectionObservation, ConnectionState, DnsObservation, DnsScope, DnsServer, Interface,
    MacObservation, ObservationSource,
};
use araiguma_core::provider::{
    ProviderId, ProviderObservation, ProviderState, SectionKind, SectionOutcome, SectionResult,
};
use araiguma_core::snapshot::{
    diff_snapshots, parse_snapshot_document, NetworkEvent, NetworkSnapshot, SNAPSHOT_SCHEMA_VERSION,
};

fn dns(
    scope: DnsScope,
    mode: Option<&str>,
    servers: &[&str],
    search: &[&str],
    routing: &[&str],
    sources: &[ObservationSource],
) -> DnsObservation {
    DnsObservation {
        scope,
        mode: mode.map(str::to_string),
        servers: servers
            .iter()
            .map(|address| DnsServer {
                address: address.parse::<IpAddr>().unwrap(),
                port: None,
            })
            .collect(),
        search_domains: search.iter().map(|s| (*s).to_string()).collect(),
        routing_domains: routing.iter().map(|s| (*s).to_string()).collect(),
        sources: sources.to_vec(),
        notes: vec!["derivedFromManagerAggregate".to_string()],
    }
}

fn connection(
    connection_type: &str,
    state: ConnectionState,
    devices: &[u32],
    vpn: bool,
) -> ConnectionObservation {
    ConnectionObservation {
        connection_type: connection_type.to_string(),
        state,
        devices: devices.to_vec(),
        vpn,
        sources: vec![ObservationSource::NetworkManager],
        notes: vec![],
    }
}

fn observation(
    provider: ProviderId,
    priority: u8,
    dns: SectionResult<DnsObservation>,
    connections: SectionResult<ConnectionObservation>,
) -> ProviderObservation {
    ProviderObservation {
        provider,
        priority,
        interfaces: SectionResult::Unsupported,
        addresses_v4: SectionResult::Unsupported,
        addresses_v6: SectionResult::Unsupported,
        routes_v4: SectionResult::Unsupported,
        routes_v6: SectionResult::Unsupported,
        dns,
        connections,
        fidelity_limited: false,
        notes: vec![],
    }
}

fn empty_snapshot() -> NetworkSnapshot {
    NetworkSnapshot {
        schema_version: SNAPSHOT_SCHEMA_VERSION,
        timestamp_ms: 0,
        interfaces: vec![],
        addresses: vec![],
        routes: vec![],
        providers: vec![],
        dns: None,
        connections: None,
    }
}

#[test]
fn known_and_unknown_provider_identities_round_trip() {
    for (value, wire) in [
        (ProviderId::Netlink, "\"netlink\""),
        (ProviderId::SysFs, "\"sysfs\""),
        (ProviderId::NetworkManager, "\"networkManager\""),
        (ProviderId::SystemdResolved, "\"systemdResolved\""),
        (
            ProviderId::Other("futureProvider".into()),
            "\"futureProvider\"",
        ),
    ] {
        let encoded = serde_json::to_string(&value).unwrap();
        assert_eq!(encoded, wire);
        let decoded: ProviderId = serde_json::from_str(&encoded).unwrap();
        assert_eq!(decoded, value);
    }

    let unknown: ProviderId = serde_json::from_str("\"futureProvider\"").unwrap();
    assert_eq!(unknown, ProviderId::Other("futureProvider".into()));
    assert!(serde_json::from_str::<ProviderId>("\"\"").is_err());
    assert!(serde_json::from_str::<ProviderId>("7").is_err());
    assert!(serde_json::to_string(&ProviderId::Other(String::new())).is_err());
}

#[test]
fn known_and_unknown_observation_sources_round_trip() {
    for (value, wire) in [
        (ObservationSource::Netlink, "\"netlink\""),
        (ObservationSource::SysFs, "\"sysfs\""),
        (ObservationSource::ProcFs, "\"procfs\""),
        (ObservationSource::NetworkManager, "\"networkManager\""),
        (ObservationSource::SystemdResolved, "\"systemdResolved\""),
        (
            ObservationSource::Other("futureSource".into()),
            "\"futureSource\"",
        ),
    ] {
        let encoded = serde_json::to_string(&value).unwrap();
        assert_eq!(encoded, wire);
        let decoded: ObservationSource = serde_json::from_str(&encoded).unwrap();
        assert_eq!(decoded, value);
    }

    let unknown: ObservationSource = serde_json::from_str("\"futureSource\"").unwrap();
    assert_eq!(unknown, ObservationSource::Other("futureSource".into()));
    assert!(serde_json::from_str::<ObservationSource>("\"\"").is_err());
    assert!(serde_json::from_str::<ObservationSource>("null").is_err());
}

#[test]
fn known_and_unknown_section_identities_round_trip() {
    for (value, wire) in [
        (SectionKind::Interfaces, "\"interfaces\""),
        (SectionKind::AddressesV4, "\"addressesV4\""),
        (SectionKind::AddressesV6, "\"addressesV6\""),
        (SectionKind::RoutesV4, "\"routesV4\""),
        (SectionKind::RoutesV6, "\"routesV6\""),
        (SectionKind::Dns, "\"dns\""),
        (SectionKind::Connections, "\"connections\""),
        (
            SectionKind::Other("futureSection".into()),
            "\"futureSection\"",
        ),
    ] {
        let encoded = serde_json::to_string(&value).unwrap();
        assert_eq!(encoded, wire);
        let decoded: SectionKind = serde_json::from_str(&encoded).unwrap();
        assert_eq!(decoded, value);
    }

    let unknown: SectionKind = serde_json::from_str("\"futureSection\"").unwrap();
    assert_eq!(unknown, SectionKind::Other("futureSection".into()));
    assert!(serde_json::from_str::<SectionKind>("\"\"").is_err());
}

#[test]
fn section_outcome_new_variants_have_exact_wire_shapes() {
    assert_eq!(
        serde_json::to_value(SectionOutcome::Unavailable).unwrap(),
        serde_json::json!({ "kind": "unavailable" })
    );
    assert_eq!(
        serde_json::to_value(SectionOutcome::PermissionDenied).unwrap(),
        serde_json::json!({ "kind": "permissionDenied" })
    );
}

#[test]
fn v4_document_round_trips_dns_and_connections() {
    let mut snapshot = empty_snapshot();
    snapshot.dns = Some(vec![
        dns(
            DnsScope::Global,
            Some("stub"),
            &["192.168.1.1"],
            &["example.com"],
            &[],
            &[ObservationSource::SystemdResolved],
        ),
        dns(
            DnsScope::Link { ifindex: 2 },
            None,
            &["192.168.1.1"],
            &[],
            &["corp.example"],
            &[ObservationSource::SystemdResolved],
        ),
    ]);
    snapshot.connections = Some(vec![
        connection("802-3-ethernet", ConnectionState::Activated, &[2], false),
        connection("vpn", ConnectionState::Activated, &[3], true),
    ]);

    let encoded = serde_json::to_string(&snapshot).unwrap();
    let decoded: NetworkSnapshot = serde_json::from_str(&encoded).unwrap();
    assert_eq!(decoded, snapshot);
    assert_eq!(decoded.schema_version, 4);
    assert_eq!(decoded.dns.as_ref().unwrap().len(), 2);
    assert_eq!(decoded.connections.as_ref().unwrap().len(), 2);
}

#[test]
fn v4_absent_empty_and_null_collections_are_distinct() {
    // Absent means not observed.
    let raw = serde_json::json!({
        "schemaVersion": 4,
        "timestampMs": 0,
        "interfaces": [],
        "addresses": [],
        "routes": [],
        "providers": []
    });
    let parsed = parse_snapshot_document(raw).unwrap();
    assert!(parsed.dns.is_none());
    assert!(parsed.connections.is_none());

    // Empty arrays mean observed empty.
    let raw = serde_json::json!({
        "schemaVersion": 4,
        "timestampMs": 0,
        "interfaces": [],
        "addresses": [],
        "routes": [],
        "providers": [],
        "dns": [],
        "connections": []
    });
    let parsed = parse_snapshot_document(raw).unwrap();
    assert_eq!(parsed.dns, Some(vec![]));
    assert_eq!(parsed.connections, Some(vec![]));

    // Explicit null is rejected rather than treated as a third state.
    let raw = serde_json::json!({
        "schemaVersion": 4,
        "timestampMs": 0,
        "interfaces": [],
        "addresses": [],
        "routes": [],
        "providers": [],
        "dns": null
    });
    assert!(parse_snapshot_document(raw)
        .unwrap_err()
        .contains("must be an array, not null"));
}

#[test]
fn v3_document_still_parses_with_absent_v4_collections() {
    let raw = serde_json::json!({
        "schemaVersion": 3,
        "timestampMs": 0,
        "interfaces": [],
        "addresses": [],
        "routes": [],
        "providers": []
    });
    let parsed = parse_snapshot_document(raw).unwrap();
    assert_eq!(parsed.schema_version, 3);
    assert!(parsed.dns.is_none());
    assert!(parsed.connections.is_none());
}

#[test]
fn provenance_is_required_for_dns_and_connections() {
    let mut snapshot = empty_snapshot();
    snapshot.dns = Some(vec![DnsObservation {
        scope: DnsScope::Global,
        mode: None,
        servers: vec![],
        search_domains: vec![],
        routing_domains: vec![],
        sources: vec![],
        notes: vec![],
    }]);
    assert!(snapshot
        .validate_provenance()
        .unwrap_err()
        .contains("$.dns[0].sources"));

    let mut snapshot = empty_snapshot();
    snapshot.connections = Some(vec![ConnectionObservation {
        connection_type: "vpn".into(),
        state: ConnectionState::Activated,
        devices: vec![],
        vpn: true,
        sources: vec![],
        notes: vec![],
    }]);
    assert!(snapshot
        .validate_provenance()
        .unwrap_err()
        .contains("$.connections[0].sources"));
}

#[test]
fn v4_optional_provider_reports_are_content_validated() {
    let mut snapshot = empty_snapshot();
    let mut report = observation(
        ProviderId::SystemdResolved,
        2,
        SectionResult::Observed(vec![dns(
            DnsScope::Global,
            Some("stub"),
            &[],
            &[],
            &[],
            &[ObservationSource::SystemdResolved],
        )]),
        SectionResult::Unsupported,
    )
    .summarize();
    report.notes = vec!["raw /org/freedesktop/resolve1/link/_32".into()];
    snapshot.providers = vec![report];

    assert!(snapshot
        .validate_v4_reports()
        .unwrap_err()
        .contains("permitted static note"));
}

#[test]
fn v4_unknown_provider_reports_are_preserved_without_content_validation() {
    let mut snapshot = empty_snapshot();
    let mut report = observation(
        ProviderId::Other("futureProvider".into()),
        9,
        SectionResult::Observed(vec![]),
        SectionResult::Observed(vec![]),
    )
    .summarize();
    report.notes = vec!["future note".into()];
    snapshot.providers = vec![report];

    assert!(snapshot.validate_v4_reports().is_ok());
    let encoded = serde_json::to_string(&snapshot).unwrap();
    assert!(encoded.contains("futureProvider"));
    assert!(encoded.contains("future note"));
}

#[test]
fn merge_keeps_dns_and_connections_additive_and_sorted() {
    let netlink = observation(
        ProviderId::Netlink,
        0,
        SectionResult::Unsupported,
        SectionResult::Unsupported,
    );
    let resolved = observation(
        ProviderId::SystemdResolved,
        2,
        SectionResult::Observed(vec![
            dns(
                DnsScope::Link { ifindex: 3 },
                None,
                &["192.168.1.1"],
                &[],
                &[],
                &[ObservationSource::SystemdResolved],
            ),
            dns(
                DnsScope::Global,
                Some("stub"),
                &["192.168.1.1"],
                &[],
                &[],
                &[ObservationSource::SystemdResolved],
            ),
        ]),
        SectionResult::Unsupported,
    );
    let nm = observation(
        ProviderId::NetworkManager,
        2,
        SectionResult::Unsupported,
        SectionResult::Observed(vec![connection(
            "802-3-ethernet",
            ConnectionState::Activated,
            &[2],
            false,
        )]),
    );

    let snapshot = merge(vec![nm, netlink, resolved], 1);
    let dns = snapshot.dns.as_ref().unwrap();
    assert_eq!(dns.len(), 2);
    assert_eq!(dns[0].scope, DnsScope::Global);
    assert_eq!(dns[1].scope, DnsScope::Link { ifindex: 3 });
    assert_eq!(snapshot.connections.as_ref().unwrap().len(), 1);
    assert!(snapshot
        .providers
        .iter()
        .all(|report| report.sections.len() == 7));
}

#[test]
fn merge_preserves_duplicate_dns_and_connection_observations() {
    let duplicate_dns = dns(
        DnsScope::Global,
        Some("stub"),
        &["192.168.1.1"],
        &[],
        &[],
        &[ObservationSource::SystemdResolved],
    );
    let resolved = observation(
        ProviderId::SystemdResolved,
        2,
        SectionResult::Observed(vec![duplicate_dns.clone(), duplicate_dns]),
        SectionResult::Unsupported,
    );
    let snapshot = merge(vec![resolved], 1);
    assert_eq!(snapshot.dns.as_ref().unwrap().len(), 2);

    let duplicate_connection =
        connection("802-3-ethernet", ConnectionState::Activated, &[2], false);
    let nm = observation(
        ProviderId::NetworkManager,
        2,
        SectionResult::Unsupported,
        SectionResult::Observed(vec![duplicate_connection.clone(), duplicate_connection]),
    );
    let snapshot = merge(vec![nm], 1);
    assert_eq!(snapshot.connections.as_ref().unwrap().len(), 2);
}

#[test]
fn merge_emits_none_when_an_owner_did_not_observe() {
    let netlink = observation(
        ProviderId::Netlink,
        0,
        SectionResult::Unsupported,
        SectionResult::Unsupported,
    );
    let resolved = observation(
        ProviderId::SystemdResolved,
        2,
        SectionResult::Failed("timeout".into()),
        SectionResult::Unsupported,
    );
    let snapshot = merge(vec![netlink, resolved], 1);
    assert!(snapshot.dns.is_none());
}

#[test]
fn merge_emits_observed_empty_when_owner_observed_empty() {
    let resolved = observation(
        ProviderId::SystemdResolved,
        2,
        SectionResult::Observed(vec![]),
        SectionResult::Unsupported,
    );
    let snapshot = merge(vec![resolved], 1);
    assert_eq!(snapshot.dns, Some(vec![]));
}

#[test]
fn diff_dns_change_is_a_single_changed_event() {
    let mut before = empty_snapshot();
    before.dns = Some(vec![dns(
        DnsScope::Global,
        Some("stub"),
        &["192.168.1.1"],
        &[],
        &[],
        &[ObservationSource::SystemdResolved],
    )]);
    let mut after = empty_snapshot();
    after.dns = Some(vec![dns(
        DnsScope::Global,
        Some("stub"),
        &["192.168.1.2"],
        &[],
        &[],
        &[ObservationSource::SystemdResolved],
    )]);

    let events = diff_snapshots(&before, &after);
    assert_eq!(events.len(), 1);
    assert!(matches!(events[0], NetworkEvent::DnsChanged { .. }));
}

#[test]
fn diff_dns_source_change_is_remove_plus_add() {
    let mut before = empty_snapshot();
    before.dns = Some(vec![dns(
        DnsScope::Global,
        Some("stub"),
        &[],
        &[],
        &[],
        &[ObservationSource::SystemdResolved],
    )]);
    let mut after = empty_snapshot();
    after.dns = Some(vec![dns(
        DnsScope::Global,
        Some("stub"),
        &[],
        &[],
        &[],
        &[ObservationSource::Other("futureResolver".into())],
    )]);

    let events = diff_snapshots(&before, &after);
    assert!(events
        .iter()
        .any(|event| matches!(event, NetworkEvent::DnsRemoved { .. })));
    assert!(events
        .iter()
        .any(|event| matches!(event, NetworkEvent::DnsAdded { .. })));
    assert!(!events
        .iter()
        .any(|event| matches!(event, NetworkEvent::DnsChanged { .. })));
}

#[test]
fn diff_connection_change_is_remove_plus_add_and_duplicates_are_a_multiset() {
    let mut before = empty_snapshot();
    before.connections = Some(vec![
        connection("802-3-ethernet", ConnectionState::Activated, &[2], false),
        connection("802-3-ethernet", ConnectionState::Activated, &[2], false),
    ]);
    let mut after = empty_snapshot();
    after.connections = Some(vec![
        connection("802-3-ethernet", ConnectionState::Activated, &[2], false),
        connection("802-3-ethernet", ConnectionState::Deactivating, &[2], false),
    ]);

    let events = diff_snapshots(&before, &after);
    assert_eq!(
        events
            .iter()
            .filter(|event| matches!(event, NetworkEvent::ConnectionRemoved { .. }))
            .count(),
        1
    );
    assert_eq!(
        events
            .iter()
            .filter(|event| matches!(event, NetworkEvent::ConnectionAdded { .. }))
            .count(),
        1
    );
}

#[test]
fn diff_provider_report_only_change_is_material() {
    let mut before = empty_snapshot();
    before.providers = vec![observation(
        ProviderId::NetworkManager,
        2,
        SectionResult::Unsupported,
        SectionResult::Observed(vec![]),
    )
    .summarize()];
    let mut after = empty_snapshot();
    let mut report = observation(
        ProviderId::NetworkManager,
        2,
        SectionResult::Unsupported,
        SectionResult::Observed(vec![]),
    )
    .summarize();
    report.state = ProviderState::Degraded;
    after.providers = vec![report];

    let events = diff_snapshots(&before, &after);
    assert_eq!(events.len(), 1);
    assert!(matches!(
        events[0],
        NetworkEvent::ProviderReportChanged { .. }
    ));
}

#[test]
fn diff_provider_report_removal_is_reported() {
    let mut before = empty_snapshot();
    before.providers = vec![observation(
        ProviderId::NetworkManager,
        2,
        SectionResult::Unsupported,
        SectionResult::Observed(vec![]),
    )
    .summarize()];
    let after = empty_snapshot();

    let events = diff_snapshots(&before, &after);
    assert_eq!(events.len(), 1);
    assert!(matches!(
        events[0],
        NetworkEvent::ProviderReportRemoved { .. }
    ));
}

#[test]
fn topology_ignores_dns_and_connections() {
    let mut snapshot = empty_snapshot();
    snapshot.interfaces = vec![Interface {
        ifindex: 2,
        name: "eth0".into(),
        kind: araiguma_core::model::InterfaceKind::Ethernet,
        kind_detail: None,
        oper_state: None,
        admin_state: None,
        mac_address: MacObservation::NotObserved,
        mtu: None,
        tx_queue_len: None,
        master_ifindex: None,
        parent_ifindex: None,
        peer_ifindex: None,
        link_netnsid: None,
        sources: vec![ObservationSource::Netlink],
        notes: vec![],
    }];
    snapshot.dns = Some(vec![dns(
        DnsScope::Link { ifindex: 2 },
        None,
        &["192.168.1.1"],
        &[],
        &[],
        &[ObservationSource::SystemdResolved],
    )]);
    snapshot.connections = Some(vec![connection(
        "802-3-ethernet",
        ConnectionState::Activated,
        &[2],
        false,
    )]);

    let topology = araiguma_core::build_topology(&snapshot);
    assert_eq!(topology.nodes.len(), 1);
    assert!(topology.edges.is_empty());
}

#[test]
fn v4_baseline_reports_leave_dns_and_connections_unsupported() {
    let mut snapshot = empty_snapshot();
    let mut report = observation(
        ProviderId::Netlink,
        0,
        SectionResult::Unsupported,
        SectionResult::Unsupported,
    )
    .summarize();
    assert_eq!(report.sections.len(), 7);
    snapshot.providers = vec![report.clone()];
    assert!(snapshot.validate_v4_reports().is_ok());

    if let Some(section) = report
        .sections
        .iter_mut()
        .find(|entry| entry.section == SectionKind::Dns)
    {
        section.outcome = SectionOutcome::Observed { count: 0 };
    }
    snapshot.providers = vec![report];
    assert!(snapshot
        .validate_v4_reports()
        .unwrap_err()
        .contains("baseline provider"));
}

#[test]
fn dns_and_connection_wire_shapes_are_exact() {
    assert_eq!(
        serde_json::to_value(DnsScope::Global).unwrap(),
        serde_json::json!({ "kind": "global" })
    );
    assert_eq!(
        serde_json::to_value(DnsScope::Link { ifindex: 2 }).unwrap(),
        serde_json::json!({ "kind": "link", "ifindex": 2 })
    );
    assert_eq!(
        serde_json::to_value(DnsServer {
            address: "192.168.1.1".parse().unwrap(),
            port: None,
        })
        .unwrap(),
        serde_json::json!({ "address": "192.168.1.1" })
    );
    assert_eq!(
        serde_json::to_value(ConnectionState::Other(9)).unwrap(),
        serde_json::json!({ "other": 9 })
    );
}

#[test]
fn v4_writer_omits_unobserved_collections() {
    let encoded = serde_json::to_value(empty_snapshot()).unwrap();
    assert!(encoded.get("dns").is_none());
    assert!(encoded.get("connections").is_none());

    let mut snapshot = empty_snapshot();
    snapshot.dns = Some(vec![]);
    snapshot.connections = Some(vec![]);
    let encoded = serde_json::to_value(snapshot).unwrap();
    assert_eq!(encoded["dns"], serde_json::json!([]));
    assert_eq!(encoded["connections"], serde_json::json!([]));
}
