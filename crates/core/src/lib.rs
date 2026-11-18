//! # araiguma-core
//!
//! The authoritative domain model for Araiguma's network observations, plus
//! the provider abstraction and merge layer that produce coherent snapshots.
//!
//! Principles:
//!
//! * **Observe, do not control.** Everything here models read-only
//!   observation; nothing in this crate mutates system state.
//! * **`None` means not observed.** A field that no provider could observe
//!   stays `None`; it is never interpreted as a negative fact. The single
//!   documented exception is [`model::Route::gateway`] (on-link routes).
//! * **Provenance is preserved.** Every item records which providers
//!   observed it, and every snapshot records provider health.
//! * **Provider failures degrade, they never destroy.** A failing provider
//!   contributes nothing but its failure report; other providers' valid
//!   observations survive the merge.

pub mod merge;
pub mod model;
pub mod provider;
pub mod snapshot;
pub mod topology;

pub use merge::merge;
pub use model::{
    Address, AddressFamily, AddressScope, AdminState, Interface, InterfaceKind, IpNet,
    ObservationSource, OperState, Route, RouteProtocol, RouteScope, RouteType, RT_TABLE_LOCAL,
    RT_TABLE_MAIN,
};
pub use provider::{
    DiscoveryProvider, ProviderId, ProviderObservation, ProviderReport, ProviderState, SectionKind,
    SectionOutcome, SectionReport, SectionResult,
};
pub use snapshot::{diff_snapshots, interface_changed_fields, NetworkEvent, NetworkSnapshot};
pub use topology::{
    build_topology, EdgeKind, Evidence, NodeDetail, NodeId, NodeKind, NodeState, TopologyDocument,
    TopologyEdge, TopologyNode, TOPOLOGY_SCHEMA_VERSION,
};
