//! Provider abstraction: how observations enter the model.
//!
//! A provider is a read-only source of network state (rtnetlink, sysfs,
//! procfs; later NetworkManager/systemd-resolved D-Bus). Providers never
//! throw away partial results: a failing section is recorded as failed while
//! other sections keep their observations, so the merge layer can degrade
//! gracefully instead of discarding valid data.

use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::model::{Address, ConnectionObservation, DnsObservation, Interface, Route};

/// Identifies a provider implementation.
///
/// Known identities serialize as exact wire strings. Unknown identities are
/// preserved verbatim as non-empty strings; `Other(String)` is never serialized
/// as a tagged enum object. `ProcFs` is an observation source, not a provider
/// identity.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ProviderId {
    Netlink,
    SysFs,
    NetworkManager,
    SystemdResolved,
    Other(String),
}

impl ProviderId {
    pub fn as_wire_str(&self) -> &str {
        match self {
            Self::Netlink => "netlink",
            Self::SysFs => "sysfs",
            Self::NetworkManager => "networkManager",
            Self::SystemdResolved => "systemdResolved",
            Self::Other(raw) => raw,
        }
    }
}

impl Serialize for ProviderId {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        if self.as_wire_str().is_empty() {
            return Err(serde::ser::Error::custom(
                "provider identity must be a non-empty string",
            ));
        }
        serializer.serialize_str(self.as_wire_str())
    }
}

impl<'de> Deserialize<'de> for ProviderId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let raw = String::deserialize(deserializer)?;
        if raw.is_empty() {
            return Err(serde::de::Error::custom(
                "provider identity must be a non-empty string",
            ));
        }
        Ok(match raw.as_str() {
            "netlink" => Self::Netlink,
            "sysfs" => Self::SysFs,
            "networkManager" => Self::NetworkManager,
            "systemdResolved" => Self::SystemdResolved,
            _ => Self::Other(raw),
        })
    }
}

/// Overall state of a provider for a given observation run.
///
/// * `NotYetObserved` — nothing was attempted and supported.
/// * `Available` — every supported section produced observations, without
///   fidelity limitations.
/// * `Degraded` — either every supported section observed with known fidelity
///   limitations, or the provider attempted work but produced no observations
///   because a section failed or permission was denied.
/// * `Partial` — some sections observed and some failed, were unavailable,
///   were denied, or were not attempted.
/// * `Unavailable` — no section observed and at least one section was
///   unavailable, without any failure or permission denial.
/// * `NotSupported` — no supported sections exist in this environment.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ProviderState {
    NotYetObserved,
    Available,
    Degraded,
    Partial,
    Unavailable,
    NotSupported,
}

/// One observation section.
///
/// Known identities serialize as exact wire strings. Unknown identities are
/// preserved verbatim as non-empty strings.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum SectionKind {
    Interfaces,
    AddressesV4,
    AddressesV6,
    RoutesV4,
    RoutesV6,
    Dns,
    Connections,
    Other(String),
}

impl SectionKind {
    pub fn as_wire_str(&self) -> &str {
        match self {
            Self::Interfaces => "interfaces",
            Self::AddressesV4 => "addressesV4",
            Self::AddressesV6 => "addressesV6",
            Self::RoutesV4 => "routesV4",
            Self::RoutesV6 => "routesV6",
            Self::Dns => "dns",
            Self::Connections => "connections",
            Self::Other(raw) => raw,
        }
    }

    /// The seven sections every WO-06 v4 provider report carries, in wire
    /// order. `Other` values are additional reader-side identities.
    pub fn known_all() -> [SectionKind; 7] {
        [
            Self::Interfaces,
            Self::AddressesV4,
            Self::AddressesV6,
            Self::RoutesV4,
            Self::RoutesV6,
            Self::Dns,
            Self::Connections,
        ]
    }
}

impl Serialize for SectionKind {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        if self.as_wire_str().is_empty() {
            return Err(serde::ser::Error::custom(
                "section identity must be a non-empty string",
            ));
        }
        serializer.serialize_str(self.as_wire_str())
    }
}

