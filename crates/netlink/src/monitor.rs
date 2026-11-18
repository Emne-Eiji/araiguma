//! Multicast rtnetlink monitoring with reconnection and resynchronization.
//!
//! The monitor subscribes to the link/address/route rtnetlink multicast
//! groups (equivalent to `ip monitor`) and yields a [`Signal`] whenever the
//! kernel reports a relevant change. If the connection dies, the monitor
//! reconnects with exponential backoff and reports
//! [`Signal::Resynchronized`], telling consumers to re-dump state instead of
//! trusting an event stream that may have missed changes while offline.
//! The same signal is reported when the kernel DROPS notifications because
//! our receive buffer overflowed (ENOBUFS, surfaced by the netlink codec as
//! an `NLMSG_OVERRUN` message): dropped events are unobserved, and the
//! monitor never lets the stream pretend it is current.

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
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Signal {
    /// The kernel reported a change of this kind ("newLink", "newRoute",
    /// ...). Notifications that do not map to a relevant change are
    /// filtered out internally and never surface here.
    Changed { kind: &'static str },
    /// The observation stream cannot be trusted to be complete right now:
    /// either the connection died and was re-established, or the kernel
    /// DROPPED notifications because our receive buffer overflowed
    /// (ENOBUFS → NLMSG_OVERRUN). Changes that happened in between were
    /// NOT observed; consumers must re-dump state — never claim their
    /// current observations are fresh.
    Resynchronized,
}

/// A multicast rtnetlink monitor. Create with [`Monitor::start`].
pub struct Monitor {
    groups: Vec<MulticastGroup>,
    receiver: Option<UnboundedReceiver<(NetlinkMessage<RouteNetlinkMessage>, SocketAddr)>>,
    backoff: Duration,
}

impl Monitor {
    /// Start monitoring the default groups. Must be called inside a tokio
    /// runtime (the connection task is spawned onto it).
    pub fn start() -> Result<Self, String> {
        Self::with_groups(MONITOR_GROUPS.to_vec())
    }

    /// Start monitoring specific groups (used by tests).
    pub fn with_groups(groups: Vec<MulticastGroup>) -> Result<Self, String> {
        match new_multicast_connection(&groups) {
            Ok((connection, _handle, receiver)) => {
                tokio::spawn(connection);
                Ok(Self {
                    groups,
                    receiver: Some(receiver),
                    backoff: INITIAL_BACKOFF,
                })
            }
            Err(error) => Err(format!("netlink multicast socket: {error}")),
        }
    }

    /// Wait for the next relevant signal. Reconnects (with exponential
    /// backoff) whenever the connection dies, and reports
    /// [`Signal::Resynchronized`] after doing so — and also when the kernel
    /// reports a receive-buffer overrun (dropped notifications).
    pub async fn next_signal(&mut self) -> Signal {
        loop {
            let Some(receiver) = self.receiver.as_mut() else {
                self.reconnect().await;
                return Signal::Resynchronized;
            };
            match receiver.next().await {
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
                    // The connection task died: channel closed.
                    self.receiver = None;
                    self.reconnect().await;
                    return Signal::Resynchronized;
                }
            }
        }
    }

    async fn reconnect(&mut self) {
        loop {
            log::debug!("netlink monitor reconnecting (backoff {:?})", self.backoff);
            tokio::time::sleep(self.backoff).await;
            self.backoff = (self.backoff * 2).min(MAX_BACKOFF);
            match new_multicast_connection(&self.groups) {
                Ok((connection, _handle, receiver)) => {
                    tokio::spawn(connection);
                    self.receiver = Some(receiver);
                    self.backoff = INITIAL_BACKOFF;
                    return;
                }
                Err(error) => {
                    log::warn!("netlink monitor reconnect failed: {error}");
                }
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
    async fn a_closed_channel_reports_resynchronized() {
        // Drop the sender: next_signal must detect the dead connection,
        // reconnect (a real socket is created and immediately usable in
        // the test environment), and report Resynchronized.
        let (sender, receiver) = mpsc::unbounded();
        let mut monitor = monitor_with_receiver(receiver);
        drop(sender);
        let signal = tokio::time::timeout(Duration::from_secs(5), monitor.next_signal())
            .await
            .expect("resynchronization within 5 seconds");
        assert_eq!(signal, Signal::Resynchronized);
    }
}
