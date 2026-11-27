//! # araiguma-provider-nm
//!
//! The read-only NetworkManager discovery provider.
//!
//! The provider observes exactly the approved WO-06C surface:
//!
//! * the manager's `ActiveConnections` list, and each active connection's
//!   `Devices`, `State`, `Type`, and `Vpn` properties on the interface real
//!   NetworkManager exports for active connections
//!   (`org.freedesktop.NetworkManager.Connection.Active`; WO-06C-C1),
//! * the device/interface mapping needed to resolve each active connection's
//!   devices to kernel ifindexes, and
//! * each active connection's state, type, and VPN boolean.
//!
//! It deliberately does not read hardware addresses, IP configuration,
//! connection settings or secrets, Wi-Fi data, display names, UUIDs, VPN type,
//! or manager metadata. D-Bus access goes only through
//! [`araiguma_provider_dbus::DbusFoundation`], so the strict property
//! allow-list, `NO_AUTO_START` flag, disabled property caching, ownership
//! preflight, and signal validation are all inherited rather than
//! reimplemented.
//!
//! Construction is side-effect-free. The first [`DiscoveryProvider::observe`]
//! call connects, preflights ownership, subscribes to the three approved
//! signals, drains any pending signals, and then performs a fresh property
//! observation. Signals are a refresh hint only; they never replace the
//! property read.
//!
//! The provider is optional. A missing bus or service produces
//! [`SectionResult::Unavailable`] for the connections section; a permission
//! failure produces [`SectionResult::PermissionDenied`]; a bounded timeout or
//! malformed reply produces a sanitized [`SectionResult::Failed`] code. A
//! failed refresh never leaves a previous observation in place.

#![forbid(unsafe_code)]

use std::fmt;
use std::path::{Path, PathBuf};
use std::time::Duration;

use araiguma_core::model::{ConnectionObservation, ConnectionState, ObservationSource};
use araiguma_core::provider::{DiscoveryProvider, ProviderId, ProviderObservation, SectionResult};
use araiguma_provider_dbus::policy::{
    NETWORK_MANAGER_ACTIVE_CONNECTION_INTERFACE, NETWORK_MANAGER_DEVICE_INTERFACE,
    NETWORK_MANAGER_MANAGER_INTERFACE, NETWORK_MANAGER_MANAGER_PATH,
};
use araiguma_provider_dbus::{
    DbusError, DbusFoundation, PropertyRequest, Service, SignalKind, SignalSubscription,
};
use async_trait::async_trait;
use zbus::OwnedObjectPath;

/// Default bounded deadline for connect, preflight, signal setup, and each
/// property read.
pub const DEFAULT_OPERATION_TIMEOUT: Duration = Duration::from_secs(5);

/// How long the first signal drain waits for a pending signal.
const SIGNAL_DRAIN_TIMEOUT: Duration = Duration::from_millis(20);

/// Upper bound on signals drained in one observation. Repeated signals are
/// coalesced into the single fresh read that follows.
const MAX_DRAINED_SIGNALS: usize = 64;

const NOTE_PARTIAL_CONNECTION_READ: &str = "partialConnectionRead";
const NOTE_UNRESOLVED_DEVICE: &str = "unresolvedDevice";
const NOTE_POLLING_AND_APPROVED_SIGNALS: &str = "pollingAndApprovedSignals";

const ERROR_TIMEOUT: &str = "timeout";
const ERROR_MALFORMED: &str = "malformedResponse";
const ERROR_PARTIAL: &str = "partialConnectionRead";
const ERROR_INTERNAL: &str = "internalError";

/// The NetworkManager discovery provider.
pub struct NetworkManagerProvider {
    sys_root: PathBuf,
    timeout: Duration,
    connection: Option<DbusFoundation>,
    subscription: Option<SignalSubscription>,
    drained_signals: usize,
}

impl NetworkManagerProvider {
    /// Construct a provider against the real `/sys` tree.
    ///
    /// This performs no I/O and opens no connection.
    pub fn new() -> Self {
        Self::with_sys_root("/sys")
    }

    /// Construct a provider with an injectable sysfs root (tests/fixtures).
    pub fn with_sys_root(sys_root: impl Into<PathBuf>) -> Self {
        Self::with_sys_root_and_timeout(sys_root, DEFAULT_OPERATION_TIMEOUT)
    }