impl<'de> Deserialize<'de> for SectionKind {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let raw = String::deserialize(deserializer)?;
        if raw.is_empty() {
            return Err(serde::de::Error::custom(
                "section identity must be a non-empty string",
            ));
        }
        Ok(match raw.as_str() {
            "interfaces" => Self::Interfaces,
            "addressesV4" => Self::AddressesV4,
            "addressesV6" => Self::AddressesV6,
            "routesV4" => Self::RoutesV4,
            "routesV6" => Self::RoutesV6,
            "dns" => Self::Dns,
            "connections" => Self::Connections,
            _ => Self::Other(raw),
        })
    }
}

/// Outcome of one section inside a provider observation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "kind")]
pub enum SectionOutcome {
    /// Observed (possibly zero items — zero is a valid observation).
    Observed { count: usize },
    /// Attempted and failed. WO-06 optional providers must use a bounded,
    /// sanitized code; baseline providers retain their existing descriptive
    /// local errors.
    Failed { error: String },
    /// The service or resource was unavailable for this run.
    Unavailable,
    /// Access was denied by policy.
    PermissionDenied,
    /// The provider cannot observe this section in principle.
    Unsupported,
    /// Not attempted in this run.
    NotObserved,
}

/// Result of one section, carrying the observed items when successful.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SectionResult<T> {
    Observed(Vec<T>),
    Failed(String),
    Unavailable,
    PermissionDenied,
    Unsupported,
    NotObserved,
}

impl<T> SectionResult<T> {
    pub fn items(&self) -> &[T] {
        match self {
            SectionResult::Observed(items) => items,
            _ => &[],
        }
    }

    pub fn outcome(&self) -> SectionOutcome {
        match self {
            SectionResult::Observed(items) => SectionOutcome::Observed { count: items.len() },
            SectionResult::Failed(error) => SectionOutcome::Failed {
                error: error.clone(),
            },
            SectionResult::Unavailable => SectionOutcome::Unavailable,
            SectionResult::PermissionDenied => SectionOutcome::PermissionDenied,
            SectionResult::Unsupported => SectionOutcome::Unsupported,
            SectionResult::NotObserved => SectionOutcome::NotObserved,
        }
    }
}

/// The complete output of one provider run.
#[derive(Debug, Clone)]
pub struct ProviderObservation {
    pub provider: ProviderId,
    /// Merge priority; lower wins (netlink 0, sysfs 1).
    pub priority: u8,
    pub interfaces: SectionResult<Interface>,
    pub addresses_v4: SectionResult<Address>,
    pub addresses_v6: SectionResult<Address>,
    pub routes_v4: SectionResult<Route>,
    pub routes_v6: SectionResult<Route>,
    pub dns: SectionResult<DnsObservation>,
    pub connections: SectionResult<ConnectionObservation>,
    /// Set when the run succeeded structurally but with known fidelity
    /// limitations (drives the `Degraded` state).
    pub fidelity_limited: bool,
    /// Caveat notes recorded by the provider.
    pub notes: Vec<String>,
}

impl ProviderObservation {
    /// All sections as type-erased (kind, outcome) pairs, in wire order.
    pub fn sections(&self) -> Vec<(SectionKind, SectionOutcome)> {
        vec![
            (SectionKind::Interfaces, self.interfaces.outcome()),
            (SectionKind::AddressesV4, self.addresses_v4.outcome()),
            (SectionKind::AddressesV6, self.addresses_v6.outcome()),
            (SectionKind::RoutesV4, self.routes_v4.outcome()),
            (SectionKind::RoutesV6, self.routes_v6.outcome()),
            (SectionKind::Dns, self.dns.outcome()),
            (SectionKind::Connections, self.connections.outcome()),
        ]
    }

