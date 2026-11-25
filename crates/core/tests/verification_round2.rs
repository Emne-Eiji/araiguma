//! Independent adversarial verification for WO-06A.
//!
//! These tests intentionally construct duplicate, malformed, and boundary
//! inputs rather than reusing the happy-path fixtures.

use std::net::IpAddr;

use araiguma_core::merge::merge;
use araiguma_core::model::{
    ConnectionObservation, ConnectionState, DnsObservation, DnsScope, DnsServer, Interface,
    MacObservation, ObservationSource,
};
use araiguma_core::provider::{ProviderId, ProviderObservation, ProviderState, SectionResult};
use araiguma_core::snapshot::{
    diff_snapshots, parse_snapshot_document, NetworkEvent, NetworkSnapshot, SNAPSHOT_SCHEMA_VERSION,
};

fn dns(ifindex: u32, server: &str, source: ObservationSource) -> DnsObservation {
    DnsObservation {
        scope: if ifindex == 0 {
            DnsScope::Global
        } else {
            DnsScope::Link { ifindex }
        },
        mode: Some("stub".into()),
        servers: vec![DnsServer {
            address: server.parse::<IpAddr>().unwrap(),
            port: None,
        }],
        search_domains: vec![],
        routing_domains: vec![],
        sources: vec![source],
        notes: vec!["derivedFromManagerAggregate".into()],
    }
}