    /// Construct a provider with an injectable sysfs root and bounded timeout.
    pub fn with_sys_root_and_timeout(sys_root: impl Into<PathBuf>, timeout: Duration) -> Self {
        Self {
            sys_root: sys_root.into(),
            timeout,
            connection: None,
            subscription: None,
            drained_signals: 0,
        }
    }

    /// Total signals drained from the approved subscription so far.
    ///
    /// This is test/diagnostic metadata only; it is not part of the snapshot.
    pub fn drained_signals(&self) -> usize {
        self.drained_signals
    }

    async fn read_connections(&mut self) -> ConnectionRead {
        let mut partial = false;
        let mut unresolved = false;

        if self.connection.is_none() {
            match DbusFoundation::connect_with_timeout(Service::NetworkManager, self.timeout).await
            {
                Ok(connection) => self.connection = Some(connection),
                Err(error) => return ConnectionRead::from_error(error),
            }
        }

        let subscription_active = if self.subscription.is_none() {
            let foundation = self
                .connection
                .as_ref()
                .expect("connection was just established");
            match foundation
                .subscribe(&[
                    SignalKind::StateChanged,
                    SignalKind::DeviceAdded,
                    SignalKind::DeviceRemoved,
                ])
                .await
            {
                Ok(subscription) => {
                    self.subscription = Some(subscription);
                    true
                }
                // A signal-setup failure is not fatal: the observation below
                // is still a fresh, authoritative property read. The provider
                // simply falls back to polling for this run.
                Err(_) => false,
            }
        } else {
            true
        };

        if let Some(subscription) = self.subscription.as_mut() {
            let drained = drain_signals(subscription).await;
            self.drained_signals += drained;
        }

        let foundation = self
            .connection
            .as_ref()
            .expect("connection was just established");
        let result =
            read_connections_inner(foundation, &self.sys_root, &mut partial, &mut unresolved).await;

        let connection_lost = matches!(result, SectionResult::Unavailable);
        if connection_lost {
            self.connection = None;
            self.subscription = None;
        }

        ConnectionRead {
            result,
            partial,
            unresolved,
            subscription_active,
        }
    }
}

impl Default for NetworkManagerProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Debug for NetworkManagerProvider {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("NetworkManagerProvider")
            .field("sys_root", &"<redacted>")
            .field("timeout", &self.timeout)
            .field("connected", &self.connection.is_some())
            .field("signals", &self.subscription.is_some())
            .finish_non_exhaustive()
    }
}

#[async_trait]
impl DiscoveryProvider for NetworkManagerProvider {
    fn id(&self) -> ProviderId {
        ProviderId::NetworkManager
    }

    fn priority(&self) -> u8 {
        // Optional providers sort after the universal baseline (netlink 0,
        // sysfs 1). The connections section has a single owner, so this only
        // affects deterministic provider ordering.
        2
    }

    async fn observe(&mut self) -> ProviderObservation {
        let read = self.read_connections().await;
        let mut notes = Vec::new();
        if read.subscription_active {
            push_note(&mut notes, NOTE_POLLING_AND_APPROVED_SIGNALS);
        }
        if read.partial {
            push_note(&mut notes, NOTE_PARTIAL_CONNECTION_READ);
        }
        if read.unresolved {
            push_note(&mut notes, NOTE_UNRESOLVED_DEVICE);
        }

        ProviderObservation {
            provider: ProviderId::NetworkManager,
            priority: self.priority(),
            interfaces: SectionResult::Unsupported,
            addresses_v4: SectionResult::Unsupported,
            addresses_v6: SectionResult::Unsupported,
            routes_v4: SectionResult::Unsupported,
            routes_v6: SectionResult::Unsupported,
            dns: SectionResult::Unsupported,
            connections: read.result,
            fidelity_limited: read.partial || read.unresolved,
            notes,
        }
    }
}

struct ConnectionRead {
    result: SectionResult<ConnectionObservation>,
    partial: bool,
    unresolved: bool,
    subscription_active: bool,
}

impl ConnectionRead {
    fn from_error(error: DbusError) -> Self {
        Self {
            result: section_from_error(error),
            partial: false,
            unresolved: false,
            subscription_active: false,
        }
    }
}

