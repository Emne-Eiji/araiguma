//! WO-07 startup snapshot validation regression tests.
//!
//! These tests drive the public startup path — registry construction, the
//! baseline-only opening snapshot, the document boundary, and topology
//! projection — and assert that the initial document is valid before any
//! optional provider has been observed.

use std::collections::VecDeque;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use araiguma_core::model::{
    ConnectionObservation, ConnectionState, DnsObservation, DnsScope, DnsServer, ObservationSource,
};
use araiguma_core::provider::{
    DiscoveryProvider, ProviderId, ProviderObservation, ProviderState, SectionKind, SectionOutcome,
    SectionResult,
};
use araiguma_core::{
    build_topology, parse_snapshot_document, NetworkSnapshot, ProviderReport, TopologyDocument,
};
use araiguma_engine::{Engine, EnvelopeKind, ProviderRegistry};

struct BaselineProvider {
    id: ProviderId,
    priority: u8,
}

#[async_trait::async_trait]
impl DiscoveryProvider for BaselineProvider {
    fn id(&self) -> ProviderId {
        self.id.clone()
    }

    fn priority(&self) -> u8 {
        self.priority
    }

    async fn observe(&mut self) -> ProviderObservation {
        ProviderObservation {
            provider: self.id.clone(),
            priority: self.priority,
            interfaces: SectionResult::Observed(Vec::new()),
            addresses_v4: SectionResult::Observed(Vec::new()),
            addresses_v6: SectionResult::Observed(Vec::new()),
            routes_v4: SectionResult::Observed(Vec::new()),
            routes_v6: SectionResult::Observed(Vec::new()),
            dns: SectionResult::Unsupported,
            connections: SectionResult::Unsupported,
            fidelity_limited: false,
            notes: Vec::new(),
        }
    }
}

struct ScriptedDns {
    results: Arc<Mutex<VecDeque<SectionResult<DnsObservation>>>>,
    observes: Arc<AtomicUsize>,
}

impl ScriptedDns {
    fn new(results: Vec<SectionResult<DnsObservation>>) -> (Self, Arc<AtomicUsize>) {
        let observes = Arc::new(AtomicUsize::new(0));
        (
            Self {
                results: Arc::new(Mutex::new(results.into())),
                observes: Arc::clone(&observes),
            },
            observes,
        )
    }
}

#[async_trait::async_trait]
impl DiscoveryProvider for ScriptedDns {
    fn id(&self) -> ProviderId {
        ProviderId::SystemdResolved
    }

    fn priority(&self) -> u8 {
        3
    }

    async fn observe(&mut self) -> ProviderObservation {
        self.observes.fetch_add(1, Ordering::SeqCst);
        let result = self
            .results
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .pop_front()
            .unwrap_or(SectionResult::NotObserved);
        ProviderObservation {
            provider: ProviderId::SystemdResolved,
            priority: 3,
            interfaces: SectionResult::Unsupported,
            addresses_v4: SectionResult::Unsupported,
            addresses_v6: SectionResult::Unsupported,
            routes_v4: SectionResult::Unsupported,
            routes_v6: SectionResult::Unsupported,
            dns: result,
            connections: SectionResult::Unsupported,
            fidelity_limited: false,
            notes: Vec::new(),
        }
    }
}

struct ScriptedConnections {
    results: Arc<Mutex<VecDeque<SectionResult<ConnectionObservation>>>>,
    observes: Arc<AtomicUsize>,
}

impl ScriptedConnections {
    fn new(results: Vec<SectionResult<ConnectionObservation>>) -> (Self, Arc<AtomicUsize>) {
        let observes = Arc::new(AtomicUsize::new(0));
        (
            Self {
                results: Arc::new(Mutex::new(results.into())),
                observes: Arc::clone(&observes),
            },
            observes,
        )
    }
}

#[async_trait::async_trait]
impl DiscoveryProvider for ScriptedConnections {
    fn id(&self) -> ProviderId {
        ProviderId::NetworkManager
    }

    fn priority(&self) -> u8 {
        2
    }

    async fn observe(&mut self) -> ProviderObservation {
        self.observes.fetch_add(1, Ordering::SeqCst);
        let result = self
            .results
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .pop_front()
            .unwrap_or(SectionResult::NotObserved);
        ProviderObservation {
            provider: ProviderId::NetworkManager,
            priority: 2,
            interfaces: SectionResult::Unsupported,
            addresses_v4: SectionResult::Unsupported,
            addresses_v6: SectionResult::Unsupported,
            routes_v4: SectionResult::Unsupported,
            routes_v6: SectionResult::Unsupported,
            dns: SectionResult::Unsupported,
            connections: result,
            fidelity_limited: false,
            notes: Vec::new(),
        }
    }
}

struct Observes {
    dns: Arc<AtomicUsize>,
    connections: Arc<AtomicUsize>,
}

