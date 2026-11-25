//! The three approved NetworkManager signals.
//!
//! Signals are receive-only. The foundation subscribes to exactly
//! `StateChanged`, `DeviceAdded`, and `DeviceRemoved` on the NetworkManager
//! manager object. It never subscribes to `PropertiesChanged`,
//! `NameOwnerChanged`, or any other signal, and systemd-resolved has no
//! signal subscription at all.
//!
//! Two independent checks protect delivery:
//!
//! 1. The daemon-side match rule pins the well-known sender, the manager
//!    object path, the manager interface, and one exact member.
//! 2. Every delivered message is validated locally against the unique owner
//!    captured by the ownership preflight, plus the same path, interface,
//!    member, and expected body signature. A message that fails any check is
//!    rejected with [`DbusError::RejectedSignal`].

use std::fmt;
use std::num::NonZeroUsize;
use std::time::Duration;

use futures::StreamExt;
use zbus::message::{Message, Type};
use zbus::names::OwnedUniqueName;
use zbus::{Connection, MatchRule, MessageStream, OwnedObjectPath};

use crate::error::DbusError;
use crate::policy::{
    is_device_object_path, NETWORK_MANAGER_MANAGER_INTERFACE, NETWORK_MANAGER_MANAGER_PATH,
    NETWORK_MANAGER_SERVICE,
};

/// The complete set of approved NetworkManager signal members.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SignalKind {
    StateChanged,
    DeviceAdded,
    DeviceRemoved,
}

impl SignalKind {
    /// The exact D-Bus member name.
    pub const fn member(self) -> &'static str {
        match self {
            Self::StateChanged => "StateChanged",
            Self::DeviceAdded => "DeviceAdded",
            Self::DeviceRemoved => "DeviceRemoved",
        }
    }

    /// The exact D-Bus body signature.
    pub const fn signature(self) -> &'static str {
        match self {
            Self::StateChanged => "u",
            Self::DeviceAdded => "o",
            Self::DeviceRemoved => "o",
        }
    }
}

/// One validated NetworkManager signal.
///
/// Device paths are part of the provider payload, not of diagnostics; the
/// `Debug` representation redacts them.
#[derive(Clone, PartialEq, Eq)]
pub enum SignalEvent {
    StateChanged(u32),
    DeviceAdded(OwnedObjectPath),
    DeviceRemoved(OwnedObjectPath),
}

impl SignalEvent {
    /// Which approved signal this event is.
    pub fn kind(&self) -> SignalKind {
        match self {
            Self::StateChanged(_) => SignalKind::StateChanged,
            Self::DeviceAdded(_) => SignalKind::DeviceAdded,
            Self::DeviceRemoved(_) => SignalKind::DeviceRemoved,
        }
    }

    /// The manager state value, for [`SignalKind::StateChanged`].
    pub fn state(&self) -> Option<u32> {
        match self {
            Self::StateChanged(state) => Some(*state),
            _ => None,
        }
    }

    /// The device object path, for `DeviceAdded`/`DeviceRemoved`.
    pub fn device_path(&self) -> Option<&str> {
        match self {
            Self::DeviceAdded(path) | Self::DeviceRemoved(path) => Some(path.as_str()),
            Self::StateChanged(_) => None,
        }
    }
}

impl fmt::Debug for SignalEvent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::StateChanged(state) => f.debug_tuple("StateChanged").field(state).finish(),
            Self::DeviceAdded(_) => f.write_str("DeviceAdded(<redacted>)"),
            Self::DeviceRemoved(_) => f.write_str("DeviceRemoved(<redacted>)"),
        }
    }
}

/// A live subscription to the approved signals.
///
/// Dropping the subscription removes the daemon-side match rules
/// asynchronously; closing the owning connection removes them immediately.
#[must_use = "subscriptions do nothing unless polled"]
pub struct SignalSubscription {
    owner: OwnedUniqueName,
    streams: Vec<(SignalKind, MessageStream)>,
}

