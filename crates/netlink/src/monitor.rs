//! Multicast rtnetlink monitoring with reconnection and resynchronization.
//!
//! The monitor subscribes to the link/address/route rtnetlink multicast
//! groups (equivalent to `ip monitor`) and yields a [`Signal`] whenever the
//! kernel reports a relevant change. If the connection dies, the monitor
//! reconnects with exponential backoff and reports
//! [`Signal::Resynchronized`], telling consumers to re-dump state instead of
//! trusting an event stream that may have missed changes while offline.

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
    /// The subscription was re-established after a connection failure (or a
    /// forced resynchronization). Changes that happened while disconnected
    /// were NOT observed; consumers must re-dump state.
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
    /// [`Signal::Resynchronized`] after doing so.
    pub async fn next_signal(&mut self) -> Signal {
        loop {
            let Some(receiver) = self.receiver.as_mut() else {
                self.reconnect().await;
                return Signal::Resynchronized;
            };
            match receiver.next().await {
                Some((message, _addr)) => {
                    if let NetlinkPayload::InnerMessage(inner) = message.payload {
                        if let Some(kind) = classify_notification(&inner) {
                            return Signal::Changed { kind };
                        }
                    }
                    // ACKs and unclassified messages keep us listening.
                }
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
