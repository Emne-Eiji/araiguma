//! Provider abstraction: how observations enter the model.
//!
//! A provider is a read-only source of network state (rtnetlink, sysfs,
//! procfs; later NetworkManager/systemd-resolved D-Bus). Providers never
//! throw away partial results: a failing section is recorded as failed while
//! other sections keep their observations, so the merge layer can degrade
//! gracefully instead of discarding valid data.

use serde::{Deserialize, Serialize};

use crate::model::{Address, Interface, Route};

/// Identifies a provider implementation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ProviderId {
    Netlink,
    SysFs,
}

/// Overall state of a provider for a given observation run.
///
/// * `NotYetObserved` — the provider has not been queried yet.
/// * `Available` — every supported section produced observations, without
///   fidelity limitations.
/// * `Degraded` — every supported section produced observations, but with
///   known fidelity limitations (e.g. sysfs cannot classify virtual
///   interface kinds). Valid data, weaker than the source's normal promise.
/// * `Partial` — some supported sections produced observations and some
///   failed at runtime.
/// * `Unavailable` — every supported section failed at runtime.
/// * `NotSupported` — the provider has no supported sections in this
///   environment at all.
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
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SectionKind {
    Interfaces,
    AddressesV4,
    AddressesV6,
    RoutesV4,
    RoutesV6,
}

/// Outcome of one section inside a provider observation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "kind")]
pub enum SectionOutcome {
    /// Observed (possibly zero items — zero is a valid observation).
    Observed { count: usize },
    /// Attempted and failed; `error` describes why.
    Failed { error: String },
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
    /// Set when the run succeeded structurally but with known fidelity
    /// limitations (drives the `Degraded` state).
    pub fidelity_limited: bool,
    /// Caveat notes recorded by the provider (kept verbatim).
    pub notes: Vec<String>,
}

impl ProviderObservation {
    /// All sections as type-erased (kind, outcome) pairs.
    pub fn sections(&self) -> Vec<(SectionKind, SectionOutcome)> {
        vec![
            (SectionKind::Interfaces, self.interfaces.outcome()),
            (SectionKind::AddressesV4, self.addresses_v4.outcome()),
            (SectionKind::AddressesV6, self.addresses_v6.outcome()),
            (SectionKind::RoutesV4, self.routes_v4.outcome()),
            (SectionKind::RoutesV6, self.routes_v6.outcome()),
        ]
    }

    /// Overall provider state for this run (see [`ProviderState`]).
    ///
    /// Aggregation rules (WO-02 correction §7 — `NotObserved` is never
    /// conflated with `Unsupported`):
    ///
    /// 1. Nothing attempted-and-supported (no section observed and none
    ///    failed): some sections not attempted → `NotYetObserved` (an
    ///    unattempted observation is not evidence of missing
    ///    functionality); otherwise every section is `Unsupported` →
    ///    `NotSupported`. An empty section list also reports
    ///    `NotSupported` ("no supported sections in this environment").
    /// 2. At least one section failed and none observed → `Unavailable`.
    /// 3. Some sections observed, and at least one supported section
    ///    failed or was not attempted → `Partial` (the run is incomplete
    ///    either way; provider health is never claimed from missing
    ///    observations).
    /// 4. Every supported section observed, none failed, none unattempted:
    ///    `Degraded` when the provider recorded fidelity limitations,
    ///    `Available` otherwise. (Sections that are `Unsupported` in
    ///    principle never lower the state.)
    pub fn state(&self) -> ProviderState {
        let mut observed = 0usize;
        let mut failed = 0usize;
        let mut not_observed = 0usize;
        for (_kind, outcome) in self.sections() {
            match outcome {
                SectionOutcome::Observed { .. } => observed += 1,
                SectionOutcome::Failed { .. } => failed += 1,
                SectionOutcome::NotObserved => not_observed += 1,
                SectionOutcome::Unsupported => {}
            }
        }
        if observed == 0 && failed == 0 {
            if not_observed > 0 {
                ProviderState::NotYetObserved
            } else {
                ProviderState::NotSupported
            }
        } else if observed == 0 {
            ProviderState::Unavailable
        } else if failed > 0 || not_observed > 0 {
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
            provider: self.provider,
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
