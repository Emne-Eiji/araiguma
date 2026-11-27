//! The shared observation engine (document 01 §4.1).
//!
//! Ownership: provider lifecycle (construct, observe), the current
//! validated document, envelope construction and sequence numbering, and
//! the subscriber registry. The CLI and the desktop shell are
//! interchangeable consumers — one engine, two transports.
//!
//! The change FEED (the signal source) is owned by the consumer, not by
//! the engine: see [`crate::change_feed`]. A consumer waits for signals and
//! calls [`Engine::apply_change`] / [`Engine::apply_resync`] with the
//! result. That split is deliberate — the desktop shell must be able to
//! answer IPC commands (`get_snapshot`, `subscribe_watch`) while it waits
//! for the next network change, so the engine lock is never held across a
//! blocking wait for a signal.

use std::time::{Duration, SystemTime, UNIX_EPOCH};

use futures::channel::mpsc::{unbounded, UnboundedReceiver, UnboundedSender};
use serde::{Deserialize, Serialize};

use araiguma_core::provider::{
    DiscoveryProvider, ProviderId, ProviderObservation, SectionKind, SectionResult,
};
use araiguma_core::snapshot::NetworkEvent;
use araiguma_core::{diff_snapshots, merge, NetworkSnapshot};
use araiguma_netlink::NetlinkProvider;
use araiguma_provider_nm::NetworkManagerProvider;
use araiguma_provider_resolved::SystemdResolvedProvider;
use araiguma_sysfs::SysFsProvider;

/// Burst-collapse window — identical to the CLI watch loop window
/// (document 01 §4.1).
pub const DEBOUNCE: Duration = Duration::from_millis(150);

/// Default optional-provider polling cadence (RD-08: 30 s, configurable).
///
/// Optional providers are never observed during the opening snapshot. The
/// CLI watch loop and desktop driver poll them on this cadence after the
/// baseline document is available.
pub const OPTIONAL_POLL_INTERVAL: Duration = Duration::from_secs(30);

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// Event envelope kind (document 04 §7).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum EnvelopeKind {
    Snapshot,
    Changed,
    Resynchronized,
    MonitorHealth,
}

impl EnvelopeKind {
    /// The wire name of the envelope kind.
    pub fn as_str(&self) -> &'static str {
        match self {
            EnvelopeKind::Snapshot => "snapshot",
            EnvelopeKind::Changed => "changed",
            EnvelopeKind::Resynchronized => "resynchronized",
            EnvelopeKind::MonitorHealth => "monitorHealth",
        }
    }
}

/// The health of the process-wide event monitor, as observed by the
/// desktop transport. This is deliberately limited to the one correctness
/// signal needed by live mode: whether netlink event delivery is reliable.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum MonitorHealthState {
    /// The event stream is not reliable. A confirmed observation may remain
    /// visible, but it is stale; with no confirmed observation the live
    /// state is unavailable.
    Degraded,
    /// The event stream was re-established and a fresh observation has been
    /// constructed. Live mode may return to LIVE after that observation is
    /// rendered.
    Recovered,
}

/// Monitor-health payload carried by an IPC [`Envelope`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MonitorHealth {
    pub state: MonitorHealthState,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

/// One event envelope (document 04 §7). Fields not applicable to the kind
/// are omitted from the JSON.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Envelope {
    pub event: EnvelopeKind,
    /// Monotonic per engine session, gap-free, in-order delivery (document
    /// 04 §8). Present on the IPC contract; the CLI NDJSON contract does
    /// not carry it (see [`Envelope::to_cli_json_string`]).
    pub seq: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub snapshot: Option<NetworkSnapshot>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub changes: Option<Vec<NetworkEvent>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notified_by: Option<Vec<String>>,
    /// Present only for a `monitorHealth` envelope. The CLI never emits
    /// this IPC control signal; the desktop forwards it to live mode.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub monitor: Option<MonitorHealth>,
}

impl Envelope {
    pub fn snapshot(seq: u64, document: NetworkSnapshot) -> Envelope {
        Envelope {
            event: EnvelopeKind::Snapshot,
            seq,
            snapshot: Some(document),
            changes: None,
            notified_by: None,
            monitor: None,
        }
    }

    pub fn changed(seq: u64, notified_by: Vec<String>, changes: Vec<NetworkEvent>) -> Envelope {
        Envelope {
            event: EnvelopeKind::Changed,
            seq,
            snapshot: None,
            changes: Some(changes),
            notified_by: Some(notified_by),
            monitor: None,
        }
    }

    pub fn resynchronized(
        seq: u64,
        changes: Vec<NetworkEvent>,
        document: NetworkSnapshot,
    ) -> Envelope {
        Envelope {
            event: EnvelopeKind::Resynchronized,
            seq,
            snapshot: Some(document),
            changes: Some(changes),
            notified_by: None,
            monitor: None,
        }
    }

    /// Construct the desktop-only monitor-health control envelope.
    pub fn monitor_health(seq: u64, state: MonitorHealthState, reason: Option<String>) -> Envelope {
        Envelope {
            event: EnvelopeKind::MonitorHealth,
            seq,
            snapshot: None,
            changes: None,
            notified_by: None,
            monitor: Some(MonitorHealth { state, reason }),
        }
    }

    /// The CLI NDJSON form — the existing CLI watch contract, unchanged:
    /// the same three envelopes without the IPC-only `seq` field. The CLI
    /// output is a published contract; the sequence number belongs to the
    /// IPC continuity rules (document 04 §8), not to the CLI stream.
    pub fn to_cli_json_string(&self) -> String {
        match self.event {
            EnvelopeKind::Snapshot => serde_json::json!({
                "event": "snapshot",
                "snapshot": self.snapshot.clone(),
            })
            .to_string(),
            EnvelopeKind::Changed => serde_json::json!({
                "event": "changed",
                "notifiedBy": self.notified_by.clone(),
                "changes": self.changes.clone(),
            })
            .to_string(),
            EnvelopeKind::Resynchronized => serde_json::json!({
                "event": "resynchronized",
                "changes": self.changes.clone(),
                "snapshot": self.snapshot.clone(),
            })
            .to_string(),
            // The CLI watch loop never creates or forwards this desktop IPC
            // control signal. Keep the no-`seq` rule if it is ever rendered
            // through this helper, but do not add it to the CLI's three-event
            // observable stream.
            EnvelopeKind::MonitorHealth => serde_json::json!({
                "event": "monitorHealth",
                "monitor": self.monitor.clone(),
            })
            .to_string(),
        }
    }
}