fn engine_with(
    dns_results: Vec<SectionResult<DnsObservation>>,
    connection_results: Vec<SectionResult<ConnectionObservation>>,
) -> (Engine, Observes) {
    let (dns, dns_observes) = ScriptedDns::new(dns_results);
    let (connections, connection_observes) = ScriptedConnections::new(connection_results);

    let mut registry = ProviderRegistry::new();
    registry
        .register(Box::new(BaselineProvider {
            id: ProviderId::Netlink,
            priority: 0,
        }))
        .expect("netlink registration");
    registry
        .register(Box::new(BaselineProvider {
            id: ProviderId::SysFs,
            priority: 1,
        }))
        .expect("sysfs registration");
    registry
        .register(Box::new(connections))
        .expect("NetworkManager registration");
    registry
        .register(Box::new(dns))
        .expect("systemd-resolved registration");
    let engine = Engine::with_registry(registry).expect("unique provider identities");
    (
        engine,
        Observes {
            dns: dns_observes,
            connections: connection_observes,
        },
    )
}

fn report_for<'a>(snapshot: &'a NetworkSnapshot, provider: &ProviderId) -> &'a ProviderReport {
    snapshot
        .providers
        .iter()
        .find(|report| &report.provider == provider)
        .unwrap_or_else(|| panic!("missing provider report for {provider:?}"))
}

fn assert_non_owned_sections_unsupported(
    snapshot: &NetworkSnapshot,
    provider: ProviderId,
    owned: SectionKind,
) {
    let report = report_for(snapshot, &provider);
    for entry in &report.sections {
        if entry.section != owned {
            assert!(
                matches!(entry.outcome, SectionOutcome::Unsupported),
                "{provider:?} non-owned section must be Unsupported: {entry:?}"
            );
        }
    }
}

fn assert_placeholder_ownership(
    snapshot: &NetworkSnapshot,
    provider: ProviderId,
    owned: SectionKind,
) {
    let report = report_for(snapshot, &provider);
    assert_eq!(report.state, ProviderState::NotYetObserved);
    assert_non_owned_sections_unsupported(snapshot, provider.clone(), owned.clone());
    let entry = report
        .sections
        .iter()
        .find(|entry| entry.section == owned)
        .unwrap_or_else(|| panic!("missing owned section for {provider:?}"));
    assert!(
        matches!(entry.outcome, SectionOutcome::NotObserved),
        "{provider:?} owned section must remain NotObserved before its first observation: {entry:?}"
    );
}

fn parse_and_project(snapshot: &NetworkSnapshot) -> TopologyDocument {
    let value = serde_json::to_value(snapshot).expect("snapshot serializes");
    let parsed =
        parse_snapshot_document(value).expect("document boundary must accept the startup snapshot");
    let topology = build_topology(&parsed);
    assert_eq!(topology.source_timestamp_ms, snapshot.timestamp_ms);
    topology
}

fn non_empty_dns() -> DnsObservation {
    DnsObservation {
        scope: DnsScope::Global,
        mode: Some("stub".into()),
        servers: vec![DnsServer {
            address: "192.0.2.53".parse().expect("address"),
            port: None,
        }],
        search_domains: vec!["example.test".into()],
        routing_domains: Vec::new(),
        sources: vec![ObservationSource::SystemdResolved],
        notes: Vec::new(),
    }
}

fn connection() -> ConnectionObservation {
    ConnectionObservation {
        connection_type: "802-3-ethernet".into(),
        state: ConnectionState::Activated,
        devices: vec![2],
        vpn: false,
        sources: vec![ObservationSource::NetworkManager],
        notes: Vec::new(),
    }
}

#[tokio::test]
async fn initial_snapshot_validates_before_optional_poll() {
    let (mut engine, observes) = engine_with(
        vec![SectionResult::Observed(Vec::new())],
        vec![SectionResult::Observed(Vec::new())],
    );

    let document = engine.snapshot_document().await.expect("initial snapshot");

    assert_eq!(observes.dns.load(Ordering::SeqCst), 0);
    assert_eq!(observes.connections.load(Ordering::SeqCst), 0);
    document
        .validate_v4_reports()
        .expect("initial provider report must validate before any optional poll");
    assert_placeholder_ownership(
        &document,
        ProviderId::NetworkManager,
        SectionKind::Connections,
    );
    assert_placeholder_ownership(&document, ProviderId::SystemdResolved, SectionKind::Dns);
}

#[tokio::test]
async fn immediate_topology_retrieval_succeeds_without_optional_observe() {
    let (mut engine, observes) = engine_with(
        vec![SectionResult::Observed(Vec::new())],
        vec![SectionResult::Observed(Vec::new())],
    );

    let document = engine.snapshot_document().await.expect("initial snapshot");
    let topology = parse_and_project(&document);

    assert_eq!(observes.dns.load(Ordering::SeqCst), 0);
    assert_eq!(observes.connections.load(Ordering::SeqCst), 0);
    assert_eq!(topology.source_timestamp_ms, document.timestamp_ms);
}

