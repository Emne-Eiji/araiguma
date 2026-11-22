//! Engine-side change feed abstraction (document 02, `ChangeFeed`).
//!
//! The feed yields the next change signal, monitor-health transition, or
//! resynchronization marker on demand — providers are sources, feeds are
//! streams. The existing netlink [`Monitor`] semantics become the first
//! implementation.
//!
//! Ownership note (WO-05): the feed is owned by the CONSUMER, not by the
//! engine. The engine owns the observation state (providers, current
//! document, envelope sequence, subscribers); a consumer owns the signal
//! source and the debounce window. That split is what lets the desktop
//! shell wait for change signals without holding the engine lock, so IPC
//! commands stay answerable while the app waits for the next network
//! change.

use std::collections::VecDeque;
use std::future::Future;

use araiguma_netlink::{Monitor, Signal};

use crate::engine::DEBOUNCE;

/// A stream of change signals or resynchronization markers.
///
/// Implementations must be cheap to call, must not block, and may return
/// `None` when the source is gone (the engine treats that as end-of-stream;
/// consumers such as the watch loop return on it).
#[async_trait::async_trait]
pub trait ChangeFeed: Send {
    /// The next signal; `None` ends the stream.
    async fn next_signal(&mut self) -> Option<Signal>;
}

/// The netlink monitor wired as a [`ChangeFeed`].
pub struct NetlinkChangeFeed {
    monitor: Monitor,
}

impl NetlinkChangeFeed {
    pub fn new(monitor: Monitor) -> Self {
        Self { monitor }
    }
}

#[async_trait::async_trait]
impl ChangeFeed for NetlinkChangeFeed {
    async fn next_signal(&mut self) -> Option<Signal> {
        Some(self.monitor.next_signal().await)
    }
}

/// The default change feed: the netlink monitor (the only event source
/// implemented so far). The error string is the monitor's own message; the
/// caller adds the user-facing framing.
pub fn default_change_feed() -> Result<NetlinkChangeFeed, String> {
    Ok(NetlinkChangeFeed::new(Monitor::start()?))
}

/// The outcome of absorbing a burst of change notifications.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Burst {
    /// The notification kinds that arrived during the burst, in arrival
    /// order (the first one included).
    pub notified_by: Vec<String>,
    /// A resynchronization marker arrived during the burst: changes during
    /// the gap were NOT observed, and the consumer must resynchronize
    /// instead of claiming a pure diff.
    pub resync: bool,
    /// The monitor re-established its event subscription during the burst.
    /// The consumer must publish recovery only after its fresh observation
    /// has been constructed.
    pub monitor_recovered: bool,
    /// The monitor became unreliable during the burst. The reason is
    /// user-facing and must be surfaced immediately; the change burst
    /// cannot be published as a trusted pure diff after this point.
    pub monitor_degraded: Option<String>,
}

/// Absorb a burst of change notifications (document 01 §4.1 — the 150 ms
/// debounce window, unchanged from the CLI watch loop).
///
/// The first notification has already been received; this waits out the
/// quiet window and collects the rest. The window is a fixed deadline from
/// the first notification (not an extending one), so a continuous stream
/// still collapses into one observation.
///
/// `stop` resolves when the consumer is asked to stop; the burst is then
/// abandoned and `None` is returned, so a shutdown signal is never delayed
/// by the debounce window. A monitor disconnect or failed reconnect ends
/// the burst immediately and is returned as `monitor_degraded`; it must
/// never wait behind the quiet window or be folded into a pure diff.
pub async fn debounce<S>(
    feed: &mut dyn ChangeFeed,
    first_kind: String,
    stop: &mut S,
) -> Option<Burst>
where
    S: Future<Output = ()> + Unpin,
{
    let mut notified_by = vec![first_kind];
    let mut resync = false;
    let mut monitor_recovered = false;
    let deadline = tokio::time::Instant::now() + DEBOUNCE;
    while !resync {
        let next = tokio::select! {
            // Shutdown has priority over a simultaneously-ready feed. Without
            // `biased`, Tokio may choose the feed branch first and return a
            // burst even though the consumer has already asked to stop.
            biased;
            _ = &mut *stop => return None,
            next = tokio::time::timeout_at(deadline, feed.next_signal()) => next,
        };
        match next {
            Ok(Some(Signal::Changed { kind })) => notified_by.push(kind.to_string()),
            Ok(Some(Signal::Resynchronized)) => resync = true,
            Ok(Some(Signal::Disconnected)) => {
                // Degradation is a control fact, not part of the change
                // burst. Return it immediately so the consumer can mark the
                // live view stale before any reconnect/recovery work.
                return Some(Burst {
                    notified_by,
                    resync: false,
                    monitor_recovered: false,
                    monitor_degraded: Some(
                        "the netlink event monitor disconnected; reconnecting".to_string(),
                    ),
                });
            }
            Ok(Some(Signal::ReconnectFailed { reason })) => {
                return Some(Burst {
                    notified_by,
                    resync: false,
                    monitor_recovered: false,
                    monitor_degraded: Some(format!(
                        "the netlink event monitor reconnect failed: {reason}"
                    )),
                });
            }
            Ok(Some(Signal::Reconnected)) => {
                resync = true;
                monitor_recovered = true;
            }
            Ok(None) => return None,
            Err(_elapsed) => break, // quiet window over
        }
    }
    Some(Burst {
        notified_by,
        resync,
        monitor_recovered,
        monitor_degraded: None,
    })
}

