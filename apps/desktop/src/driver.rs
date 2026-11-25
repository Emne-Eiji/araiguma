//! Live observation driver and Tauri event forwarding (document 01 §7.3).
//!
//! The CHANGE FEED is owned here, not by the engine. The driver waits for
//! a signal without holding the engine lock, then asks the shared engine
//! to observe and produce an envelope. That split is structural: while the
//! driver waits, `get_snapshot`, `get_topology`, and `subscribe_watch`
//! remain answerable.
//!
//! Envelopes are forwarded to the webview on the `araiguma/event` channel
//! (document 04 §7). The first event for a subscription is always a
//! `snapshot` envelope, delivered by the engine when the subscription is
//! created.

use std::sync::Arc;
use std::time::Duration;

use araiguma_engine::{
    debounce, default_change_feed, ChangeFeed, Engine, Envelope, MonitorHealthState,
    OPTIONAL_POLL_INTERVAL,
};
use araiguma_netlink::Signal;
use tauri::{Emitter, WebviewWindow};
use tokio::sync::{watch, Mutex};

use crate::state::{AppState, MonitoringStatus};

/// The IPC event channel name (document 04 §7).
pub const EVENT_CHANNEL: &str = "araiguma/event";

/// Drive live observations until the application requests shutdown.
///
/// The engine lock is held only for the observation and broadcast itself;
/// it is never held while waiting for a netlink signal or during the
/// debounce window.
pub async fn drive(state: AppState) {
    let feed = match default_change_feed() {
        Ok(feed) => feed,
        Err(error) => {
            // Monitoring is unavailable, but observation itself remains
            // available through the IPC snapshot/topology commands. Report
            // the degraded mode; never pretend that live events are flowing.
            eprintln!("araiguma: cannot start netlink monitoring: {error}");
            state.set_monitoring_status(MonitoringStatus::Unavailable(error));
            return;
        }
    };

    if let Err(error) = drive_with_feed(state, feed).await {
        eprintln!("araiguma: network change monitoring stopped: {error}");
        // `drive_with_feed` owns the state it used, so report the terminal
        // failure through the same state instance before returning.
        // (The caller keeps no second status channel.)
    }
}

async fn drive_with_feed<F: ChangeFeed>(state: AppState, feed: F) -> Result<(), String> {
    drive_with_feed_and_optional_interval(state, feed, OPTIONAL_POLL_INTERVAL).await
}

async fn drive_with_feed_and_optional_interval<F: ChangeFeed>(
    state: AppState,
    mut feed: F,
    optional_interval: Duration,
) -> Result<(), String> {
    // Seed the engine before the first event can arrive. This establishes
    // the baseline against which the first change diff is computed. The
    // opening document is baseline-only; optional providers remain
    // NotYetObserved until the optional refresh loop runs.
    {
        let mut engine = state.engine.lock().await;
        let _ = engine.snapshot_document().await;
    }
    state.set_monitoring_status(MonitoringStatus::Running);

    let mut shutdown = state.shutdown_receiver();
    let mut stop = std::pin::pin!(wait_for_shutdown(&mut shutdown));
    let mut optional_timer = tokio::time::interval_at(
        tokio::time::Instant::now() + optional_interval,
        optional_interval,
    );
    optional_timer.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    let outcome = loop {
        let signal = tokio::select! {
            _ = &mut stop => break Ok(()),
            signal = feed.next_signal() => signal,
            _ = optional_timer.tick() => {
                let envelope = {
                    let mut engine = state.engine.lock().await;
                    engine.refresh_optional().await
                };
                if let Some(envelope) = envelope {
                    let mut engine = state.engine.lock().await;
                    engine.broadcast(&envelope);
                }
                continue;
            }
        };
        let Some(signal) = signal else {
            break Err("the network change feed ended".to_string());
        };

        match signal {
            Signal::Disconnected => {
                report_monitor_degraded(
                    &state,
                    "the netlink event monitor disconnected; reconnecting".to_string(),
                )
                .await;
            }
            Signal::ReconnectFailed { reason } => {
                report_monitor_degraded(
                    &state,
                    format!("the netlink event monitor reconnect failed: {reason}"),
                )
                .await;
            }
            Signal::Reconnected => {
                publish_resynchronization(&state, true).await;
            }
            Signal::Resynchronized => {
                publish_resynchronization(&state, false).await;
            }
            Signal::Changed { kind } => {
                let Some(burst) = debounce(&mut feed, kind.to_string(), &mut stop).await else {
                    break Ok(());
                };
                if let Some(reason) = burst.monitor_degraded {
                    report_monitor_degraded(&state, reason).await;
                } else if burst.resync {
                    publish_resynchronization(&state, burst.monitor_recovered).await;
                } else {
                    let mut engine = state.engine.lock().await;
                    let envelope = engine.apply_change(burst.notified_by).await;
                    engine.broadcast(&envelope);
                }
            }
        }
    };

    if let Err(error) = &outcome {
        state.set_monitoring_status(MonitoringStatus::Unavailable(error.clone()));
    }
    let mut engine = state.engine.lock().await;
    engine.request_shutdown();
    outcome
}

