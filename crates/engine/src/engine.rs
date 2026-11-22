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

use araiguma_core::provider::DiscoveryProvider;
use araiguma_core::snapshot::NetworkEvent;
use araiguma_core::{diff_snapshots, merge, NetworkSnapshot};
use araiguma_netlink::NetlinkProvider;
use araiguma_sysfs::SysFsProvider;

/// Burst-collapse window — identical to the CLI watch loop window
/// (document 01 §4.1).
pub const DEBOUNCE: Duration = Duration::from_millis(150);

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

/// The shared observation engine.
pub struct Engine {
    providers: Vec<Box<dyn DiscoveryProvider>>,
    document: Option<NetworkSnapshot>,
    seq: u64,
    subscriptions: Vec<(String, UnboundedSender<Envelope>)>,
    subscription_counter: u64,
    stopped: bool,
}

impl Engine {
    pub fn new(providers: Vec<Box<dyn DiscoveryProvider>>) -> Engine {
        Engine {
            providers,
            document: None,
            seq: 0,
            subscriptions: Vec::new(),
            subscription_counter: 0,
            stopped: false,
        }
    }

    /// The default provider set: netlink first (the authoritative source),
    /// sysfs second (gap filling). The merge layer orders by priority
    /// regardless of observation order.
    pub fn default_providers() -> Engine {
        Engine::new(vec![
            Box::new(NetlinkProvider::new()),
            Box::new(SysFsProvider::new()),
        ])
    }

    /// Run one observation pass and merge (never panics: provider failures
    /// surface as failed sections in the returned snapshot).
    pub async fn observe_now(&mut self) -> NetworkSnapshot {
        let mut observations = Vec::new();
        for provider in &mut self.providers {
            observations.push(provider.observe().await);
        }
        merge(observations, now_ms())
    }

    /// The current validated document; seeds via [`Engine::observe_now`]
    /// on the first call.
    pub async fn snapshot_document(&mut self) -> Result<NetworkSnapshot, String> {
        if self.document.is_none() {
            let document = self.observe_now().await;
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
        let fresh = self.observe_now().await;
        let changes = self.diff_to(&fresh);
        self.document = Some(fresh.clone());
        let seq = self.next_seq();
        Envelope::changed(seq, notified_by, changes)
    }

    /// Observe now and return a `resynchronized` envelope: a monitoring gap
    /// occurred, changes during the gap were NOT observed, and the fresh
    /// snapshot is authoritative again (document 04 §8.3).
    pub async fn apply_resync(&mut self) -> Envelope {
        let fresh = self.observe_now().await;
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

// ---------------------------------------------------------------------------
// Tests (engine unit tests, document 07 §3 and §6).
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::change_feed::{debounce, ChangeFeed, FakeFeed};
    use araiguma_core::provider::{ProviderId, ProviderObservation, SectionResult};
    use araiguma_netlink::Signal;
    use std::sync::Arc;

    fn empty_observation(provider: ProviderId) -> ProviderObservation {
        ProviderObservation {
            provider,
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
            fidelity_limited: false,
            notes: Vec::new(),
        }
    }

    struct FakeProvider(ProviderId);

    #[async_trait::async_trait]
    impl DiscoveryProvider for FakeProvider {
        fn id(&self) -> ProviderId {
            self.0
        }
        fn priority(&self) -> u8 {
            if self.0 == ProviderId::Netlink {
                0
            } else {
                1
            }
        }
        async fn observe(&mut self) -> ProviderObservation {
            empty_observation(self.0)
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
        assert_eq!(value["snapshot"]["schemaVersion"], 3);
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
        assert_eq!(value["snapshot"]["schemaVersion"], 3);
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
}