/// Scripted feed for tests: pops signals off a queue.
pub struct FakeFeed {
    signals: VecDeque<Signal>,
    end_on_empty: bool,
}

impl FakeFeed {
    /// A finite scripted feed. Exhausting the script ends the stream.
    pub fn new(signals: Vec<Signal>) -> Self {
        Self {
            signals: signals.into(),
            end_on_empty: true,
        }
    }

    /// A scripted feed that stays pending after its script is exhausted.
    /// Use this when a test is exercising the debounce quiet window rather
    /// than end-of-stream behavior.
    pub fn pending_after_script(signals: Vec<Signal>) -> Self {
        Self {
            signals: signals.into(),
            end_on_empty: false,
        }
    }
}

#[async_trait::async_trait]
impl ChangeFeed for FakeFeed {
    async fn next_signal(&mut self) -> Option<Signal> {
        match self.signals.pop_front() {
            Some(signal) => Some(signal),
            None if self.end_on_empty => None,
            None => std::future::pending::<Option<Signal>>().await,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use araiguma_netlink::Signal;

    #[tokio::test]
    async fn debounce_collects_the_burst() {
        let mut feed = FakeFeed::pending_after_script(vec![
            Signal::Changed { kind: "newRoute" },
            Signal::Changed { kind: "newAddress" },
        ]);
        let mut stop = std::pin::pin!(std::future::pending::<()>());
        let burst = debounce(&mut feed, "newLink".to_string(), &mut stop)
            .await
            .expect("burst");
        assert_eq!(
            burst.notified_by,
            vec![
                "newLink".to_string(),
                "newRoute".to_string(),
                "newAddress".to_string()
            ]
        );
        assert!(!burst.resync);
        assert!(!burst.monitor_recovered);
    }

    #[tokio::test]
    async fn debounce_reports_a_resync_inside_the_window() {
        let mut feed = FakeFeed::new(vec![Signal::Resynchronized]);
        let mut stop = std::pin::pin!(std::future::pending::<()>());
        let burst = debounce(&mut feed, "newLink".to_string(), &mut stop)
            .await
            .expect("burst");
        assert!(burst.resync);
        assert!(!burst.monitor_recovered);
    }

    #[tokio::test]
    async fn debounce_surfaces_a_disconnect_immediately() {
        let mut feed = FakeFeed::new(vec![
            Signal::Disconnected,
            Signal::ReconnectFailed {
                reason: "synthetic failure".to_string(),
            },
            Signal::Reconnected,
        ]);
        let mut stop = std::pin::pin!(std::future::pending::<()>());
        let burst = debounce(&mut feed, "newLink".to_string(), &mut stop)
            .await
            .expect("burst");
        assert!(!burst.resync);
        assert!(!burst.monitor_recovered);
        assert!(burst
            .monitor_degraded
            .as_deref()
            .is_some_and(|reason| reason.contains("disconnected")));
    }

    #[tokio::test]
    async fn debounce_ends_when_the_feed_ends_inside_the_window() {
        let mut feed = FakeFeed::new(Vec::new());
        let mut stop = std::pin::pin!(std::future::pending::<()>());
        assert!(debounce(&mut feed, "newLink".to_string(), &mut stop)
            .await
            .is_none());
    }

    #[tokio::test]
    async fn debounce_returns_none_when_stopped() {
        let mut feed = FakeFeed::new(vec![Signal::Changed { kind: "newRoute" }]);
        let mut stop = std::pin::pin!(async {});
        assert!(debounce(&mut feed, "newLink".to_string(), &mut stop)
            .await
            .is_none());
    }
}