/// Publish a degraded monitor-health signal. The process-wide status is
/// changed first so a new subscription cannot be admitted as LIVE while an
/// existing subscriber receives the same fact.
async fn report_monitor_degraded(state: &AppState, reason: String) {
    eprintln!("araiguma: live event monitor degraded: {reason}");
    state.set_monitoring_status(MonitoringStatus::Degraded(reason.clone()));
    let mut engine = state.engine.lock().await;
    let envelope = engine.monitor_health(MonitorHealthState::Degraded, Some(reason));
    engine.broadcast(&envelope);
}

/// Re-observe after a monitoring gap and publish the authoritative snapshot.
/// When `monitor_recovered` is true, recovery is published only after that
/// fresh observation exists; the frontend therefore cannot return to LIVE
/// from a stale document.
async fn publish_resynchronization(state: &AppState, monitor_recovered: bool) {
    let mut engine = state.engine.lock().await;
    let envelope = engine.apply_resync().await;
    engine.broadcast(&envelope);

    if monitor_recovered {
        let recovered = engine.monitor_health(MonitorHealthState::Recovered, None);
        engine.broadcast(&recovered);
    }
    drop(engine);

    if monitor_recovered {
        state.set_monitoring_status(MonitoringStatus::Running);
        eprintln!("araiguma: live event monitor recovered");
    }
}

/// Resolve when shutdown has been requested (or the sender is dropped).
async fn wait_for_shutdown(receiver: &mut watch::Receiver<bool>) {
    loop {
        if *receiver.borrow_and_update() {
            return;
        }
        if receiver.changed().await.is_err() {
            return;
        }
    }
}