/// A deterministic provider registry.
///
/// Registration is side-effect-free: no provider is constructed or observed
/// here. Duplicate provider identities are rejected. Providers are ordered
/// by `(priority, identity)` so observation and report order are stable.
pub struct ProviderRegistry {
    providers: Vec<(ProviderId, Box<dyn DiscoveryProvider>)>,
}

impl ProviderRegistry {
    pub fn new() -> Self {
        Self {
            providers: Vec::new(),
        }
    }

    /// Register one provider. The provider's own `id()` is the identity.
    pub fn register(&mut self, provider: Box<dyn DiscoveryProvider>) -> Result<(), String> {
        let id = provider.id();
        if self.providers.iter().any(|(existing, _)| *existing == id) {
            return Err(format!(
                "duplicateProvider: provider identity {id:?} is already registered"
            ));
        }
        self.providers.push((id, provider));
        Ok(())
    }

    /// Provider identities in deterministic order.
    pub fn ids(&self) -> Vec<ProviderId> {
        let mut ids: Vec<ProviderId> = self.providers.iter().map(|(id, _)| id.clone()).collect();
        ids.sort();
        ids
    }

    fn into_providers(mut self) -> Vec<Box<dyn DiscoveryProvider>> {
        self.providers
            .sort_by(|(left_id, left), (right_id, right)| {
                left.priority()
                    .cmp(&right.priority())
                    .then_with(|| left_id.cmp(right_id))
            });
        self.providers
            .into_iter()
            .map(|(_, provider)| provider)
            .collect()
    }
}

impl Default for ProviderRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// The shared observation engine.
pub struct Engine {
    providers: Vec<Box<dyn DiscoveryProvider>>,
    optional_indices: Vec<usize>,
    last_optional: Vec<Option<ProviderObservation>>,
    baseline_observations: Vec<ProviderObservation>,
    document: Option<NetworkSnapshot>,
    seq: u64,
    subscriptions: Vec<(String, UnboundedSender<Envelope>)>,
    subscription_counter: u64,
    stopped: bool,
}

impl Engine {
    pub fn new(providers: Vec<Box<dyn DiscoveryProvider>>) -> Engine {
        let optional_indices = providers
            .iter()
            .enumerate()
            .filter_map(|(index, provider)| is_optional(provider.id()).then_some(index))
            .collect();
        let last_optional = vec![None; providers.len()];
        Engine {
            providers,
            optional_indices,
            last_optional,
            baseline_observations: Vec::new(),
            document: None,
            seq: 0,
            subscriptions: Vec::new(),
            subscription_counter: 0,
            stopped: false,
        }
    }

    /// Build an engine from a registry, rejecting duplicate identities.
    pub fn with_registry(registry: ProviderRegistry) -> Result<Engine, String> {
        Ok(Engine::new(registry.into_providers()))
    }

    /// The default provider set: baseline netlink/sysfs plus the optional
    /// NetworkManager and systemd-resolved providers. Construction performs
    /// no D-Bus I/O; optional providers are first observed only by the
    /// optional refresh loop.
    pub fn default_providers() -> Engine {
        let mut registry = ProviderRegistry::new();
        registry
            .register(Box::new(NetlinkProvider::new()))
            .expect("default netlink registration");
        registry
            .register(Box::new(SysFsProvider::new()))
            .expect("default sysfs registration");
        registry
            .register(Box::new(NetworkManagerProvider::new()))
            .expect("default NetworkManager registration");
        registry
            .register(Box::new(SystemdResolvedProvider::new()))
            .expect("default resolved registration");
        Engine::with_registry(registry).expect("default registry has unique identities")
    }

    /// Provider identities in deterministic order.
    pub fn provider_ids(&self) -> Vec<ProviderId> {
        self.providers
            .iter()
            .map(|provider| provider.id())
            .collect()
    }

    /// Run one full observation pass, including optional providers.
    ///
    /// This is an explicit full refresh. Normal snapshot/change/resync paths
    /// use the baseline plus the latest optional observations so optional
    /// providers never block baseline observation.
    pub async fn observe_now(&mut self) -> NetworkSnapshot {
        let mut observations = Vec::new();
        let mut baseline = Vec::new();
        for (index, provider) in self.providers.iter_mut().enumerate() {
            let observation = provider.observe().await;
            if self.optional_indices.contains(&index) {
                self.last_optional[index] = Some(observation.clone());
            } else {
                baseline.push(observation.clone());
            }
            observations.push(observation);
        }
        self.baseline_observations = baseline;
        merge(observations, now_ms())
    }

    /// Observe the baseline providers and merge them with the latest optional
    /// observations (or stable `NotYetObserved` placeholders).
    pub async fn observe_baseline_now(&mut self) -> NetworkSnapshot {
        let mut observations = Vec::new();
        let mut baseline = Vec::new();
        for (index, provider) in self.providers.iter_mut().enumerate() {
            if self.optional_indices.contains(&index) {
                observations.push(
                    self.last_optional[index]
                        .clone()
                        .unwrap_or_else(|| not_yet_observed(provider.as_ref())),
                );
            } else {
                let observation = provider.observe().await;
                baseline.push(observation.clone());
                observations.push(observation);
            }
        }
        self.baseline_observations = baseline;
        merge(observations, now_ms())
    }