    /// Overall provider state for this run (see [`ProviderState`]).
    ///
    /// Aggregation precedence (WO-06):
    ///
    /// 1. All `NotObserved` → `NotYetObserved`.
    /// 2. All `Unsupported` → `NotSupported`.
    /// 3. Any `Observed`:
    ///    - any `Failed`, `Unavailable`, `PermissionDenied`, or
    ///      `NotObserved` → `Partial`;
    ///    - otherwise `fidelity_limited` → `Degraded`;
    ///    - otherwise → `Available`.
    /// 4. No `Observed`:
    ///    - any `Failed` or `PermissionDenied` → `Degraded`;
    ///    - otherwise any `Unavailable` → `Unavailable`;
    ///    - otherwise any `NotObserved` → `NotYetObserved`;
    ///    - otherwise → `NotSupported`.
    ///
    /// `Unsupported` never lowers a state. An empty section list reports
    /// `NotSupported`.
    pub fn state(&self) -> ProviderState {
        let mut observed = 0usize;
        let mut failed = 0usize;
        let mut unavailable = 0usize;
        let mut permission_denied = 0usize;
        let mut not_observed = 0usize;
        for (_kind, outcome) in self.sections() {
            match outcome {
                SectionOutcome::Observed { .. } => observed += 1,
                SectionOutcome::Failed { .. } => failed += 1,
                SectionOutcome::Unavailable => unavailable += 1,
                SectionOutcome::PermissionDenied => permission_denied += 1,
                SectionOutcome::NotObserved => not_observed += 1,
                SectionOutcome::Unsupported => {}
            }
        }

        if observed == 0 {
            if failed > 0 || permission_denied > 0 {
                ProviderState::Degraded
            } else if unavailable > 0 {
                ProviderState::Unavailable
            } else if not_observed > 0 {
                ProviderState::NotYetObserved
            } else {
                ProviderState::NotSupported
            }
        } else if failed > 0 || unavailable > 0 || permission_denied > 0 || not_observed > 0 {
            ProviderState::Partial
        } else if self.fidelity_limited {
            ProviderState::Degraded
        } else {
            ProviderState::Available
        }
    }

    /// Compact report for embedding in a snapshot (items excluded).
    pub fn summarize(&self) -> ProviderReport {
        ProviderReport {
            provider: self.provider.clone(),
            state: self.state(),
            sections: self
                .sections()
                .into_iter()
                .map(|(section, outcome)| SectionReport { section, outcome })
                .collect(),
            notes: self.notes.clone(),
        }
    }
}

/// Bounded error codes permitted in optional-provider `Failed` outcomes.
pub const OPTIONAL_ERROR_CODES: &[&str] = &[
    "timeout",
    "missingProperty",
    "malformedResponse",
    "unsupportedVersion",
    "internalError",
    "cancelled",
    "partialConnectionRead",
    "unsupportedAddressFamily",
];

/// Static provider notes permitted for NetworkManager.
pub const NETWORK_MANAGER_NOTES: &[&str] = &[
    "partialConnectionRead",
    "unresolvedDevice",
    "pollingAndApprovedSignals",
];

/// Static provider notes permitted for systemd-resolved.
pub const SYSTEMD_RESOLVED_NOTES: &[&str] = &["pollOnly"];

/// Static notes permitted on DNS observations.
pub const DNS_NOTES: &[&str] = &[
    "derivedFromManagerAggregate",
    "resolvConfModeUnavailable",
    "linkNotPresent",
];

/// Static notes permitted on connection observations.
pub const CONNECTION_NOTES: &[&str] = &["partialConnectionRead", "unresolvedDevice"];

/// Provider summary embedded in snapshots.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderReport {
    pub provider: ProviderId,
    pub state: ProviderState,
    pub sections: Vec<SectionReport>,
    pub notes: Vec<String>,
}

/// One section inside a provider report.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SectionReport {
    pub section: SectionKind,
    pub outcome: SectionOutcome,
}

/// The discovery provider trait. Implementations must be read-only.
#[async_trait::async_trait]
pub trait DiscoveryProvider: Send {
    fn id(&self) -> ProviderId;

    /// Merge priority: lower value wins field conflicts (netlink < sysfs).
    fn priority(&self) -> u8;

    /// Run one observation pass. Never panics on failure: failures are
    /// captured per-section in the returned observation.
    async fn observe(&mut self) -> ProviderObservation;
}