impl SignalSubscription {
    /// Wait for the next validated signal.
    ///
    /// There is deliberately no built-in deadline: providers choose their own
    /// bounded wait. A connection failure is reported as
    /// [`DbusError::BusUnavailable`]; a message that fails local validation is
    /// reported as [`DbusError::RejectedSignal`] and the subscription remains
    /// usable for the next message.
    pub async fn next(&mut self) -> Result<SignalEvent, DbusError> {
        let futures = self
            .streams
            .iter_mut()
            .map(|(kind, stream)| {
                let kind = *kind;
                Box::pin(async move { (kind, stream.next().await) })
            })
            .collect::<Vec<_>>();
        let ((kind, item), _, _) = futures::future::select_all(futures).await;

        match item {
            Some(Ok(message)) => validate_signal(&self.owner, kind, &message),
            Some(Err(_)) | None => Err(DbusError::BusUnavailable),
        }
    }
}

impl fmt::Debug for SignalSubscription {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SignalSubscription")
            .field(
                "signals",
                &self
                    .streams
                    .iter()
                    .map(|(kind, _)| kind)
                    .collect::<Vec<_>>(),
            )
            .finish_non_exhaustive()
    }
}

pub(crate) async fn subscribe(
    connection: &Connection,
    owner: &OwnedUniqueName,
    kinds: &[SignalKind],
    timeout: Duration,
) -> Result<SignalSubscription, DbusError> {
    if kinds.is_empty() {
        return Err(DbusError::UnsupportedRequest);
    }

    let mut seen = Vec::with_capacity(kinds.len());
    for kind in kinds {
        if seen.contains(kind) {
            return Err(DbusError::UnsupportedRequest);
        }
        seen.push(*kind);
    }

    let capacity = NonZeroUsize::new(16).expect("non-zero signal queue capacity");
    let mut streams = Vec::with_capacity(kinds.len());
    for kind in kinds {
        let rule = MatchRule::builder()
            .msg_type(Type::Signal)
            .sender(NETWORK_MANAGER_SERVICE)
            .path(NETWORK_MANAGER_MANAGER_PATH)
            .interface(NETWORK_MANAGER_MANAGER_INTERFACE)
            .member(kind.member())
            .build()
            .map_err(|_| DbusError::Protocol)?;

        let stream = match tokio::time::timeout(
            timeout,
            MessageStream::for_match_rule(rule, connection, Some(capacity)),
        )
        .await
        {
            Ok(Ok(stream)) => stream,
            Ok(Err(error)) => return Err(DbusError::from(error)),
            Err(_) => return Err(DbusError::Timeout),
        };
        streams.push((*kind, stream));
    }

    Ok(SignalSubscription {
        owner: owner.clone(),
        streams,
    })
}