async fn read_connections_inner(
    foundation: &DbusFoundation,
    sys_root: &Path,
    partial: &mut bool,
    unresolved: &mut bool,
) -> SectionResult<ConnectionObservation> {
    let active = match foundation
        .read_property(PropertyRequest::new(
            Service::NetworkManager,
            NETWORK_MANAGER_MANAGER_PATH,
            NETWORK_MANAGER_MANAGER_INTERFACE,
            "ActiveConnections",
        ))
        .await
    {
        Ok(value) => match Vec::<OwnedObjectPath>::try_from(value) {
            Ok(paths) => paths,
            Err(_) => return SectionResult::Failed(ERROR_MALFORMED.to_owned()),
        },
        Err(error) => return section_from_error(error),
    };

    if active.is_empty() {
        return SectionResult::Observed(Vec::new());
    }

    let mut observed = Vec::new();
    let mut failures = 0usize;
    let mut malformed_state = false;
    for active_path in active {
        let path = active_path.as_str();
        let Some(devices) = read_devices(foundation, path).await else {
            failures += 1;
            *partial = true;
            continue;
        };
        let state = match read_state(foundation, path).await {
            Ok(state) => state,
            Err(StateReadError::OutOfRange) => {
                failures += 1;
                malformed_state = true;
                *partial = true;
                continue;
            }
            Err(StateReadError::Unavailable) => {
                failures += 1;
                *partial = true;
                continue;
            }
        };
        let Some(connection_type) = read_type(foundation, path).await else {
            failures += 1;
            *partial = true;
            continue;
        };
        if connection_type.is_empty() {
            failures += 1;
            *partial = true;
            continue;
        }
        let Some(vpn) = read_vpn(foundation, path).await else {
            failures += 1;
            *partial = true;
            continue;
        };

        let mut devices_out = Vec::new();
        let mut device_unresolved = false;
        for device_path in devices {
            let name = read_device_name(foundation, device_path.as_str()).await;
            match name
                .as_deref()
                .and_then(|name| resolve_ifindex(sys_root, name))
            {
                Some(ifindex) => devices_out.push(ifindex),
                None => device_unresolved = true,
            }
        }
        devices_out.sort_unstable();
        devices_out.dedup();

        if device_unresolved {
            *unresolved = true;
        }

        let mut notes = Vec::new();
        if device_unresolved {
            push_note(&mut notes, NOTE_UNRESOLVED_DEVICE);
        }
        observed.push(ConnectionObservation {
            connection_type,
            state: map_connection_state(state),
            devices: devices_out,
            vpn,
            sources: vec![ObservationSource::NetworkManager],
            notes,
        });
    }

    if observed.is_empty() {
        if malformed_state {
            return SectionResult::Failed(ERROR_MALFORMED.to_owned());
        }
        if failures > 0 {
            return SectionResult::Failed(ERROR_PARTIAL.to_owned());
        }
        return SectionResult::Observed(Vec::new());
    }

    if *partial {
        for observation in &mut observed {
            push_note(&mut observation.notes, NOTE_PARTIAL_CONNECTION_READ);
        }
    }

    SectionResult::Observed(observed)
}

async fn read_devices(foundation: &DbusFoundation, path: &str) -> Option<Vec<OwnedObjectPath>> {
    let value = foundation
        .read_property(PropertyRequest::new(
            Service::NetworkManager,
            path,
            NETWORK_MANAGER_ACTIVE_CONNECTION_INTERFACE,
            "Devices",
        ))
        .await
        .ok()?;
    Vec::<OwnedObjectPath>::try_from(value).ok()
}

enum StateReadError {
    Unavailable,
    OutOfRange,
}

async fn read_state(foundation: &DbusFoundation, path: &str) -> Result<u8, StateReadError> {
    let value = foundation
        .read_property(PropertyRequest::new(
            Service::NetworkManager,
            path,
            NETWORK_MANAGER_ACTIVE_CONNECTION_INTERFACE,
            "State",
        ))
        .await
        .map_err(|_| StateReadError::Unavailable)?;
    let raw = u32::try_from(value).map_err(|_| StateReadError::Unavailable)?;
    u8::try_from(raw).map_err(|_| StateReadError::OutOfRange)
}