/// Drain a subscriber channel and forward envelopes to the window on the
/// `araiguma/event` channel. The task ends when the engine removes the
/// subscription and closes the channel.
pub fn pump(
    window: WebviewWindow,
    engine: Arc<Mutex<Engine>>,
    subscription_id: String,
    rx: futures::channel::mpsc::UnboundedReceiver<Envelope>,
) {
    tauri::async_runtime::spawn(async move {
        let mut rx = rx;
        while let Ok(envelope) = rx.recv().await {
            if window.emit(EVENT_CHANNEL, &envelope).is_err() {
                break;
            }
        }

        // A disconnected webview must not leave its subscription retained
        // by the engine. Shutdown may already have removed it; either way,
        // cleanup is idempotent at this boundary.
        let mut engine = engine.lock().await;
        let _ = engine.unsubscribe(&subscription_id);
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::paths::XdgPaths;
    use araiguma_core::model::{DnsObservation, DnsScope, ObservationSource};
    use araiguma_core::provider::{
        DiscoveryProvider, ProviderId, ProviderObservation, SectionResult,
    };
    use araiguma_engine::{Engine, EnvelopeKind, FakeFeed};
    use araiguma_netlink::Signal;
    use std::path::PathBuf;
    use std::time::Duration;

    struct WaitingFeed;

    #[async_trait::async_trait]
    impl ChangeFeed for WaitingFeed {
        async fn next_signal(&mut self) -> Option<Signal> {
            std::future::pending().await
        }
    }

    fn state() -> AppState {
        AppState::new(XdgPaths {
            config_dir: PathBuf::from("/tmp/config/araiguma"),
            state_dir: PathBuf::from("/tmp/state/araiguma"),
        })
    }

    #[tokio::test]
    async fn the_engine_lock_is_not_held_while_waiting_for_a_signal() {
        let state = state();
        let engine = state.engine.clone();
        let task = tokio::spawn(drive_with_feed(state.clone(), WaitingFeed));

        // The driver has seeded the engine and is now waiting for a signal.
        // An IPC command must still be able to take the engine lock.
        tokio::time::sleep(std::time::Duration::from_millis(25)).await;
        let acquired =
            tokio::time::timeout(std::time::Duration::from_millis(100), engine.lock()).await;
        assert!(acquired.is_ok(), "IPC could not take the engine lock");
        drop(acquired);

        state.request_shutdown();
        tokio::time::timeout(std::time::Duration::from_secs(1), task)
            .await
            .expect("driver stopped")
            .expect("driver task completed")
            .expect("driver stopped cleanly");
    }

    #[tokio::test]
    async fn a_disconnect_inside_a_change_burst_is_published_before_recovery() {
        let state = state();
        let (subscription_id, mut events) = state
            .engine
            .lock()
            .await
            .subscribe_and_snapshot()
            .await
            .expect("subscribe");
        let feed = FakeFeed::new(vec![
            // The disconnect arrives inside the 150 ms debounce window.
            // It must still be published as degraded before recovery.
            Signal::Changed { kind: "newLink" },
            Signal::Disconnected,
            Signal::ReconnectFailed {
                reason: "synthetic reconnect failure".to_string(),
            },
            Signal::Reconnected,
        ]);
        let task = tokio::spawn(drive_with_feed(state.clone(), feed));

        let opening = events.recv().await.expect("opening snapshot");
        assert_eq!(opening.event, EnvelopeKind::Snapshot);

        let degraded = events.recv().await.expect("disconnect health signal");
        assert_eq!(degraded.event, EnvelopeKind::MonitorHealth);
        assert_eq!(
            degraded.monitor.as_ref().unwrap().state,
            MonitorHealthState::Degraded
        );
        assert!(degraded
            .monitor
            .as_ref()
            .unwrap()
            .reason
            .as_deref()
            .unwrap()
            .contains("disconnected"));

        let failed = events.recv().await.expect("failed reconnect health signal");
        assert_eq!(failed.event, EnvelopeKind::MonitorHealth);
        assert_eq!(
            failed.monitor.as_ref().unwrap().state,
            MonitorHealthState::Degraded
        );
        assert!(failed
            .monitor
            .as_ref()
            .unwrap()
            .reason
            .as_deref()
            .unwrap()
            .contains("synthetic reconnect failure"));

        let resynchronized = events.recv().await.expect("fresh resync snapshot");
        assert_eq!(resynchronized.event, EnvelopeKind::Resynchronized);
        assert!(resynchronized.snapshot.is_some());

        let recovered = events.recv().await.expect("recovery health signal");
        assert_eq!(recovered.event, EnvelopeKind::MonitorHealth);
        assert_eq!(
            recovered.monitor.as_ref().unwrap().state,
            MonitorHealthState::Recovered
        );

        assert_eq!(
            [
                opening.seq,
                degraded.seq,
                failed.seq,
                resynchronized.seq,
                recovered.seq
            ],
            [1, 2, 3, 4, 5]
        );
        let _ = state.engine.lock().await.unsubscribe(&subscription_id);

        tokio::time::timeout(std::time::Duration::from_secs(1), task)
            .await
            .expect("driver stopped")
            .expect("driver task completed")
            .expect_err("feed end reported");
    }

    struct BaselineProvider;

    #[async_trait::async_trait]
    impl DiscoveryProvider for BaselineProvider {
        fn id(&self) -> ProviderId {
            ProviderId::Netlink
        }

        fn priority(&self) -> u8 {
            0
        }

        async fn observe(&mut self) -> ProviderObservation {
            ProviderObservation {
                provider: ProviderId::Netlink,
                priority: 0,
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

    struct OptionalDnsProvider;

    #[async_trait::async_trait]
    impl DiscoveryProvider for OptionalDnsProvider {
        fn id(&self) -> ProviderId {
            ProviderId::SystemdResolved
        }

        fn priority(&self) -> u8 {
            3
        }

        async fn observe(&mut self) -> ProviderObservation {
            ProviderObservation {
                provider: ProviderId::SystemdResolved,
                priority: 3,
                interfaces: SectionResult::Unsupported,
                addresses_v4: SectionResult::Unsupported,
                addresses_v6: SectionResult::Unsupported,
                routes_v4: SectionResult::Unsupported,
                routes_v6: SectionResult::Unsupported,
                dns: SectionResult::Observed(vec![DnsObservation {
                    scope: DnsScope::Global,
                    mode: None,
                    servers: Vec::new(),
                    search_domains: Vec::new(),
                    routing_domains: Vec::new(),
                    sources: vec![ObservationSource::SystemdResolved],
                    notes: Vec::new(),
                }]),
                connections: SectionResult::Unsupported,
                fidelity_limited: false,
                notes: Vec::new(),
            }
        }
    }

    #[tokio::test]
    async fn optional_refresh_reaches_subscribers_without_restart() {
        let engine = Engine::new(vec![
            Box::new(BaselineProvider),
            Box::new(OptionalDnsProvider),
        ]);
        let state = AppState::with_engine(
            XdgPaths {
                config_dir: PathBuf::from("/tmp/config/araiguma"),
                state_dir: PathBuf::from("/tmp/state/araiguma"),
            },
            engine,
        );
        let (_id, mut events) = state
            .engine
            .lock()
            .await
            .subscribe_and_snapshot()
            .await
            .expect("subscribe");
        let task = tokio::spawn(drive_with_feed_and_optional_interval(
            state.clone(),
            WaitingFeed,
            Duration::from_millis(10),
        ));

        let opening = tokio::time::timeout(Duration::from_secs(1), events.recv())
            .await
            .expect("opening snapshot")
            .expect("opening envelope");
        assert_eq!(opening.event, EnvelopeKind::Snapshot);
        assert!(opening.snapshot.as_ref().unwrap().dns.is_none());

        let changed = tokio::time::timeout(Duration::from_secs(1), events.recv())
            .await
            .expect("optional refresh event")
            .expect("changed envelope");
        assert_eq!(changed.event, EnvelopeKind::Changed);
        assert_eq!(
            changed.notified_by.as_deref(),
            Some(["optionalRefresh".to_string()].as_slice())
        );

        state.request_shutdown();
        tokio::time::timeout(Duration::from_secs(1), task)
            .await
            .expect("driver stopped")
            .expect("driver task completed")
            .expect("driver stopped cleanly");
    }
}