    /// Refresh only the optional providers and merge the result with the
    /// stored baseline observations. Returns a `changed` envelope when the
    /// document changed, or `None` when it did not.
    pub async fn refresh_optional(&mut self) -> Option<Envelope> {
        if self.stopped {
            return None;
        }
        let optional_indices = self.optional_indices.clone();
        for index in optional_indices {
            let observation = self.providers[index].observe().await;
            self.last_optional[index] = Some(observation);
        }

        let fresh = self.merge_with_optional();
        if self.document.is_none() {
            self.document = Some(fresh.clone());
            let seq = self.next_seq();
            return Some(Envelope::snapshot(seq, fresh));
        }

        let changes = self.diff_to(&fresh);
        self.document = Some(fresh);
        if changes.is_empty() {
            None
        } else {
            let seq = self.next_seq();
            Some(Envelope::changed(
                seq,
                vec!["optionalRefresh".to_string()],
                changes,
            ))
        }
    }

    fn merge_with_optional(&self) -> NetworkSnapshot {
        let mut observations = self.baseline_observations.clone();
        for index in &self.optional_indices {
            observations.push(
                self.last_optional[*index]
                    .clone()
                    .unwrap_or_else(|| not_yet_observed(self.providers[*index].as_ref())),
            );
        }
        merge(observations, now_ms())
    }

    /// The current validated document; seeds with a baseline-only
    /// observation so optional providers never delay the opening snapshot.
    pub async fn snapshot_document(&mut self) -> Result<NetworkSnapshot, String> {
        if self.document.is_none() {
            let document = self.observe_baseline_now().await;
            self.document = Some(document.clone());
        }
        Ok(self
            .document
            .clone()
            .expect("engine seeds document on first call"))
    }

    pub fn current_document(&self) -> Option<&NetworkSnapshot> {
        self.document.as_ref()
    }

    /// Adopt an observation as the current document without producing a
    /// diff (the first observation of a session has no predecessor).
    pub fn seed(&mut self, document: NetworkSnapshot) {
        self.document = Some(document);
    }

    /// The next envelope sequence number (document 04 §8: monotonic per
    /// engine session, gap-free).
    pub fn next_seq(&mut self) -> u64 {
        self.seq += 1;
        self.seq
    }

    /// The current document as a `snapshot` envelope — the opening
    /// envelope of a session or of a new subscription.
    pub async fn snapshot_envelope(&mut self) -> Envelope {
        let document = self
            .snapshot_document()
            .await
            .expect("observation never fails the engine");
        let seq = self.next_seq();
        Envelope::snapshot(seq, document)
    }

    /// Observe now and return a `changed` envelope: a debounced pure diff
    /// against the current document (document 04 §7).
    pub async fn apply_change(&mut self, notified_by: Vec<String>) -> Envelope {
        let fresh = self.observe_baseline_now().await;
        let changes = self.diff_to(&fresh);
        self.document = Some(fresh.clone());
        let seq = self.next_seq();
        Envelope::changed(seq, notified_by, changes)
    }

    /// Observe now and return a `resynchronized` envelope: a monitoring gap
    /// occurred, changes during the gap were NOT observed, and the fresh
    /// snapshot is authoritative again (document 04 §8.3).
    pub async fn apply_resync(&mut self) -> Envelope {
        let fresh = self.observe_baseline_now().await;
        let changes = self.diff_to(&fresh);
        self.document = Some(fresh.clone());
        let seq = self.next_seq();
        Envelope::resynchronized(seq, changes, fresh)
    }

    /// Construct a monitor-health control envelope on the same global
    /// sequence stream as document events. The desktop broadcasts it to
    /// active subscribers; the CLI watch contract never emits it.
    pub fn monitor_health(
        &mut self,
        state: MonitorHealthState,
        reason: Option<String>,
    ) -> Envelope {
        let seq = self.next_seq();
        Envelope::monitor_health(seq, state, reason)
    }

    fn diff_to(&self, fresh: &NetworkSnapshot) -> Vec<NetworkEvent> {
        match &self.document {
            Some(previous) => diff_snapshots(previous, fresh),
            None => Vec::new(),
        }
    }

    // -- Subscriptions (document 04 §6 lifecycle) ---------------------------

    /// Register a new subscriber. The first envelope a subscriber sees is a
    /// `snapshot` envelope (see [`Engine::subscribe_and_snapshot`]).
    pub fn subscribe(&mut self) -> (String, UnboundedReceiver<Envelope>) {
        let (tx, rx) = unbounded();
        self.subscription_counter += 1;
        let id = format!("araiguma-sub-{}", self.subscription_counter);
        self.subscriptions.push((id.clone(), tx));
        (id, rx)
    }

    /// Subscribe and immediately deliver the current document as a
    /// `snapshot` envelope to the new subscriber's channel (document 04:
    /// "the first delivered event is always a `snapshot` envelope").
    ///
    /// The opening snapshot is broadcast on the engine session stream, not
    /// merely sent to the new subscriber. Its sequence number is therefore
    /// visible to every active subscriber: creating a subscription cannot
    /// consume a global sequence number that another subscriber misses.
    pub async fn subscribe_and_snapshot(
        &mut self,
    ) -> Result<(String, UnboundedReceiver<Envelope>), String> {
        if self.stopped {
            return Err(
                "shutdownInProgress: the Araiguma observation engine is shutting down".to_string(),
            );
        }
        let document = self.snapshot_document().await?;
        let (id, rx) = self.subscribe();
        let seq = self.next_seq();
        self.broadcast(&Envelope::snapshot(seq, document));
        Ok((id, rx))
    }

    /// Fan an envelope out to all live subscribers; drops closed channels.
    /// Returns the number of deliveries.
    pub fn broadcast(&mut self, event: &Envelope) -> usize {
        let mut delivered = 0;
        self.subscriptions
            .retain(|(_, tx)| match tx.unbounded_send(event.clone()) {
                Ok(()) => {
                    delivered += 1;
                    true
                }
                Err(_) => false,
            });
        delivered
    }

    pub fn subscriber_count(&self) -> usize {
        self.subscriptions.len()
    }

    pub fn unsubscribe(&mut self, subscription_id: &str) -> Result<(), String> {
        let before = self.subscriptions.len();
        self.subscriptions.retain(|(id, _)| id != subscription_id);
        if before == self.subscriptions.len() {
            Err(format!(
                "invalidRequest: unknown subscription id {subscription_id}"
            ))
        } else {
            Ok(())
        }
    }

