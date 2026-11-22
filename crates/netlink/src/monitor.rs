//! Multicast rtnetlink monitoring with reconnection and resynchronization.
//!
//! The monitor subscribes to the link/address/route rtnetlink multicast
//! groups (equivalent to `ip monitor`) and yields a [`Signal`] whenever the
//! kernel reports a relevant change. If the connection dies, the monitor
//! reports [`Signal::Disconnected`] immediately, then retries with
//! exponential backoff. Each failed attempt reports
//! [`Signal::ReconnectFailed`]; a successful attempt reports
//! [`Signal::Reconnected`]. Consumers can therefore stop claiming that live
//! event delivery is reliable while the stream is down and only restore
//! that claim after re-observing the system.
//!
//! A receive-buffer overrun (ENOBUFS, surfaced by the netlink codec as an
//! `NLMSG_OVERRUN` message) reports [`Signal::Resynchronized`]: dropped
//! events are unobserved, and the monitor never lets the stream pretend it
//! is current.

use std::sync::Arc;
use std::time::Duration;

use futures::channel::mpsc::UnboundedReceiver;
use futures::StreamExt;
use rtnetlink::packet_core::{NetlinkMessage, NetlinkPayload};
use rtnetlink::packet_route::RouteNetlinkMessage;
use rtnetlink::sys::SocketAddr;
use rtnetlink::{new_multicast_connection, MulticastGroup};

use crate::translate::classify_notification;

/// The change groups Araiguma listens to.
pub const MONITOR_GROUPS: [MulticastGroup; 5] = [
    MulticastGroup::Link,
    MulticastGroup::Ipv4Ifaddr,
    MulticastGroup::Ipv6Ifaddr,
    MulticastGroup::Ipv4Route,
    MulticastGroup::Ipv6Route,
];

const INITIAL_BACKOFF: Duration = Duration::from_millis(250);
const MAX_BACKOFF: Duration = Duration::from_secs(8);

/// A monitor signal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Signal {
    /// The kernel reported a change of this kind ("newLink", "newRoute",
    /// ...). Notifications that do not map to a relevant change are
    /// filtered out internally and never surface here.
    Changed { kind: &'static str },
    /// The observation stream cannot be trusted to be complete right now:
    /// the kernel DROPPED notifications because our receive buffer overflowed
    /// (ENOBUFS → NLMSG_OVERRUN), or a forced resynchronization completed.
    /// Changes that happened in between were NOT observed; consumers must
    /// re-dump state — never claim their current observations are fresh.
    Resynchronized,
    /// The multicast event stream ended. No later event is reliable until a
    /// reconnect succeeds; consumers must immediately mark live delivery as
    /// degraded while the monitor reconnects.
    Disconnected,
    /// One reconnect attempt failed. The monitor remains degraded and will
    /// retry with exponential backoff.
    ReconnectFailed { reason: String },
    /// The multicast subscription was re-established. The consumer must
    /// re-observe the system before claiming that live delivery is current.
    Reconnected,
}

type MonitorReceiver = UnboundedReceiver<(NetlinkMessage<RouteNetlinkMessage>, SocketAddr)>;
type ReconnectFn = dyn Fn(&[MulticastGroup]) -> Result<MonitorReceiver, String> + Send + Sync;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ReconnectMode {
    /// A live subscription was lost and must be re-established.
    Lost,
    /// A resynchronization was requested explicitly; no health degradation
    /// is reported because the subscription was not unexpectedly lost.
    Forced,
}

fn connect_multicast(groups: &[MulticastGroup]) -> Result<MonitorReceiver, String> {
    match new_multicast_connection(groups) {
        Ok((connection, _handle, receiver)) => {
            tokio::spawn(connection);
            Ok(receiver)
        }
        Err(error) => Err(format!("netlink multicast socket: {error}")),
    }
}

/// A multicast rtnetlink monitor. Create with [`Monitor::start`].
pub struct Monitor {
    groups: Vec<MulticastGroup>,
    receiver: Option<MonitorReceiver>,
    backoff: Duration,
    reconnect: Arc<ReconnectFn>,
    reconnect_mode: ReconnectMode,
}