fn validate_signal(
    owner: &OwnedUniqueName,
    expected: SignalKind,
    message: &Message,
) -> Result<SignalEvent, DbusError> {
    if message.message_type() != Type::Signal {
        return Err(DbusError::RejectedSignal);
    }

    let header = message.header();
    if header.sender().map(|sender| sender.as_str()) != Some(owner.as_str())
        || header.path().map(|path| path.as_str()) != Some(NETWORK_MANAGER_MANAGER_PATH)
        || header.interface().map(|interface| interface.as_str())
            != Some(NETWORK_MANAGER_MANAGER_INTERFACE)
        || header.member().map(|member| member.as_str()) != Some(expected.member())
        || header.signature().to_string() != expected.signature()
    {
        return Err(DbusError::RejectedSignal);
    }

    match expected {
        SignalKind::StateChanged => message
            .body()
            .deserialize::<u32>()
            .map(SignalEvent::StateChanged)
            .map_err(|_| DbusError::RejectedSignal),
        SignalKind::DeviceAdded => {
            let path: OwnedObjectPath = message
                .body()
                .deserialize()
                .map_err(|_| DbusError::RejectedSignal)?;
            if !is_device_object_path(path.as_str()) {
                return Err(DbusError::RejectedSignal);
            }
            Ok(SignalEvent::DeviceAdded(path))
        }
        SignalKind::DeviceRemoved => {
            let path: OwnedObjectPath = message
                .body()
                .deserialize()
                .map_err(|_| DbusError::RejectedSignal)?;
            if !is_device_object_path(path.as_str()) {
                return Err(DbusError::RejectedSignal);
            }
            Ok(SignalEvent::DeviceRemoved(path))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{validate_signal, SignalEvent, SignalKind};
    use crate::error::DbusError;
    use zbus::names::OwnedUniqueName;
    use zbus::Message;

    const OWNER: &str = ":1.42";
    const PATH: &str = "/org/freedesktop/NetworkManager";
    const INTERFACE: &str = "org.freedesktop.NetworkManager";

    fn owner() -> OwnedUniqueName {
        OwnedUniqueName::try_from(OWNER).expect("valid unique name")
    }

    fn signal(member: &str, body: &(impl serde::Serialize + zbus::DynamicType)) -> Message {
        Message::signal(PATH, INTERFACE, member)
            .sender(OWNER)
            .build(body)
            .expect("build signal")
    }

    #[test]
    fn accepts_exact_state_changed() {
        let message = signal("StateChanged", &(70u32,));
        assert_eq!(
            validate_signal(&owner(), SignalKind::StateChanged, &message),
            Ok(SignalEvent::StateChanged(70))
        );
    }

    #[test]
    fn accepts_exact_device_signals_with_canonical_paths() {
        let path = "/org/freedesktop/NetworkManager/Devices/3";
        let message = signal(
            "DeviceAdded",
            &(zbus::OwnedObjectPath::try_from(path).expect("path")),
        );
        assert_eq!(
            validate_signal(&owner(), SignalKind::DeviceAdded, &message),
            Ok(SignalEvent::DeviceAdded(
                zbus::OwnedObjectPath::try_from(path).expect("path")
            ))
        );

        let message = signal(
            "DeviceRemoved",
            &(zbus::OwnedObjectPath::try_from(path).expect("path")),
        );
        assert_eq!(
            validate_signal(&owner(), SignalKind::DeviceRemoved, &message),
            Ok(SignalEvent::DeviceRemoved(
                zbus::OwnedObjectPath::try_from(path).expect("path")
            ))
        );
    }

    #[test]
    fn rejects_wrong_sender_path_interface_member_and_type() {
        let message = signal("StateChanged", &(70u32,));
        assert_eq!(
            validate_signal(
                &OwnedUniqueName::try_from(":1.43").expect("name"),
                SignalKind::StateChanged,
                &message
            ),
            Err(DbusError::RejectedSignal)
        );

        for bad in [
            Message::signal(
                "/org/freedesktop/NetworkManager/Devices/1",
                INTERFACE,
                "StateChanged",
            )
            .sender(OWNER)
            .build(&(70u32,))
            .expect("signal"),
            Message::signal(
                PATH,
                "org.freedesktop.NetworkManager.Device",
                "StateChanged",
            )
            .sender(OWNER)
            .build(&(70u32,))
            .expect("signal"),
            Message::signal(PATH, INTERFACE, "DeviceAdded")
                .sender(OWNER)
                .build(&(70u32,))
                .expect("signal"),
            Message::method_call(PATH, "StateChanged")
                .sender(OWNER)
                .build(&(70u32,))
                .expect("call"),
        ] {
            assert_eq!(
                validate_signal(&owner(), SignalKind::StateChanged, &bad),
                Err(DbusError::RejectedSignal)
            );
        }
    }

    #[test]
    fn rejects_wrong_body_signatures_and_non_canonical_device_paths() {
        let wrong_body = signal("StateChanged", &("70",));
        assert_eq!(
            validate_signal(&owner(), SignalKind::StateChanged, &wrong_body),
            Err(DbusError::RejectedSignal)
        );

        for path in [
            "/org/freedesktop/NetworkManager/Devices/03",
            "/org/freedesktop/NetworkManager/Devices/3/extra",
            "/org/freedesktop/NetworkManager/Devices/three",
            "/org/freedesktop/NetworkManager/Devices/3evil",
            "/tmp/elsewhere",
        ] {
            let message = signal(
                "DeviceAdded",
                &(zbus::OwnedObjectPath::try_from(path).expect("path")),
            );
            assert_eq!(
                validate_signal(&owner(), SignalKind::DeviceAdded, &message),
                Err(DbusError::RejectedSignal),
                "path {path:?}"
            );
        }
    }

    #[test]
    fn event_debug_redacts_device_paths() {
        let path = "/org/freedesktop/NetworkManager/Devices/12";
        let event = SignalEvent::DeviceAdded(zbus::OwnedObjectPath::try_from(path).expect("path"));
        let text = format!("{event:?}");
        assert!(!text.contains(path));
        assert!(!text.contains("/org/"));
        assert!(text.contains("<redacted>"));
        assert_eq!(event.kind(), SignalKind::DeviceAdded);
        assert_eq!(event.device_path(), Some(path));
        assert_eq!(event.state(), None);
    }
}