    // -- Lifecycle ------------------------------------------------------------

    pub fn request_shutdown(&mut self) {
        self.stopped = true;
        // Closing every sender releases the consumer-side delivery tasks.
        // Stage 1 has no terminal `ended` control envelope; ending the
        // channel is the transport-level shutdown signal.
        self.subscriptions.clear();
    }

    pub fn is_shutdown(&self) -> bool {
        self.stopped
    }
}

fn is_optional(id: ProviderId) -> bool {
    matches!(id, ProviderId::NetworkManager | ProviderId::SystemdResolved)
}

fn not_yet_observed(provider: &dyn DiscoveryProvider) -> ProviderObservation {
    let id = provider.id();
    let owned = id.owned_optional_section();
    // The placeholder must preserve the report ownership matrix: the
    // section this optional provider owns is not observed yet, while every
    // section it does not own is unsupported. Reporting `NotObserved` for a
    // non-owned section conflates "not attempted" with "not ours" and makes
    // the opening document invalid at the v4 report boundary (WO-07).
    ProviderObservation {
        provider: id,
        priority: provider.priority(),
        interfaces: placeholder_section(owned.as_ref(), SectionKind::Interfaces),
        addresses_v4: placeholder_section(owned.as_ref(), SectionKind::AddressesV4),
        addresses_v6: placeholder_section(owned.as_ref(), SectionKind::AddressesV6),
        routes_v4: placeholder_section(owned.as_ref(), SectionKind::RoutesV4),
        routes_v6: placeholder_section(owned.as_ref(), SectionKind::RoutesV6),
        dns: placeholder_section(owned.as_ref(), SectionKind::Dns),
        connections: placeholder_section(owned.as_ref(), SectionKind::Connections),
        fidelity_limited: false,
        notes: Vec::new(),
    }
}

fn placeholder_section<T>(owned: Option<&SectionKind>, section: SectionKind) -> SectionResult<T> {
    match owned {
        // A provider that declares no owned section has nothing to report
        // yet. Keeping every section `NotObserved` preserves that meaning
        // and lets the report boundary reject the shape, instead of
        // silently claiming the provider is "not supported".
        None => SectionResult::NotObserved,
        Some(owned) if *owned == section => SectionResult::NotObserved,
        Some(_) => SectionResult::Unsupported,
    }
}