impl Monitor {
    /// Start monitoring the default groups. Must be called inside a tokio
    /// runtime (the connection task is spawned onto it).
    pub fn start() -> Result<Self, String> {
        Self::with_groups(MONITOR_GROUPS.to_vec())
    }

    /// Start monitoring specific groups (used by tests).
    pub fn with_groups(groups: Vec<MulticastGroup>) -> Result<Self, String> {
        let receiver = connect_multicast(&groups)?;
        Ok(Self {
            groups,
            receiver: Some(receiver),
            backoff: INITIAL_BACKOFF,
            reconnect: Arc::new(connect_multicast),
            reconnect_mode: ReconnectMode::Lost,
        })
    }

    /// Wait for the next relevant signal. A lost multicast subscription is
    /// reported as [`Signal::Disconnected`] immediately. Each later call
    /// makes one backoff-delayed reconnect attempt, reporting
    /// [`Signal::ReconnectFailed`] or [`Signal::Reconnected`]. The kernel
    /// receive-buffer overrun path remains [`Signal::Resynchronized`].
    pub async fn next_signal(&mut self) -> Signal {
        loop {
            let message = {
                let Some(receiver) = self.receiver.as_mut() else {
                    return self.reconnect_signal().await;
                };
                receiver.next().await
            };

            match message {
                Some((message, _addr)) => match message.payload {
                    NetlinkPayload::InnerMessage(inner) => {
                        if let Some(kind) = classify_notification(&inner) {
                            return Signal::Changed { kind };
                        }
                        // ACKs and unclassified messages keep us listening.
                    }
                    // The kernel dropped notifications because our receive
                    // buffer overflowed (the netlink codec surfaces ENOBUFS
                    // as an NLMSG_OVERRUN message) — the events that
                    // overflowed were NOT observed. Report Resynchronized so
                    // consumers re-dump state; the stream must never pretend
                    // it is current.
                    NetlinkPayload::Overrun(_) => return Signal::Resynchronized,
                    _ => {
                        // ACKs, errors, and other payloads keep us listening.
                    }
                },
                None => {
                    // The connection task died: channel closed. Report the
                    // loss before attempting a reconnect so a consumer can
                    // stop claiming that event delivery is live.
                    self.receiver = None;
                    self.reconnect_mode = ReconnectMode::Lost;
                    return Signal::Disconnected;
                }
            }
        }
    }

    async fn reconnect_signal(&mut self) -> Signal {
        let mode = self.reconnect_mode;
        match self.try_reconnect().await {
            Ok(()) if mode == ReconnectMode::Forced => Signal::Resynchronized,
            Ok(()) => Signal::Reconnected,
            Err(reason) => Signal::ReconnectFailed { reason },
        }
    }

    async fn try_reconnect(&mut self) -> Result<(), String> {
        log::debug!("netlink monitor reconnecting (backoff {:?})", self.backoff);
        tokio::time::sleep(self.backoff).await;
        match (self.reconnect)(&self.groups) {
            Ok(receiver) => {
                self.receiver = Some(receiver);
                self.backoff = INITIAL_BACKOFF;
                self.reconnect_mode = ReconnectMode::Lost;
                Ok(())
            }
            Err(error) => {
                log::warn!("netlink monitor reconnect failed: {error}");
                self.backoff = (self.backoff * 2).min(MAX_BACKOFF);
                Err(error)
            }
        }
    }