fn connection(state: ConnectionState, devices: &[u32]) -> ConnectionObservation {
    ConnectionObservation {
        connection_type: "802-3-ethernet".into(),
        state,
        devices: devices.to_vec(),
        vpn: false,
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

fn snapshot_with_dns(dns: Option<Vec<DnsObservation>>) -> NetworkSnapshot {
    NetworkSnapshot {
        schema_version: SNAPSHOT_SCHEMA_VERSION,
        timestamp_ms: 0,
        interfaces: vec![],
        addresses: vec![],
        routes: vec![],
        providers: vec![],
        dns,
        connections: None,
    }
}

fn snapshot_with_connections(connections: Option<Vec<ConnectionObservation>>) -> NetworkSnapshot {
    NetworkSnapshot {
        schema_version: SNAPSHOT_SCHEMA_VERSION,
        timestamp_ms: 0,
        interfaces: vec![],
        addresses: vec![],
        routes: vec![],
        providers: vec![],
        dns: None,
        connections,
    }
}

fn raw_v4(
    dns: Option<serde_json::Value>,
    connections: Option<serde_json::Value>,
) -> serde_json::Value {
    let mut value = serde_json::json!({
        "schemaVersion": 4,
        "timestampMs": 0,
        "interfaces": [],
        "addresses": [],
        "routes": [],
        "providers": []
    });
    if let Some(dns) = dns {
        value["dns"] = dns;
    }
    if let Some(connections) = connections {
        value["connections"] = connections;
    }
    value
}

fn dns_json(mode: serde_json::Value) -> serde_json::Value {
    serde_json::json!([{
        "scope": { "kind": "global" },
        "mode": mode,
        "servers": [],
        "searchDomains": [],
        "routingDomains": [],
        "sources": ["systemdResolved"],
        "notes": ["derivedFromManagerAggregate"]
    }])
}

// ── A. DNS duplicate and multiset diff behavior ─────────────────────────────

#[test]
fn dns_two_identical_to_one_emits_one_removal() {
    let item = dns(0, "192.0.2.1", ObservationSource::SystemdResolved);
    let before = snapshot_with_dns(Some(vec![item.clone(), item.clone()]));
    let after = snapshot_with_dns(Some(vec![item.clone()]));

    let events = diff_snapshots(&before, &after);
    assert_eq!(events.len(), 1, "events: {events:?}");
    assert!(matches!(events[0], NetworkEvent::DnsRemoved { .. }));
}

#[test]
fn dns_one_to_two_identical_emits_one_addition() {
    let item = dns(0, "192.0.2.1", ObservationSource::SystemdResolved);
    let before = snapshot_with_dns(Some(vec![item.clone()]));
    let after = snapshot_with_dns(Some(vec![item.clone(), item]));

    let events = diff_snapshots(&before, &after);
    assert_eq!(events.len(), 1, "events: {events:?}");
    assert!(matches!(events[0], NetworkEvent::DnsAdded { .. }));
}

#[test]
fn dns_two_identical_replaced_by_two_changed_emits_two_changes() {
    let old = dns(0, "192.0.2.1", ObservationSource::SystemdResolved);
    let new = dns(0, "192.0.2.2", ObservationSource::SystemdResolved);
    let before = snapshot_with_dns(Some(vec![old.clone(), old]));
    let after = snapshot_with_dns(Some(vec![new.clone(), new]));

    let events = diff_snapshots(&before, &after);
    assert_eq!(events.len(), 2, "events: {events:?}");
    assert!(events
        .iter()
        .all(|event| matches!(event, NetworkEvent::DnsChanged { .. })));
}

#[test]
fn dns_different_source_sets_are_never_merged() {
    let resolved = dns(0, "192.0.2.1", ObservationSource::SystemdResolved);
    let future = dns(
        0,
        "192.0.2.1",
        ObservationSource::Other("futureResolver".into()),
    );
    let before = snapshot_with_dns(Some(vec![resolved.clone()]));
    let after = snapshot_with_dns(Some(vec![future.clone()]));

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
fn dns_diff_order_is_deterministic_across_repeated_runs() {
    let old_a = dns(0, "192.0.2.1", ObservationSource::SystemdResolved);
    let old_b = dns(2, "192.0.2.2", ObservationSource::SystemdResolved);
    let new_a = dns(0, "192.0.2.3", ObservationSource::SystemdResolved);
    let new_b = dns(3, "192.0.2.4", ObservationSource::SystemdResolved);
    let before = snapshot_with_dns(Some(vec![old_b.clone(), old_a.clone()]));
    let after = snapshot_with_dns(Some(vec![new_b.clone(), new_a.clone()]));

    let first = diff_snapshots(&before, &after);
    let second = diff_snapshots(&before, &after);
    assert_eq!(first, second);
    assert_eq!(first.len(), 3, "events: {first:?}");
}

// ── B. Provider report lifecycle ────────────────────────────────────────────

#[test]
fn not_yet_observed_optional_report_is_stable_when_unchanged() {
    let report = observation(
        ProviderId::NetworkManager,
        2,
        SectionResult::Unsupported,
        SectionResult::NotObserved,
    )
    .summarize();
    assert_eq!(report.state, ProviderState::NotYetObserved);

    let snapshot = NetworkSnapshot {
        schema_version: SNAPSHOT_SCHEMA_VERSION,
        timestamp_ms: 0,
        interfaces: vec![],
        addresses: vec![],
        routes: vec![],
        providers: vec![report.clone()],
        dns: None,
        connections: None,
    };

    assert!(diff_snapshots(&snapshot, &snapshot).is_empty());
    assert!(snapshot.validate_v4_reports().is_ok());
}

#[test]
fn not_yet_observed_to_observed_is_a_provider_report_change() {
    let before_report = observation(
        ProviderId::NetworkManager,
        2,
        SectionResult::Unsupported,
        SectionResult::NotObserved,
    )
    .summarize();
    let after_report = observation(
        ProviderId::NetworkManager,
        2,
        SectionResult::Unsupported,
        SectionResult::Observed(vec![]),
    )
    .summarize();
    assert_eq!(after_report.state, ProviderState::Available);

    let before = NetworkSnapshot {
        schema_version: SNAPSHOT_SCHEMA_VERSION,
        timestamp_ms: 0,
        interfaces: vec![],
        addresses: vec![],
        routes: vec![],
        providers: vec![before_report],
        dns: None,
        connections: None,
    };
    let after = NetworkSnapshot {
        schema_version: SNAPSHOT_SCHEMA_VERSION,
        timestamp_ms: 0,
        interfaces: vec![],
        addresses: vec![],
        routes: vec![],
        providers: vec![after_report],
        dns: None,
        connections: Some(vec![]),
    };

    let events = diff_snapshots(&before, &after);
    assert_eq!(events.len(), 1, "events: {events:?}");
    assert!(matches!(
        events[0],
        NetworkEvent::ProviderReportChanged { .. }
    ));
}

#[test]
fn still_registered_not_yet_observed_provider_is_not_removed() {
    let report = observation(
        ProviderId::SystemdResolved,
        2,
        SectionResult::NotObserved,
        SectionResult::Unsupported,
    )
    .summarize();
    let before = NetworkSnapshot {
        schema_version: SNAPSHOT_SCHEMA_VERSION,
        timestamp_ms: 0,
        interfaces: vec![],
        addresses: vec![],
        routes: vec![],
        providers: vec![report.clone()],
        dns: Some(vec![dns(
            0,
            "192.0.2.1",
            ObservationSource::SystemdResolved,
        )]),
        connections: None,
    };
    let after = NetworkSnapshot {
        schema_version: SNAPSHOT_SCHEMA_VERSION,
        timestamp_ms: 0,
        interfaces: vec![],
        addresses: vec![],
        routes: vec![],
        providers: vec![report],
        dns: Some(vec![dns(
            0,
            "192.0.2.2",
            ObservationSource::SystemdResolved,
        )]),
        connections: None,
    };

    let events = diff_snapshots(&before, &after);
    assert!(
        events
            .iter()
            .any(|event| matches!(event, NetworkEvent::DnsChanged { .. })),
        "events: {events:?}"
    );
    assert!(!events
        .iter()
        .any(|event| matches!(event, NetworkEvent::ProviderReportRemoved { .. })));
}

#[test]
fn provider_report_removed_only_when_absent() {
    let report = observation(
        ProviderId::NetworkManager,
        2,
        SectionResult::Unsupported,
        SectionResult::Observed(vec![]),
    )
    .summarize();
    let before = NetworkSnapshot {
        schema_version: SNAPSHOT_SCHEMA_VERSION,
        timestamp_ms: 0,
        interfaces: vec![],
        addresses: vec![],
        routes: vec![],
        providers: vec![report],
        dns: None,
        connections: None,
    };
    let after = NetworkSnapshot {
        schema_version: SNAPSHOT_SCHEMA_VERSION,
        timestamp_ms: 0,
        interfaces: vec![],
        addresses: vec![],
        routes: vec![],
        providers: vec![],
        dns: None,
        connections: None,
    };

    let events = diff_snapshots(&before, &after);
    assert_eq!(events.len(), 1, "events: {events:?}");
    assert!(matches!(
        events[0],
        NetworkEvent::ProviderReportRemoved { .. }
    ));
}

// ── C. DNS mode validation ──────────────────────────────────────────────────

#[test]
fn dns_mode_known_and_unknown_tokens_are_preserved() {
    for mode in [
        "stub",
        "static",
        "uplink",
        "foreign",
        "missing",
        "n/a",
        "futureMode",
    ] {
        let parsed = parse_snapshot_document(raw_v4(Some(dns_json(serde_json::json!(mode))), None))
            .unwrap_or_else(|error| panic!("mode {mode:?} should parse: {error}"));
        assert_eq!(parsed.dns.unwrap()[0].mode.as_deref(), Some(mode));
    }
}

#[test]
fn dns_mode_absent_and_null_mean_not_observed() {
    let absent = parse_snapshot_document(raw_v4(
        Some(serde_json::json!([{
            "scope": { "kind": "global" },
            "servers": [],
            "searchDomains": [],
            "routingDomains": [],
            "sources": ["systemdResolved"],
            "notes": []
        }])),
        None,
    ))
    .unwrap();
    assert_eq!(absent.dns.unwrap()[0].mode, None);

    let null =
        parse_snapshot_document(raw_v4(Some(dns_json(serde_json::Value::Null)), None)).unwrap();
    assert_eq!(null.dns.unwrap()[0].mode, None);
}

#[test]
fn dns_mode_empty_or_controlled_text_is_rejected() {
    for mode in [
        "",
        "bad mode",
        "org.freedesktop.DBus.Error.AccessDenied: no such file",
        "bad\nmode",
    ] {
        let error = parse_snapshot_document(raw_v4(Some(dns_json(serde_json::json!(mode))), None))
            .expect_err("malformed DNS mode must be rejected");
        assert!(
            error.contains("$.dns[0].mode"),
            "mode {mode:?}: unexpected error {error}"
        );
    }
}

#[test]
fn dns_mode_never_reaches_provider_notes() {
    let note = "org.freedesktop.DBus.Error.AccessDenied: /org/freedesktop/resolve1/link/_32";
    let report = observation(
        ProviderId::SystemdResolved,
        2,
        SectionResult::Observed(vec![dns(
            0,
            "192.0.2.1",
            ObservationSource::SystemdResolved,
        )]),
        SectionResult::Unsupported,
    )
    .summarize();
    let mut snapshot = NetworkSnapshot {
        schema_version: SNAPSHOT_SCHEMA_VERSION,
        timestamp_ms: 0,
        interfaces: vec![],
        addresses: vec![],
        routes: vec![],
        providers: vec![report],
        dns: None,
        connections: None,
    };
    snapshot.providers[0].notes = vec![note.into()];
    let error = snapshot.validate_v4_reports().unwrap_err();
    assert!(error.contains("permitted static note"));
}

// ── D. Ownership and empty-section semantics ────────────────────────────────

#[test]
fn baseline_dns_unsupported_does_not_create_a_dns_collection() {
    let baseline = observation(
        ProviderId::Netlink,
        0,
        SectionResult::Unsupported,
        SectionResult::Unsupported,
    );
    let snapshot = merge(vec![baseline], 1);
    assert!(snapshot.dns.is_none());
}

#[test]
fn dns_owner_observed_empty_is_some_empty() {
    let owner = observation(
        ProviderId::SystemdResolved,
        2,
        SectionResult::Observed(vec![]),
        SectionResult::Unsupported,
    );
    let snapshot = merge(vec![owner], 1);
    assert_eq!(snapshot.dns, Some(vec![]));
}

#[test]
fn dns_owner_failure_or_denial_is_none_not_stale_data() {
    for outcome in [
        SectionResult::Failed("timeout".into()),
        SectionResult::PermissionDenied,
        SectionResult::Unavailable,
        SectionResult::NotObserved,
    ] {
        let owner = observation(
            ProviderId::SystemdResolved,
            2,
            outcome,
            SectionResult::Unsupported,
        );
        let snapshot = merge(vec![owner], 1);
        assert!(snapshot.dns.is_none());
    }
}

#[test]
fn failed_optional_refresh_removes_old_dns_from_the_current_document() {
    let before = snapshot_with_dns(Some(vec![dns(
        0,
        "192.0.2.1",
        ObservationSource::SystemdResolved,
    )]));
    let failed = observation(
        ProviderId::SystemdResolved,
        2,
        SectionResult::Failed("timeout".into()),
        SectionResult::Unsupported,
    );
    let after = merge(vec![failed], 1);
    assert!(after.dns.is_none());

    let events = diff_snapshots(&before, &after);
    assert!(
        events
            .iter()
            .any(|event| matches!(event, NetworkEvent::DnsRemoved { .. })),
        "events: {events:?}"
    );
}

#[test]
fn all_expected_owners_present_merge_their_sections() {
    let resolved = observation(
        ProviderId::SystemdResolved,
        2,
        SectionResult::Observed(vec![dns(
            0,
            "192.0.2.1",
            ObservationSource::SystemdResolved,
        )]),
        SectionResult::Unsupported,
    );
    let nm = observation(
        ProviderId::NetworkManager,
        2,
        SectionResult::Unsupported,
        SectionResult::Observed(vec![connection(ConnectionState::Activated, &[2])]),
    );
    let snapshot = merge(vec![nm, resolved], 1);
    assert_eq!(snapshot.dns.as_ref().unwrap().len(), 1);
    assert_eq!(snapshot.connections.as_ref().unwrap().len(), 1);
    assert!(snapshot
        .providers
        .iter()
        .all(|report| report.sections.len() == 7));
}

#[test]
fn absent_and_observed_empty_collections_remain_distinct() {
    let absent = parse_snapshot_document(raw_v4(None, None)).unwrap();
    assert!(absent.dns.is_none());
    assert!(absent.connections.is_none());

    let empty = parse_snapshot_document(raw_v4(
        Some(serde_json::json!([])),
        Some(serde_json::json!([])),
    ))
    .unwrap();
    assert_eq!(empty.dns, Some(vec![]));
    assert_eq!(empty.connections, Some(vec![]));
}

// ── E. Ownership, provenance, and malformed input ───────────────────────────

#[test]
fn network_manager_cannot_populate_dns() {
    let mut report = observation(
        ProviderId::NetworkManager,
        2,
        SectionResult::Observed(vec![dns(0, "192.0.2.1", ObservationSource::NetworkManager)]),
        SectionResult::Unsupported,
    )
    .summarize();
    report.notes = vec![];
    let snapshot = NetworkSnapshot {
        schema_version: SNAPSHOT_SCHEMA_VERSION,
        timestamp_ms: 0,
        interfaces: vec![],
        addresses: vec![],
        routes: vec![],
        providers: vec![report],
        dns: None,
        connections: None,
    };
    assert!(snapshot.validate_v4_reports().is_err());
}

#[test]
fn resolved_cannot_populate_connections() {
    let report = observation(
        ProviderId::SystemdResolved,
        2,
        SectionResult::Unsupported,
        SectionResult::Observed(vec![connection(ConnectionState::Activated, &[2])]),
    )
    .summarize();
    let snapshot = NetworkSnapshot {
        schema_version: SNAPSHOT_SCHEMA_VERSION,
        timestamp_ms: 0,
        interfaces: vec![],
        addresses: vec![],
        routes: vec![],
        providers: vec![report],
        dns: None,
        connections: None,
    };
    assert!(snapshot.validate_v4_reports().is_err());
}

#[test]
fn unknown_provider_notes_are_preserved_for_forward_compatibility() {
    let mut report = observation(
        ProviderId::Other("futureProvider".into()),
        9,
        SectionResult::Observed(vec![]),
        SectionResult::Observed(vec![]),
    )
    .summarize();
    report.notes = vec!["future provider note".into()];
    let snapshot = NetworkSnapshot {
        schema_version: SNAPSHOT_SCHEMA_VERSION,
        timestamp_ms: 0,
        interfaces: vec![],
        addresses: vec![],
        routes: vec![],
        providers: vec![report],
        dns: None,
        connections: None,
    };
    assert!(snapshot.validate_v4_reports().is_ok());
}

#[test]
fn dns_link_scope_zero_is_rejected() {
    let mut observation = dns(0, "192.0.2.1", ObservationSource::SystemdResolved);
    observation.scope = DnsScope::Link { ifindex: 0 };
    let snapshot = snapshot_with_dns(Some(vec![observation]));
    let error = snapshot.validate_v4_reports().unwrap_err();
    assert!(error.contains("scope.ifindex"));
}

#[test]
fn connection_devices_must_be_nonzero_sorted_and_unique() {
    for devices in [vec![0], vec![2, 1], vec![2, 2]] {
        let snapshot =
            snapshot_with_connections(Some(vec![connection(ConnectionState::Activated, &devices)]));
        let error = snapshot.validate_v4_reports().unwrap_err();
        assert!(
            error.contains("devices"),
            "devices {devices:?}: unexpected error {error}"
        );
    }
}

#[test]
fn empty_connection_type_is_rejected() {
    let mut item = connection(ConnectionState::Activated, &[2]);
    item.connection_type.clear();
    let snapshot = snapshot_with_connections(Some(vec![item]));
    let error = snapshot.validate_v4_reports().unwrap_err();
    assert!(error.contains("connectionType"));
}

#[test]
fn dns_and_connection_provenance_must_not_be_empty() {
    let mut item = dns(0, "192.0.2.1", ObservationSource::SystemdResolved);
    item.sources.clear();
    let snapshot = snapshot_with_dns(Some(vec![item]));
    assert!(snapshot
        .validate_provenance()
        .unwrap_err()
        .contains("$.dns[0].sources"));

    let mut item = connection(ConnectionState::Activated, &[2]);
    item.sources.clear();
    let snapshot = snapshot_with_connections(Some(vec![item]));
    assert!(snapshot
        .validate_provenance()
        .unwrap_err()
        .contains("$.connections[0].sources"));
}

#[test]
fn empty_unknown_identity_strings_are_rejected_in_json() {
    assert!(serde_json::from_value::<ProviderId>(serde_json::json!("")).is_err());
    let space: ProviderId = serde_json::from_value(serde_json::json!(" ")).unwrap();
    assert_eq!(space, ProviderId::Other(" ".into()));

    let raw = raw_v4(None, None);
    let mut raw = raw;
    raw["providers"] = serde_json::json!([{
        "provider": "",
        "state": "available",
        "sections": [],
        "notes": []
    }]);
    assert!(parse_snapshot_document(raw).is_err());
}

#[test]
fn merge_is_deterministic_for_equal_inputs() {
    let a = observation(
        ProviderId::SystemdResolved,
        2,
        SectionResult::Observed(vec![
            dns(2, "192.0.2.2", ObservationSource::SystemdResolved),
            dns(0, "192.0.2.1", ObservationSource::SystemdResolved),
        ]),
        SectionResult::Unsupported,
    );
    let b = observation(
        ProviderId::NetworkManager,
        2,
        SectionResult::Unsupported,
        SectionResult::Observed(vec![
            connection(ConnectionState::Activated, &[2]),
            connection(ConnectionState::Activated, &[2]),
        ]),
    );

    let first = merge(vec![a.clone(), b.clone()], 1);
    let second = merge(vec![b, a], 1);
    assert_eq!(first, second);
    assert_eq!(
        serde_json::to_string(&first).unwrap(),
        serde_json::to_string(&second).unwrap()
    );
}

#[test]
fn merge_preserves_duplicate_connections_and_dns() {
    let dns_item = dns(0, "192.0.2.1", ObservationSource::SystemdResolved);
    let connection_item = connection(ConnectionState::Activated, &[2]);
    let resolved = observation(
        ProviderId::SystemdResolved,
        2,
        SectionResult::Observed(vec![dns_item.clone(), dns_item]),
        SectionResult::Unsupported,
    );
    let nm = observation(
        ProviderId::NetworkManager,
        2,
        SectionResult::Unsupported,
        SectionResult::Observed(vec![connection_item.clone(), connection_item]),
    );

    let snapshot = merge(vec![resolved, nm], 1);
    assert_eq!(snapshot.dns.as_ref().unwrap().len(), 2);
    assert_eq!(snapshot.connections.as_ref().unwrap().len(), 2);
}

#[test]
fn topology_ignores_dns_and_connections_even_with_duplicates() {
    let mut snapshot = NetworkSnapshot {
        schema_version: SNAPSHOT_SCHEMA_VERSION,
        timestamp_ms: 0,
        interfaces: vec![Interface {
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
        }],
        addresses: vec![],
        routes: vec![],
        providers: vec![],
        dns: None,
        connections: None,
    };
    snapshot.dns = Some(vec![
        dns(2, "192.0.2.1", ObservationSource::SystemdResolved),
        dns(2, "192.0.2.1", ObservationSource::SystemdResolved),
    ]);
    snapshot.connections = Some(vec![
        connection(ConnectionState::Activated, &[2]),
        connection(ConnectionState::Activated, &[2]),
    ]);

    let topology = araiguma_core::build_topology(&snapshot);
    assert_eq!(topology.nodes.len(), 1);
    assert!(topology.edges.is_empty());
}

#[test]
fn dns_and_connection_notes_are_bounded_for_known_sources() {
    let mut dns_item = dns(0, "192.0.2.1", ObservationSource::SystemdResolved);
    dns_item.notes = vec!["raw /org/freedesktop/resolve1/link/_32".into()];
    let dns_snapshot = snapshot_with_dns(Some(vec![dns_item]));
    assert!(dns_snapshot
        .validate_v4_reports()
        .unwrap_err()
        .contains("permitted static note"));

    let mut connection_item = connection(ConnectionState::Activated, &[2]);
    connection_item.notes = vec!["raw /org/freedesktop/NetworkManager/Devices/3".into()];
    let connection_snapshot = snapshot_with_connections(Some(vec![connection_item]));
    assert!(connection_snapshot
        .validate_v4_reports()
        .unwrap_err()
        .contains("permitted static note"));
}

#[test]
fn unknown_source_notes_are_preserved_for_forward_compatibility() {
    let mut dns_item = dns(
        0,
        "192.0.2.1",
        ObservationSource::Other("futureResolver".into()),
    );
    dns_item.notes = vec!["future resolver note".into()];
    let snapshot = snapshot_with_dns(Some(vec![dns_item]));
    assert!(snapshot.validate_v4_reports().is_ok());
}