// ---------------------------------------------------------------------------
// Tests (engine unit tests, document 07 §3 and §6).
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::change_feed::{debounce, ChangeFeed, FakeFeed};
    use araiguma_core::model::{DnsObservation, DnsScope, ObservationSource};
    use araiguma_core::provider::{
        ProviderId, ProviderObservation, ProviderState, SectionKind, SectionOutcome, SectionResult,
    };
    use araiguma_netlink::Signal;
    use std::collections::VecDeque;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::{Arc, Mutex};
    use tokio::sync::Notify;

    fn empty_observation(provider: ProviderId) -> ProviderObservation {
        ProviderObservation {
            provider: provider.clone(),
            priority: if provider == ProviderId::Netlink {
                0
            } else {
                1
            },
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

    fn dns_observation(
        provider: ProviderId,
        priority: u8,
        result: SectionResult<DnsObservation>,
        fidelity_limited: bool,
    ) -> ProviderObservation {
        ProviderObservation {
            provider,
            priority,
            interfaces: SectionResult::Unsupported,
            addresses_v4: SectionResult::Unsupported,
            addresses_v6: SectionResult::Unsupported,
            routes_v4: SectionResult::Unsupported,
            routes_v6: SectionResult::Unsupported,
            dns: result,
            connections: SectionResult::Unsupported,
            fidelity_limited,
            notes: Vec::new(),
        }
    }

    fn empty_dns() -> DnsObservation {
        DnsObservation {
            scope: DnsScope::Global,
            mode: None,
            servers: Vec::new(),
            search_domains: Vec::new(),
            routing_domains: Vec::new(),
            sources: vec![ObservationSource::SystemdResolved],
            notes: Vec::new(),
        }
    }

    struct ScriptedOptional {
        id: ProviderId,
        priority: u8,
        results: Arc<Mutex<VecDeque<SectionResult<DnsObservation>>>>,
        gate: Option<Arc<Notify>>,
        fidelity_limited: bool,
        observes: Arc<AtomicUsize>,
    }

    impl ScriptedOptional {
        fn new(
            id: ProviderId,
            priority: u8,
            results: Vec<SectionResult<DnsObservation>>,
        ) -> (Self, Arc<AtomicUsize>) {
            let observes = Arc::new(AtomicUsize::new(0));
            (
                Self {
                    id,
                    priority,
                    results: Arc::new(Mutex::new(results.into())),
                    gate: None,
                    fidelity_limited: false,
                    observes: Arc::clone(&observes),
                },
                observes,
            )
        }

        fn gated(mut self, gate: Arc<Notify>) -> Self {
            self.gate = Some(gate);
            self
        }

        fn fidelity_limited(mut self) -> Self {
            self.fidelity_limited = true;
            self
        }
    }

    #[async_trait::async_trait]
    impl DiscoveryProvider for ScriptedOptional {
        fn id(&self) -> ProviderId {
            self.id.clone()
        }

        fn priority(&self) -> u8 {
            self.priority
        }

        async fn observe(&mut self) -> ProviderObservation {
            self.observes.fetch_add(1, Ordering::SeqCst);
            if let Some(gate) = &self.gate {
                gate.notified().await;
            }
            let result = self
                .results
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .pop_front()
                .unwrap_or(SectionResult::NotObserved);
            dns_observation(
                self.id.clone(),
                self.priority,
                result,
                self.fidelity_limited,
            )
        }
    }

    fn report_for<'a>(
        snapshot: &'a NetworkSnapshot,
        id: &ProviderId,
    ) -> &'a araiguma_core::provider::ProviderReport {
        snapshot
            .providers
            .iter()
            .find(|report| &report.provider == id)
            .unwrap_or_else(|| panic!("missing provider report for {id:?}"))
    }

    struct FakeProvider(ProviderId);

    #[async_trait::async_trait]
    impl DiscoveryProvider for FakeProvider {
        fn id(&self) -> ProviderId {
            self.0.clone()
        }
        fn priority(&self) -> u8 {
            if self.0 == ProviderId::Netlink {
                0
            } else {
                1
            }
        }
        async fn observe(&mut self) -> ProviderObservation {
            empty_observation(self.0.clone())
        }
    }

    fn fake_engine() -> Engine {
        Engine::new(vec![Box::new(FakeProvider(ProviderId::Netlink))])
    }

    #[tokio::test]
    async fn observe_now_merges_providers_by_priority() {
        let mut engine = Engine::new(vec![
            Box::new(FakeProvider(ProviderId::Netlink)),
            Box::new(FakeProvider(ProviderId::SysFs)),
        ]);
        let snapshot = engine.observe_now().await;
        assert_eq!(snapshot.providers.len(), 2);
        assert_eq!(snapshot.providers[0].provider, ProviderId::Netlink);
        assert_eq!(snapshot.providers[1].provider, ProviderId::SysFs);
    }

    fn empty_snapshot() -> NetworkSnapshot {
        NetworkSnapshot {
            schema_version: araiguma_core::snapshot::SNAPSHOT_SCHEMA_VERSION,
            timestamp_ms: 0,
            interfaces: vec![],
            addresses: vec![],
            routes: vec![],
            providers: vec![],
            dns: None,
            connections: None,
        }
    }

    #[tokio::test]
    async fn snapshot_envelope_is_the_opening_envelope() {
        let mut engine = fake_engine();
        let envelope = engine.snapshot_envelope().await;
        assert_eq!(envelope.event, EnvelopeKind::Snapshot);
        assert_eq!(envelope.seq, 1);
        assert!(envelope.snapshot.is_some());
    }

    #[tokio::test]
    async fn apply_change_produces_a_debounced_pure_diff() {
        let mut engine = fake_engine();
        let _ = engine.snapshot_envelope().await;
        let envelope = engine.apply_change(vec!["newLink".to_string()]).await;
        assert_eq!(envelope.event, EnvelopeKind::Changed);
        assert_eq!(envelope.seq, 2);
        assert_eq!(
            envelope.notified_by.as_deref(),
            Some(["newLink".to_string()].as_slice())
        );
        // A pure diff carries no document — the consumer re-derives from a
        // snapshot when it needs one.
        assert!(envelope.snapshot.is_none());
    }

    #[tokio::test]
    async fn apply_resync_carries_the_fresh_snapshot() {
        let mut engine = fake_engine();
        let _ = engine.snapshot_envelope().await;
        let envelope = engine.apply_resync().await;
        assert_eq!(envelope.event, EnvelopeKind::Resynchronized);
        assert_eq!(envelope.seq, 2);
        assert!(envelope.snapshot.is_some());
    }

    #[tokio::test]
    async fn shutdown_is_observable() {
        let mut engine = fake_engine();
        assert!(!engine.is_shutdown());
        engine.request_shutdown();
        assert!(engine.is_shutdown());
    }

    #[tokio::test]
    async fn subscribe_and_snapshot_and_lifecycle() {
        let mut engine = fake_engine();
        let (id, mut rx) = engine.subscribe_and_snapshot().await.expect("subscribe");
        assert!(id.starts_with("araiguma-sub-"));
        assert_eq!(engine.subscriber_count(), 1);
        let first = rx.recv().await.expect("first envelope is snapshot");
        assert_eq!(first.event, EnvelopeKind::Snapshot);

        // Broadcast reaches the live subscriber.
        let env = Envelope::changed(1, vec!["newLink".to_string()], Vec::new());
        assert_eq!(engine.broadcast(&env), 1);

        // Dropping the receiver disconnects: broadcast drops the dead
        // subscription.
        drop(rx);
        assert_eq!(engine.broadcast(&env), 0);
        assert_eq!(engine.subscriber_count(), 0);

        // Unsubscribe: idempotent for unknown ids.
        let (id2, _rx2) = engine.subscribe_and_snapshot().await.expect("re-subscribe");
        assert!(engine.unsubscribe(&id2).is_ok());
        assert!(engine.unsubscribe(&id2).is_err());
    }

    #[tokio::test]
    async fn sequence_numbers_are_monotonic_and_gap_free() {
        let mut engine = fake_engine();
        let mut seqs = Vec::new();
        seqs.push(engine.snapshot_envelope().await.seq);
        seqs.push(engine.apply_change(vec!["newLink".to_string()]).await.seq);
        seqs.push(engine.apply_resync().await.seq);
        seqs.push(
            engine
                .monitor_health(MonitorHealthState::Degraded, None)
                .seq,
        );
        let (id, mut rx) = engine.subscribe_and_snapshot().await.expect("subscribe");
        seqs.push(rx.recv().await.expect("envelope").seq);
        assert_eq!(seqs, vec![1, 2, 3, 4, 5]);
        assert!(engine.unsubscribe(&id).is_ok());
    }

    #[tokio::test]
    async fn two_subscribers_observe_contiguous_session_sequences() {
        let mut engine = fake_engine();
        let (first_id, mut first_rx) = engine
            .subscribe_and_snapshot()
            .await
            .expect("first subscription");
        assert_eq!(first_rx.recv().await.expect("opening snapshot").seq, 1);

        let (second_id, mut second_rx) = engine
            .subscribe_and_snapshot()
            .await
            .expect("second subscription");
        assert_eq!(
            first_rx.recv().await.expect("shared opening snapshot").seq,
            2
        );
        assert_eq!(second_rx.recv().await.expect("opening snapshot").seq, 2);

        let changed = engine.apply_change(vec!["newLink".to_string()]).await;
        assert_eq!(changed.seq, 3);
        engine.broadcast(&changed);
        assert_eq!(first_rx.recv().await.expect("first change").seq, 3);
        assert_eq!(second_rx.recv().await.expect("second change").seq, 3);

        assert!(engine.unsubscribe(&first_id).is_ok());
        assert!(engine.unsubscribe(&second_id).is_ok());
    }

    #[tokio::test]
    async fn concurrent_subscriptions_cannot_reorder_the_session_stream() {
        let engine = Arc::new(tokio::sync::Mutex::new(fake_engine()));
        let first_engine = engine.clone();
        let first = tokio::spawn(async move {
            first_engine
                .lock()
                .await
                .subscribe_and_snapshot()
                .await
                .expect("first subscription")
        });
        let second_engine = engine.clone();
        let second = tokio::spawn(async move {
            second_engine
                .lock()
                .await
                .subscribe_and_snapshot()
                .await
                .expect("second subscription")
        });

        let (first_result, second_result) = tokio::join!(first, second);
        let (_, mut first_rx) = first_result.expect("first task");
        let (_, mut second_rx) = second_result.expect("second task");

        let mut first_seqs = Vec::new();
        let mut second_seqs = Vec::new();
        while let Ok(envelope) = first_rx.try_recv() {
            first_seqs.push(envelope.seq);
        }
        while let Ok(envelope) = second_rx.try_recv() {
            second_seqs.push(envelope.seq);
        }

        let changed = {
            let mut engine = engine.lock().await;
            let changed = engine.apply_change(vec!["newLink".to_string()]).await;
            engine.broadcast(&changed);
            changed
        };
        first_seqs.push(first_rx.recv().await.expect("first change").seq);
        second_seqs.push(second_rx.recv().await.expect("second change").seq);

        for sequence in first_seqs.windows(2) {
            assert_eq!(sequence[1], sequence[0] + 1);
        }
        for sequence in second_seqs.windows(2) {
            assert_eq!(sequence[1], sequence[0] + 1);
        }
        assert_eq!(first_seqs.last(), Some(&changed.seq));
        assert_eq!(second_seqs.last(), Some(&changed.seq));
    }

    #[tokio::test]
    async fn a_shut_down_engine_rejects_new_subscriptions() {
        let mut engine = fake_engine();
        engine.request_shutdown();
        let error = engine
            .subscribe_and_snapshot()
            .await
            .expect_err("post-shutdown subscription is rejected");
        assert!(error.starts_with("shutdownInProgress:"));
        assert_eq!(engine.subscriber_count(), 0);
    }

    #[test]
    fn envelope_shapes_match_the_ipc_contract() {
        let value = serde_json::to_value(Envelope::snapshot(1, empty_snapshot())).unwrap();
        assert_eq!(value["event"], "snapshot");
        assert_eq!(value["seq"], 1);
        assert_eq!(
            value["snapshot"]["schemaVersion"],
            araiguma_core::snapshot::SNAPSHOT_SCHEMA_VERSION
        );
        assert_eq!(value["snapshot"]["timestampMs"], 0);

        let value = serde_json::to_value(Envelope::changed(
            2,
            vec!["newLink".to_string()],
            vec![NetworkEvent::Resynchronized],
        ))
        .unwrap();
        assert_eq!(value["event"], "changed");
        assert_eq!(value["notifiedBy"][0], "newLink");
        assert_eq!(value["changes"][0], "resynchronized");

        let value = serde_json::to_value(Envelope::resynchronized(3, Vec::new(), empty_snapshot()))
            .unwrap();
        assert_eq!(value["event"], "resynchronized");
        assert!(value["snapshot"]["interfaces"].is_array());

        let value = serde_json::to_value(Envelope::monitor_health(
            4,
            MonitorHealthState::Degraded,
            Some("netlink event stream disconnected".to_string()),
        ))
        .unwrap();
        assert_eq!(value["event"], "monitorHealth");
        assert_eq!(value["seq"], 4);
        assert_eq!(value["monitor"]["state"], "degraded");
        assert_eq!(
            value["monitor"]["reason"],
            "netlink event stream disconnected"
        );
        assert!(value.get("snapshot").is_none());
    }

    /// The CLI contract is unchanged by the IPC extension: the CLI stream
    /// carries the same three envelopes and NO sequence number.
    #[test]
    fn cli_envelope_shape_is_unchanged() {
        let value: serde_json::Value =
            serde_json::from_str(&Envelope::snapshot(1, empty_snapshot()).to_cli_json_string())
                .unwrap();
        assert_eq!(value["event"], "snapshot");
        assert_eq!(
            value["snapshot"]["schemaVersion"],
            araiguma_core::snapshot::SNAPSHOT_SCHEMA_VERSION
        );
        assert!(value.get("seq").is_none());

        let value: serde_json::Value = serde_json::from_str(
            &Envelope::changed(
                2,
                vec!["newLink".to_string()],
                vec![NetworkEvent::Resynchronized],
            )
            .to_cli_json_string(),
        )
        .unwrap();
        assert_eq!(value["event"], "changed");
        assert_eq!(value["notifiedBy"][0], "newLink");
        assert_eq!(value["changes"][0], "resynchronized");
        assert!(value.get("seq").is_none());

        let value: serde_json::Value = serde_json::from_str(
            &Envelope::resynchronized(3, Vec::new(), empty_snapshot()).to_cli_json_string(),
        )
        .unwrap();
        assert_eq!(value["event"], "resynchronized");
        assert!(value["snapshot"]["interfaces"].is_array());
        assert!(value.get("seq").is_none());
    }

    /// The engine and the consumer compose into the CLI watch loop: a
    /// snapshot envelope, then debounced change envelopes.
    #[tokio::test]
    async fn engine_and_feed_compose_into_the_watch_stream() {
        let mut feed = FakeFeed::pending_after_script(vec![
            Signal::Changed { kind: "newLink" },
            Signal::Changed { kind: "newRoute" },
        ]);
        let mut engine = fake_engine();
        let mut stop = std::pin::pin!(std::future::pending::<()>());

        let opening = engine.snapshot_envelope().await;
        assert_eq!(opening.event, EnvelopeKind::Snapshot);

        let first = feed.next_signal().await.expect("signal");
        let Signal::Changed { kind } = first else {
            panic!("expected a change signal");
        };
        let burst = debounce(&mut feed, kind.to_string(), &mut stop)
            .await
            .expect("burst");
        let envelope = engine.apply_change(burst.notified_by).await;
        assert_eq!(envelope.event, EnvelopeKind::Changed);
        assert_eq!(
            envelope.notified_by.as_deref(),
            Some(["newLink".to_string(), "newRoute".to_string()].as_slice())
        );
    }

    #[test]
    fn registry_is_deterministic_and_rejects_duplicate_identities() {
        let mut registry = ProviderRegistry::new();
        registry
            .register(Box::new(FakeProvider(ProviderId::SysFs)))
            .expect("sysfs");
        registry
            .register(Box::new(FakeProvider(ProviderId::Netlink)))
            .expect("netlink");
        registry
            .register(Box::new(FakeProvider(ProviderId::NetworkManager)))
            .expect("NetworkManager");
        let error = registry
            .register(Box::new(FakeProvider(ProviderId::Netlink)))
            .expect_err("duplicate identity must be rejected");
        assert!(error.contains("duplicateProvider"));
        assert_eq!(
            registry.ids(),
            vec![
                ProviderId::Netlink,
                ProviderId::SysFs,
                ProviderId::NetworkManager
            ]
        );
    }

    #[test]
    fn registry_construction_does_not_observe() {
        let (provider, observes) = ScriptedOptional::new(
            ProviderId::SystemdResolved,
            3,
            vec![SectionResult::Observed(Vec::new())],
        );
        let mut registry = ProviderRegistry::new();
        registry.register(Box::new(provider)).expect("register");
        assert_eq!(observes.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn default_registry_contains_all_four_providers_in_order() {
        let engine = Engine::default_providers();
        assert_eq!(
            engine.provider_ids(),
            vec![
                ProviderId::Netlink,
                ProviderId::SysFs,
                ProviderId::NetworkManager,
                ProviderId::SystemdResolved
            ]
        );
    }

    #[tokio::test]
    async fn initial_snapshot_does_not_wait_for_optional_providers() {
        let gate = Arc::new(Notify::new());
        let (optional, observes) = ScriptedOptional::new(
            ProviderId::SystemdResolved,
            3,
            vec![SectionResult::Observed(Vec::new())],
        );
        let optional = optional.gated(Arc::clone(&gate));
        let mut engine = Engine::new(vec![
            Box::new(FakeProvider(ProviderId::Netlink)),
            Box::new(optional),
        ]);

        let document = tokio::time::timeout(Duration::from_millis(100), engine.snapshot_document())
            .await
            .expect("initial snapshot must not wait for optional providers")
            .expect("snapshot");
        assert_eq!(observes.load(Ordering::SeqCst), 0);
        assert!(document.dns.is_none());
        document
            .validate_v4_reports()
            .expect("initial report must validate before optional polling");
        let report = report_for(&document, &ProviderId::SystemdResolved);
        assert_eq!(report.state, ProviderState::NotYetObserved);
        for section in &report.sections {
            if section.section == SectionKind::Dns {
                assert!(matches!(section.outcome, SectionOutcome::NotObserved));
            } else {
                assert!(matches!(section.outcome, SectionOutcome::Unsupported));
            }
        }

        gate.notify_one();
        let envelope = engine
            .refresh_optional()
            .await
            .expect("optional refresh produces an envelope");
        assert_eq!(envelope.event, EnvelopeKind::Changed);
        let current = engine.current_document().expect("document");
        assert_eq!(current.dns, Some(Vec::new()));
        assert_eq!(
            report_for(current, &ProviderId::SystemdResolved).state,
            ProviderState::Available
        );
    }

    #[tokio::test]
    async fn optional_success_empty_and_failure_replace_the_document() {
        let (optional, _observes) = ScriptedOptional::new(
            ProviderId::SystemdResolved,
            3,
            vec![
                SectionResult::Observed(vec![empty_dns()]),
                SectionResult::Observed(Vec::new()),
                SectionResult::Unavailable,
                SectionResult::PermissionDenied,
                SectionResult::Failed("timeout".to_owned()),
                SectionResult::Failed("malformedResponse".to_owned()),
            ],
        );
        let mut engine = Engine::new(vec![
            Box::new(FakeProvider(ProviderId::Netlink)),
            Box::new(optional),
        ]);
        let _ = engine.snapshot_document().await.expect("seed");

        let _ = engine.refresh_optional().await.expect("success");
        assert_eq!(
            engine
                .current_document()
                .unwrap()
                .dns
                .as_ref()
                .map(Vec::len),
            Some(1)
        );

        let _ = engine.refresh_optional().await.expect("empty");
        assert_eq!(engine.current_document().unwrap().dns, Some(Vec::new()));

        let _ = engine.refresh_optional().await.expect("unavailable");
        assert!(engine.current_document().unwrap().dns.is_none());
        assert_eq!(
            report_for(
                engine.current_document().unwrap(),
                &ProviderId::SystemdResolved
            )
            .state,
            ProviderState::Unavailable
        );

        let _ = engine.refresh_optional().await.expect("permission");
        assert!(engine.current_document().unwrap().dns.is_none());
        let _ = engine.refresh_optional().await.expect("timeout");
        assert!(engine.current_document().unwrap().dns.is_none());
        let _ = engine.refresh_optional().await.expect("malformed");
        assert!(engine.current_document().unwrap().dns.is_none());
    }

    #[tokio::test]
    async fn optional_partial_observation_is_degraded_not_failed() {
        let (optional, _) = ScriptedOptional::new(
            ProviderId::SystemdResolved,
            3,
            vec![SectionResult::Observed(Vec::new())],
        );
        let optional = optional.fidelity_limited();
        let mut engine = Engine::new(vec![
            Box::new(FakeProvider(ProviderId::Netlink)),
            Box::new(optional),
        ]);
        let _ = engine.snapshot_document().await.expect("seed");
        let _ = engine.refresh_optional().await.expect("partial");
        let document = engine.current_document().expect("document");
        assert_eq!(document.dns, Some(Vec::new()));
        assert_eq!(
            report_for(document, &ProviderId::SystemdResolved).state,
            ProviderState::Degraded
        );
    }

    #[tokio::test]
    async fn optional_disappearance_and_recovery_do_not_restart_the_engine() {
        let (optional, observes) = ScriptedOptional::new(
            ProviderId::SystemdResolved,
            3,
            vec![
                SectionResult::Observed(vec![empty_dns()]),
                SectionResult::Unavailable,
                SectionResult::Observed(vec![empty_dns()]),
            ],
        );
        let mut engine = Engine::new(vec![
            Box::new(FakeProvider(ProviderId::Netlink)),
            Box::new(optional),
        ]);
        let _ = engine.snapshot_document().await.expect("seed");

        let _ = engine.refresh_optional().await.expect("first");
        assert_eq!(
            engine
                .current_document()
                .unwrap()
                .dns
                .as_ref()
                .map(Vec::len),
            Some(1)
        );
        let _ = engine.refresh_optional().await.expect("disappear");
        assert!(engine.current_document().unwrap().dns.is_none());
        let _ = engine.refresh_optional().await.expect("recover");
        assert_eq!(
            engine
                .current_document()
                .unwrap()
                .dns
                .as_ref()
                .map(Vec::len),
            Some(1)
        );
        assert_eq!(observes.load(Ordering::SeqCst), 3);
    }

    #[tokio::test]
    async fn optional_refresh_does_not_observe_the_baseline_again() {
        let baseline_observes = Arc::new(AtomicUsize::new(0));
        let baseline = FakeProviderWithCount(ProviderId::Netlink, Arc::clone(&baseline_observes));
        let (optional, _) = ScriptedOptional::new(
            ProviderId::SystemdResolved,
            3,
            vec![SectionResult::Observed(Vec::new())],
        );
        let mut engine = Engine::new(vec![Box::new(baseline), Box::new(optional)]);
        let _ = engine.snapshot_document().await.expect("seed");
        assert_eq!(baseline_observes.load(Ordering::SeqCst), 1);
        let _ = engine.refresh_optional().await.expect("optional");
        assert_eq!(
            baseline_observes.load(Ordering::SeqCst),
            1,
            "optional refresh must not re-observe the baseline"
        );
    }

    struct SlowOptional {
        active: Arc<AtomicUsize>,
        max_active: Arc<AtomicUsize>,
    }

    impl SlowOptional {
        fn new(max_active: Arc<AtomicUsize>) -> Self {
            Self {
                active: Arc::new(AtomicUsize::new(0)),
                max_active,
            }
        }
    }

    #[async_trait::async_trait]
    impl DiscoveryProvider for SlowOptional {
        fn id(&self) -> ProviderId {
            ProviderId::SystemdResolved
        }

        fn priority(&self) -> u8 {
            3
        }

        async fn observe(&mut self) -> ProviderObservation {
            let active = self.active.fetch_add(1, Ordering::SeqCst) + 1;
            self.max_active.fetch_max(active, Ordering::SeqCst);
            tokio::time::sleep(Duration::from_millis(50)).await;
            self.active.fetch_sub(1, Ordering::SeqCst);
            dns_observation(
                self.id(),
                self.priority(),
                SectionResult::Observed(Vec::new()),
                false,
            )
        }
    }

    #[tokio::test]
    async fn optional_refreshes_do_not_overlap() {
        let max_active = Arc::new(AtomicUsize::new(0));
        let optional = SlowOptional::new(Arc::clone(&max_active));
        let engine = Arc::new(tokio::sync::Mutex::new(Engine::new(vec![
            Box::new(FakeProvider(ProviderId::Netlink)),
            Box::new(optional),
        ])));

        let first_engine = Arc::clone(&engine);
        let first = tokio::spawn(async move { first_engine.lock().await.refresh_optional().await });
        tokio::time::sleep(Duration::from_millis(10)).await;
        let second_engine = Arc::clone(&engine);
        let second =
            tokio::spawn(async move { second_engine.lock().await.refresh_optional().await });

        let _ = first.await.expect("first refresh");
        let _ = second.await.expect("second refresh");
        assert_eq!(
            max_active.load(Ordering::SeqCst),
            1,
            "optional refreshes must not overlap"
        );
    }

    #[tokio::test]
    async fn optional_change_is_delivered_to_subscribers() {
        let (optional, _) = ScriptedOptional::new(
            ProviderId::SystemdResolved,
            3,
            vec![SectionResult::Observed(vec![empty_dns()])],
        );
        let mut engine = Engine::new(vec![
            Box::new(FakeProvider(ProviderId::Netlink)),
            Box::new(optional),
        ]);
        let (_id, mut rx) = engine.subscribe_and_snapshot().await.expect("subscribe");
        let opening = rx.recv().await.expect("opening snapshot");
        assert_eq!(opening.event, EnvelopeKind::Snapshot);
        assert!(opening.snapshot.as_ref().unwrap().dns.is_none());

        let envelope = engine.refresh_optional().await.expect("optional change");
        assert_eq!(envelope.event, EnvelopeKind::Changed);
        engine.broadcast(&envelope);
        let changed = rx.recv().await.expect("changed envelope");
        assert_eq!(changed.event, EnvelopeKind::Changed);
        assert_eq!(
            changed.notified_by.as_deref(),
            Some(["optionalRefresh".to_string()].as_slice())
        );
    }

    #[tokio::test]
    async fn shutdown_cancels_optional_refresh() {
        let (optional, _) = ScriptedOptional::new(
            ProviderId::SystemdResolved,
            3,
            vec![SectionResult::Observed(Vec::new())],
        );
        let mut engine = Engine::new(vec![
            Box::new(FakeProvider(ProviderId::Netlink)),
            Box::new(optional),
        ]);
        let _ = engine.snapshot_document().await.expect("seed");
        engine.request_shutdown();
        assert!(engine.refresh_optional().await.is_none());
    }

    struct FakeProviderWithCount(ProviderId, Arc<AtomicUsize>);

    #[async_trait::async_trait]
    impl DiscoveryProvider for FakeProviderWithCount {
        fn id(&self) -> ProviderId {
            self.0.clone()
        }

        fn priority(&self) -> u8 {
            0
        }

        async fn observe(&mut self) -> ProviderObservation {
            self.1.fetch_add(1, Ordering::SeqCst);
            empty_observation(self.0.clone())
        }
    }
}