#[tokio::test]
async fn snapshot_and_topology_paths_agree_on_initial_ownership() {
    let (mut engine, _observes) = engine_with(
        vec![SectionResult::Observed(Vec::new())],
        vec![SectionResult::Observed(Vec::new())],
    );

    let document = engine.snapshot_document().await.expect("initial snapshot");
    document
        .validate_v4_reports()
        .expect("snapshot path must accept the initial report");

    let value = serde_json::to_value(&document).expect("snapshot serializes");
    let parsed = parse_snapshot_document(value).expect("topology path must accept the same report");
    assert_eq!(parsed.providers, document.providers);

    let topology = build_topology(&parsed);
    assert_eq!(topology.source_timestamp_ms, document.timestamp_ms);
}

#[tokio::test]
async fn lifecycle_transition_from_placeholder_to_observed_stays_valid() {
    let (mut engine, observes) = engine_with(
        vec![SectionResult::Observed(Vec::new())],
        vec![SectionResult::Observed(Vec::new())],
    );

    let initial = engine.snapshot_document().await.expect("initial snapshot");
    initial
        .validate_v4_reports()
        .expect("initial report must validate");
    assert!(initial.dns.is_none());
    assert!(initial.connections.is_none());
    let _ = parse_and_project(&initial);
    assert_eq!(observes.dns.load(Ordering::SeqCst), 0);
    assert_eq!(observes.connections.load(Ordering::SeqCst), 0);

    let envelope = engine
        .refresh_optional()
        .await
        .expect("first optional refresh changes the document");
    assert_eq!(envelope.event, EnvelopeKind::Changed);
    assert_eq!(observes.dns.load(Ordering::SeqCst), 1);
    assert_eq!(observes.connections.load(Ordering::SeqCst), 1);

    let current = engine.current_document().expect("document").clone();
    current
        .validate_v4_reports()
        .expect("post-refresh report must validate");
    assert_eq!(current.dns, Some(Vec::new()));
    assert_eq!(current.connections, Some(Vec::new()));
    assert_eq!(
        report_for(&current, &ProviderId::NetworkManager).state,
        ProviderState::Available
    );
    assert_eq!(
        report_for(&current, &ProviderId::SystemdResolved).state,
        ProviderState::Available
    );
    let _ = parse_and_project(&current);
}

#[tokio::test]
async fn optional_outcomes_preserve_ownership_and_distinct_states() {
    let (mut engine, _observes) = engine_with(
        vec![
            SectionResult::Failed("timeout".into()),
            SectionResult::Observed(Vec::new()),
            SectionResult::Observed(vec![non_empty_dns()]),
        ],
        vec![
            SectionResult::Observed(vec![connection()]),
            SectionResult::Unavailable,
            SectionResult::Failed("timeout".into()),
        ],
    );

    let initial = engine.snapshot_document().await.expect("initial snapshot");
    assert_placeholder_ownership(
        &initial,
        ProviderId::NetworkManager,
        SectionKind::Connections,
    );
    assert_placeholder_ownership(&initial, ProviderId::SystemdResolved, SectionKind::Dns);

    engine
        .refresh_optional()
        .await
        .expect("step 1 changes the document");
    let step1 = engine.current_document().expect("document").clone();
    step1.validate_v4_reports().expect("step 1 report");
    assert_eq!(step1.connections.as_ref().map(Vec::len), Some(1));
    assert!(step1.dns.is_none());
    assert_eq!(
        report_for(&step1, &ProviderId::NetworkManager).state,
        ProviderState::Available
    );
    assert_eq!(
        report_for(&step1, &ProviderId::SystemdResolved).state,
        ProviderState::Degraded
    );
    assert_non_owned_sections_unsupported(
        &step1,
        ProviderId::NetworkManager,
        SectionKind::Connections,
    );
    assert_non_owned_sections_unsupported(&step1, ProviderId::SystemdResolved, SectionKind::Dns);

    engine
        .refresh_optional()
        .await
        .expect("step 2 changes the document");
    let step2 = engine.current_document().expect("document").clone();
    step2.validate_v4_reports().expect("step 2 report");
    assert!(step2.connections.is_none());
    assert_eq!(step2.dns, Some(Vec::new()));
    assert_eq!(
        report_for(&step2, &ProviderId::NetworkManager).state,
        ProviderState::Unavailable
    );
    assert_eq!(
        report_for(&step2, &ProviderId::SystemdResolved).state,
        ProviderState::Available
    );

    engine
        .refresh_optional()
        .await
        .expect("step 3 changes the document");
    let step3 = engine.current_document().expect("document").clone();
    step3.validate_v4_reports().expect("step 3 report");
    assert!(step3.connections.is_none());
    assert_eq!(step3.dns.as_ref().map(Vec::len), Some(1));
    assert_eq!(
        report_for(&step3, &ProviderId::NetworkManager).state,
        ProviderState::Degraded
    );
    assert_eq!(
        report_for(&step3, &ProviderId::SystemdResolved).state,
        ProviderState::Available
    );
    let _ = parse_and_project(&step3);
}