async fn read_type(foundation: &DbusFoundation, path: &str) -> Option<String> {
    let value = foundation
        .read_property(PropertyRequest::new(
            Service::NetworkManager,
            path,
            NETWORK_MANAGER_ACTIVE_CONNECTION_INTERFACE,
            "Type",
        ))
        .await
        .ok()?;
    String::try_from(value).ok()
}

async fn read_vpn(foundation: &DbusFoundation, path: &str) -> Option<bool> {
    let value = foundation
        .read_property(PropertyRequest::new(
            Service::NetworkManager,
            path,
            NETWORK_MANAGER_ACTIVE_CONNECTION_INTERFACE,
            "Vpn",
        ))
        .await
        .ok()?;
    bool::try_from(value).ok()
}

/// Read the kernel interface name for a device, preferring `Interface` and
/// falling back to `IpInterface` only when the former is absent or empty.
async fn read_device_name(foundation: &DbusFoundation, path: &str) -> Option<String> {
    if let Ok(value) = foundation
        .read_property(PropertyRequest::new(
            Service::NetworkManager,
            path,
            NETWORK_MANAGER_DEVICE_INTERFACE,
            "Interface",
        ))
        .await
    {
        if let Ok(name) = String::try_from(value) {
            if !name.is_empty() {
                return Some(name);
            }
        }
    }

    let value = foundation
        .read_property(PropertyRequest::new(
            Service::NetworkManager,
            path,
            NETWORK_MANAGER_DEVICE_INTERFACE,
            "IpInterface",
        ))
        .await
        .ok()?;
    let name = String::try_from(value).ok()?;
    (!name.is_empty()).then_some(name)
}

fn map_connection_state(raw: u8) -> ConnectionState {
    match raw {
        0 => ConnectionState::Unknown,
        1 => ConnectionState::Activating,
        2 => ConnectionState::Activated,
        3 => ConnectionState::Deactivating,
        4 => ConnectionState::Deactivated,
        other => ConnectionState::Other(other),
    }
}

fn section_from_error(error: DbusError) -> SectionResult<ConnectionObservation> {
    match error {
        DbusError::BusUnavailable | DbusError::ServiceUnavailable => SectionResult::Unavailable,
        DbusError::PermissionDenied => SectionResult::PermissionDenied,
        DbusError::Timeout => SectionResult::Failed(ERROR_TIMEOUT.to_owned()),
        DbusError::MalformedResponse => SectionResult::Failed(ERROR_MALFORMED.to_owned()),
        DbusError::UnsupportedRequest => SectionResult::Unsupported,
        DbusError::RejectedSignal | DbusError::Protocol => {
            SectionResult::Failed(ERROR_INTERNAL.to_owned())
        }
    }
}

async fn drain_signals(subscription: &mut SignalSubscription) -> usize {
    let mut drained = 0usize;
    loop {
        if drained >= MAX_DRAINED_SIGNALS {
            break;
        }
        let wait = if drained == 0 {
            SIGNAL_DRAIN_TIMEOUT
        } else {
            Duration::from_millis(1)
        };
        match tokio::time::timeout(wait, subscription.next()).await {
            Ok(Ok(_)) => drained += 1,
            // A malformed/imposter signal is rejected by the foundation. It is
            // still consumed so it cannot accumulate, but it never becomes an
            // observation.
            Ok(Err(DbusError::RejectedSignal)) => drained += 1,
            Ok(Err(_)) | Err(_) => break,
        }
    }
    drained
}

fn resolve_ifindex(sys_root: &Path, name: &str) -> Option<u32> {
    if !valid_interface_name(name) {
        return None;
    }
    let path = sys_root
        .join("class")
        .join("net")
        .join(name)
        .join("ifindex");
    let text = std::fs::read_to_string(path).ok()?;
    let text = text.trim();
    if text.is_empty() || !text.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    let ifindex = text.parse::<u32>().ok()?;
    (ifindex != 0).then_some(ifindex)
}

fn valid_interface_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 15
        && name != "."
        && name != ".."
        && name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
}

fn push_note(notes: &mut Vec<String>, note: &str) {
    if !notes.iter().any(|existing| existing == note) {
        notes.push(note.to_owned());
    }
}