    /// Force a resynchronization on the next [`Monitor::next_signal`]
    /// call: the current subscription is torn down, re-established, and
    /// reported as [`Signal::Resynchronized`]. Used by tests and by the
    /// watch loop as a manual refresh.
    pub fn force_resync(&mut self) {
        // Dropping the receiver closes the channel; the connection task
        // observes the closed channel and terminates.
        self.receiver = None;
        self.reconnect_mode = ReconnectMode::Forced;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::channel::mpsc;

    /// A monitor wired to a synthetic channel, so message classification
    /// (including the ENOBUFS overrun path) is unit-testable without a
    /// socket. The channel's messages flow through the same
    /// [`Monitor::next_signal`] logic as real ones.
    fn monitor_with_receiver(
        receiver: UnboundedReceiver<(NetlinkMessage<RouteNetlinkMessage>, SocketAddr)>,
    ) -> Monitor {
        Monitor {
            groups: Vec::new(),
            receiver: Some(receiver),
            backoff: INITIAL_BACKOFF,
            reconnect: Arc::new(connect_multicast),
            reconnect_mode: ReconnectMode::Lost,
        }
    }

    fn overrun_message() -> NetlinkMessage<RouteNetlinkMessage> {
        NetlinkMessage::new(
            rtnetlink::packet_core::NetlinkHeader::default(),
            NetlinkPayload::Overrun(Vec::new()),
        )
    }

    fn inner_message() -> NetlinkMessage<RouteNetlinkMessage> {
        NetlinkMessage::new(
            rtnetlink::packet_core::NetlinkHeader::default(),
            NetlinkPayload::InnerMessage(rtnetlink::packet_route::RouteNetlinkMessage::NewLink(
                netlink_packet_route::link::LinkMessage::default(),
            )),
        )
    }

    #[tokio::test]
    async fn overrun_reports_resynchronized() {
        let (sender, receiver) = mpsc::unbounded();
        sender
            .unbounded_send((overrun_message(), SocketAddr::new(0, 0)))
            .expect("send");
        drop(sender);
        let mut monitor = monitor_with_receiver(receiver);
        assert_eq!(monitor.next_signal().await, Signal::Resynchronized);
    }

    #[tokio::test]
    async fn classified_inner_messages_still_report_changes() {
        let (sender, receiver) = mpsc::unbounded();
        sender
            .unbounded_send((inner_message(), SocketAddr::new(0, 0)))
            .expect("send");
        drop(sender);
        let mut monitor = monitor_with_receiver(receiver);
        assert_eq!(
            monitor.next_signal().await,
            Signal::Changed { kind: "newLink" }
        );
    }

    #[tokio::test]
    async fn a_closed_channel_reports_disconnect_then_reconnect() {
        // Dropping the sender proves that the loss is reported before any
        // reconnect work begins. The following call establishes a real
        // socket and reports that the event stream was restored.
        let (sender, receiver) = mpsc::unbounded();
        let mut monitor = monitor_with_receiver(receiver);
        drop(sender);

        let disconnected = tokio::time::timeout(Duration::from_secs(1), monitor.next_signal())
            .await
            .expect("disconnect reported within 1 second");
        assert_eq!(disconnected, Signal::Disconnected);

        let reconnected = tokio::time::timeout(Duration::from_secs(5), monitor.next_signal())
            .await
            .expect("reconnect within 5 seconds");
        assert_eq!(reconnected, Signal::Reconnected);
    }

    #[tokio::test]
    async fn a_failed_reconnect_remains_degraded_until_recovery() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        use std::sync::Mutex;

        let (dead_sender, dead_receiver) = mpsc::unbounded();
        drop(dead_sender);

        let (live_sender, live_receiver) = mpsc::unbounded();
        let replacement = Arc::new(Mutex::new(Some(live_receiver)));
        let attempts = Arc::new(AtomicUsize::new(0));
        let attempts_for_closure = attempts.clone();
        let replacement_for_closure = replacement.clone();
        let reconnect: Arc<ReconnectFn> = Arc::new(move |_| {
            let attempt = attempts_for_closure.fetch_add(1, Ordering::SeqCst);
            if attempt == 0 {
                Err("synthetic reconnect failure".to_string())
            } else {
                replacement_for_closure
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner())
                    .take()
                    .ok_or_else(|| "replacement receiver already used".to_string())
            }
        });

        let mut monitor = Monitor {
            groups: Vec::new(),
            receiver: Some(dead_receiver),
            backoff: Duration::ZERO,
            reconnect,
            reconnect_mode: ReconnectMode::Lost,
        };

        assert_eq!(monitor.next_signal().await, Signal::Disconnected);
        assert_eq!(
            monitor.next_signal().await,
            Signal::ReconnectFailed {
                reason: "synthetic reconnect failure".to_string(),
            }
        );
        assert_eq!(monitor.next_signal().await, Signal::Reconnected);

        live_sender
            .unbounded_send((inner_message(), SocketAddr::new(0, 0)))
            .expect("send after recovery");
        assert_eq!(
            monitor.next_signal().await,
            Signal::Changed { kind: "newLink" }
        );
    }
}
