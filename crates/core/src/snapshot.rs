//! Snapshots and change events.
//!
//! A [`NetworkSnapshot`] is the merged, authoritative view of one observation
//! run. [`NetworkEvent`]s describe the difference between two snapshots; the
//! watch CLI emits them as newline-delimited JSON.

use serde::{Deserialize, Serialize};

use crate::model::{
    Address, AddressFamily, ConnectionObservation, DnsObservation, Interface, ObservationSource,
    Route,
};
use crate::provider::{
    ProviderId, ProviderReport, SectionKind, SectionOutcome, CONNECTION_NOTES, DNS_NOTES,
    NETWORK_MANAGER_NOTES, OPTIONAL_ERROR_CODES, SYSTEMD_RESOLVED_NOTES,
};

/// Current snapshot schema revision.
///
/// * **v4** (WO-06): additive optional `dns` and `connections` collections,
///   two new section kinds (`dns`, `connections`), and identity enums whose
///   unknown values round-trip as raw non-empty strings. v3 documents parse
///   unchanged: the new collections are absent, which means "not observed".
/// * **v3** (WO-03): interfaces gained `peerIfindex` (the veth peer
///   relationship; `null` = not observed — see
///   [`crate::model::Interface::peer_ifindex`]), and routes gain a
///   complete multipath next-hop representation (`nextHops`) instead of
///   the previous first-hop-only form (landed with M3 in this version).
/// * **v2** (WO-02 correction §5): `macAddress` became the three-state
///   [`crate::model::MacObservation`] — the plain address string (present,
///   identical to v1), or the sentinels `"absent"` (observed to have no
///   hardware address) / `"notObserved"`. v1's ambiguous `null` is
///   rejected on read.
/// * v1: everything else, as originally defined.
pub const SNAPSHOT_SCHEMA_VERSION: u32 = 4;

/// One merged observation of the local network environment.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NetworkSnapshot {
    /// Model schema revision; bump on breaking JSON changes. See
    /// [`SNAPSHOT_SCHEMA_VERSION`].
    pub schema_version: u32,
    /// Unix epoch milliseconds of the observation run.
    pub timestamp_ms: u64,
    /// All observed interfaces, sorted by ifindex.
    pub interfaces: Vec<Interface>,
    /// All observed addresses, sorted by (ifindex, family, address).
    pub addresses: Vec<Address>,
    /// All observed routes (all tables), sorted for stable display.
    pub routes: Vec<Route>,
    /// Provider status reports for this run.
    pub providers: Vec<ProviderReport>,
    /// DNS configuration observations, or `None` when not observed.
    /// An observed empty collection is `Some(vec![])`.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "de_opt_section"
    )]
    pub dns: Option<Vec<DnsObservation>>,
    /// NetworkManager active-connection observations, or `None` when not
    /// observed. An observed empty collection is `Some(vec![])`.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "de_opt_section"
    )]
    pub connections: Option<Vec<ConnectionObservation>>,
}

fn de_opt_section<'de, D, T>(deserializer: D) -> Result<Option<Vec<T>>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    match Option::<Vec<T>>::deserialize(deserializer)? {
        Some(items) => Ok(Some(items)),
        // The writer omits absent collections. Explicit null is not a
        // distinct third state and is rejected rather than silently mapped
        // to "not observed".
        None => Err(serde::de::Error::custom(
            "must be an array, not null; omit the field when not observed",
        )),
    }
}

impl NetworkSnapshot {
    pub fn interface_by_ifindex(&self, ifindex: u32) -> Option<&Interface> {
        self.interfaces.iter().find(|i| i.ifindex == ifindex)
    }

    pub fn interface_name(&self, ifindex: u32) -> Option<&str> {
        self.interface_by_ifindex(ifindex).map(|i| i.name.as_str())
    }

    pub fn addresses_on(&self, ifindex: u32) -> Vec<&Address> {
        self.addresses
            .iter()
            .filter(|a| a.ifindex == ifindex)
            .collect()
    }

    /// Routes whose next-hop set uses the interface: a single-path route's
    /// output interface, or any multipath hop's interface (WO-03 M3).
    pub fn routes_using_interface(&self, ifindex: u32) -> Vec<&Route> {
        self.routes
            .iter()
            .filter(|r| match &r.next_hops {
                crate::model::NextHops::Single => r.output_interface == Some(ifindex),
                crate::model::NextHops::Multi(hops) => {
                    r.output_interface == Some(ifindex)
                        || hops.iter().any(|hop| hop.output_interface == Some(ifindex))
                }
                crate::model::NextHops::Undecoded(_) => r.output_interface == Some(ifindex),
            })
            .collect()
    }

    /// Default routes, optionally restricted to a family.
    pub fn default_routes(&self, family: Option<AddressFamily>) -> Vec<&Route> {
        self.routes
            .iter()
            .filter(|r| r.is_default() && family.is_none_or(|f| r.family == f))
            .collect()
    }

    /// Distinct routing-table ids present in the observation.
    pub fn tables(&self) -> Vec<u32> {
        let mut tables: Vec<u32> = self.routes.iter().filter_map(|r| r.table).collect();
        tables.sort_unstable();
        tables.dedup();
        tables
    }

    /// Validate that every observation carries provenance (WO-03-F /
    /// AUD-06). An observation with NO source is not a valid observation:
    /// it cannot be traced to anything that actually supplied it, and
    /// downstream presentation must never invent an attribution. Returns
    /// the first offending location as a path-qualified error.
    pub fn validate_provenance(&self) -> Result<(), String> {
        for (index, interface) in self.interfaces.iter().enumerate() {
            if interface.sources.is_empty() {
                return Err(format!(
                    "$.interfaces[{index}].sources is empty — every observation must name at \
                     least one provider that actually supplied it"
                ));
            }
        }
        for (index, address) in self.addresses.iter().enumerate() {
            if address.sources.is_empty() {
                return Err(format!(
                    "$.addresses[{index}].sources is empty — every observation must name at \
                     least one provider that actually supplied it"
                ));
            }
        }
        for (index, route) in self.routes.iter().enumerate() {
            if route.sources.is_empty() {
                return Err(format!(
                    "$.routes[{index}].sources is empty — every observation must name at \
                     least one provider that actually supplied it"
                ));
            }
        }
        if let Some(dns) = &self.dns {
            for (index, observation) in dns.iter().enumerate() {
                if observation.sources.is_empty() {
                    return Err(format!(
                        "$.dns[{index}].sources is empty — every observation must name at \
                         least one provider that actually supplied it"
                    ));
                }
            }
        }
        if let Some(connections) = &self.connections {
            for (index, observation) in connections.iter().enumerate() {
                if observation.sources.is_empty() {
                    return Err(format!(
                        "$.connections[{index}].sources is empty — every observation must \
                         name at least one provider that actually supplied it"
                    ));
                }
            }
        }
        Ok(())
    }

    /// Validate the WO-06 v4 optional-report contract.
    ///
    /// Known providers must carry all seven v4 sections and may only
    /// populate their owned section; every non-owned section must be
    /// `Unsupported`. Error codes and notes are checked against the bounded
    /// static vocabularies. Unknown provider identities are preserved and
    /// not content-validated, preserving forward compatibility.
    pub fn validate_v4_reports(&self) -> Result<(), String> {
        for (index, report) in self.providers.iter().enumerate() {
            let path = format!("$.providers[{index}]");
            match &report.provider {
                ProviderId::NetworkManager => {
                    self.validate_known_sections(index, report, SectionKind::Connections)?;
                    validate_notes(
                        &report.notes,
                        NETWORK_MANAGER_NOTES,
                        &format!("{path}.notes"),
                    )?;
                    validate_failed_codes(report, &path)?;
                }
                ProviderId::SystemdResolved => {
                    self.validate_known_sections(index, report, SectionKind::Dns)?;
                    validate_notes(
                        &report.notes,
                        SYSTEMD_RESOLVED_NOTES,
                        &format!("{path}.notes"),
                    )?;
                    validate_failed_codes(report, &path)?;
                }
                ProviderId::Netlink | ProviderId::SysFs => {
                    self.validate_baseline_optional_sections(index, report)?;
                }
                _ => {}
            }
        }
        if let Some(dns) = &self.dns {
            for (index, observation) in dns.iter().enumerate() {
                if matches!(
                    observation.scope,
                    crate::model::DnsScope::Link { ifindex: 0 }
                ) {
                    return Err(format!(
                        "$.dns[{index}].scope.ifindex must be non-zero for link scope"
                    ));
                }
                if let Some(mode) = &observation.mode {
                    if !valid_dns_mode(mode) {
                        return Err(format!(
                            "$.dns[{index}].mode is not a valid resolver mode token"
                        ));
                    }
                }
                if observation
                    .sources
                    .contains(&ObservationSource::SystemdResolved)
                {
                    validate_notes(
                        &observation.notes,
                        DNS_NOTES,
                        &format!("$.dns[{index}].notes"),
                    )?;
                }
            }
        }
        if let Some(connections) = &self.connections {
            for (index, observation) in connections.iter().enumerate() {
                if observation.connection_type.is_empty() {
                    return Err(format!(
                        "$.connections[{index}].connectionType must be non-empty"
                    ));
                }
                if observation.devices.contains(&0) {
                    return Err(format!("$.connections[{index}].devices contains ifindex 0"));
                }
                if !observation.devices.windows(2).all(|pair| pair[0] < pair[1]) {
                    return Err(format!(
                        "$.connections[{index}].devices must be sorted ascending and deduplicated"
                    ));
                }
                if observation
                    .sources
                    .contains(&ObservationSource::NetworkManager)
                {
                    validate_notes(
                        &observation.notes,
                        CONNECTION_NOTES,
                        &format!("$.connections[{index}].notes"),
                    )?;
                }
            }
        }
        Ok(())
    }

    fn validate_baseline_optional_sections(
        &self,
        provider_index: usize,
        report: &ProviderReport,
    ) -> Result<(), String> {
        for section in SectionKind::known_all() {
            let count = report
                .sections
                .iter()
                .filter(|entry| entry.section == section)
                .count();
            if count != 1 {
                return Err(format!(
                    "$.providers[{provider_index}].sections must contain exactly one {section:?} entry; found {count}"
                ));
            }
        }
        for (section_index, entry) in report.sections.iter().enumerate() {
            if matches!(entry.section, SectionKind::Dns | SectionKind::Connections)
                && !matches!(entry.outcome, SectionOutcome::Unsupported)
            {
                return Err(format!(
                    "$.providers[{provider_index}].sections[{section_index}].outcome must be unsupported for a baseline provider's non-owned section"
                ));
            }
        }
        Ok(())
    }

    fn validate_known_sections(
        &self,
        provider_index: usize,
        report: &ProviderReport,
        owned: SectionKind,
    ) -> Result<(), String> {
        let known = SectionKind::known_all();
        for section in known {
            let count = report
                .sections
                .iter()
                .filter(|entry| entry.section == section)
                .count();
            if count != 1 {
                return Err(format!(
                    "$.providers[{provider_index}].sections must contain exactly one {section:?} entry; found {count}"
                ));
            }
        }
        for (section_index, entry) in report.sections.iter().enumerate() {
            let is_known = SectionKind::known_all()
                .into_iter()
                .any(|known| known == entry.section);
            if !is_known {
                continue;
            }
            if entry.section != owned && !matches!(entry.outcome, SectionOutcome::Unsupported) {
                return Err(format!(
                    "$.providers[{provider_index}].sections[{section_index}].outcome must be unsupported for a non-owned section"
                ));
            }
        }
        Ok(())
    }
}

fn valid_dns_mode(mode: &str) -> bool {
    !mode.is_empty()
        && mode.len() <= 64
        && mode.bytes().any(|byte| byte.is_ascii_alphanumeric())
        && mode
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'/'))
}

fn validate_notes(notes: &[String], allowed: &[&str], path: &str) -> Result<(), String> {
    for (index, note) in notes.iter().enumerate() {
        if !allowed.contains(&note.as_str()) {
            return Err(format!(
                "{path}[{index}] is not a permitted static note: {note:?}"
            ));
        }
    }
    Ok(())
}

fn validate_failed_codes(report: &ProviderReport, path: &str) -> Result<(), String> {
    for (index, entry) in report.sections.iter().enumerate() {
        if let SectionOutcome::Failed { error } = &entry.outcome {
            if !OPTIONAL_ERROR_CODES.contains(&error.as_str()) {
                return Err(format!(
                    "{path}.sections[{index}].outcome.error is not a permitted sanitized code: {error:?}"
                ));
            }
        }
    }
    Ok(())
}

/// Parse and validate a snapshot document at the document boundary.
///
/// This is the ONE document boundary (OD-03, document 04 §5): the CLI
/// file/stdin path and the Tauri IPC path both call it, so a document
/// accepted by one transport is accepted by the other, and a document
/// rejected by one is rejected by the other for the same reason.
///
/// The version policy is unchanged from the CLI implementation: the current
/// version (v4) is written; v3 and v2 documents parse unchanged (the v4
/// `dns` and `connections` fields are absent and mean not observed); v1 is
/// accepted only where it is unambiguous (a null
/// `macAddress` is rejected with an explanation); unknown versions are
/// rejected with the supported range stated. The provenance contract
/// (AUD-06) is enforced for every accepted document.
pub fn parse_snapshot_document(value: serde_json::Value) -> Result<NetworkSnapshot, String> {
    let version = value
        .get("schemaVersion")
        .and_then(|v| v.as_u64())
        .ok_or_else(|| {
            "invalid snapshot document: schemaVersion must be a non-negative integer".to_string()
        })?;
    match version {
        v if v == u64::from(SNAPSHOT_SCHEMA_VERSION) => {}
        3 => {
            // v3 documents predate the optional DNS/connection collections.
            // They parse under the v4 model with both fields absent
            // (`None`), which is the correct "not observed" meaning.
        }
        2 => {
            // v2 documents (three-state MAC, pre-veth/multipath) parse
            // under the v3 model: every v3 addition is defaulted
            // (`peerIfindex`, `nextHops`) or backward-compatible. The MAC
            // rule already rejects ambiguity. A route without `nextHops`
            // is single-path by construction — pre-M3 documents could not
            // represent multipath hop lists.
        }
        1 => {
            // v1 is acceptable only where its macAddress fields are
            // unambiguous (non-null). A null fails with a clear reason.
            if let Some(interfaces) = value.get("interfaces").and_then(|v| v.as_array()) {
                for (index, iface) in interfaces.iter().enumerate() {
                    if iface.get("macAddress").is_some_and(|m| m.is_null()) {
                        return Err(format!(
                            "invalid snapshot at $.interfaces[{index}].macAddress: null is \
                             ambiguous in schema v1 (it could mean either \"observed no hardware \
                             address\" or \"not observed\"); re-export the document with the \
                             current araiguma CLI, which writes unambiguous v2 documents"
                        ));
                    }
                }
            }
        }
        other => {
            return Err(format!(
                "unsupported snapshot schemaVersion {other} (this build reads \
                 {SNAPSHOT_VERSION} documents)",
                SNAPSHOT_VERSION = SNAPSHOT_SCHEMA_VERSION
            ))
        }
    }
    let snapshot: NetworkSnapshot = serde_json::from_value(value)
        .map_err(|error| format!("invalid snapshot document: {error}"))?;
    // Provenance contract (WO-03-F / AUD-06): an observation without a
    // source is not a valid observation, and the projection refuses to
    // invent one.
    snapshot
        .validate_provenance()
        .map_err(|error| format!("invalid snapshot document: {error}"))?;
    if version == u64::from(SNAPSHOT_SCHEMA_VERSION) {
        snapshot
            .validate_v4_reports()
            .map_err(|error| format!("invalid snapshot document: {error}"))?;
    }
    Ok(snapshot)
}

/// A change between two snapshots (see [`diff_snapshots`]).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum NetworkEvent {
    InterfaceAdded {
        interface: Interface,
    },
    InterfaceRemoved {
        interface: Interface,
    },
    InterfaceChanged {
        interface: Interface,
        changed: Vec<String>,
    },
    AddressAdded {
        address: Address,
    },
    AddressRemoved {
        address: Address,
    },
    AddressChanged {
        address: Address,
    },
    RouteAdded {
        route: Route,
    },
    RouteRemoved {
        route: Route,
    },
    RouteChanged {
        route: Route,
    },
    DnsAdded {
        dns: DnsObservation,
    },
    DnsRemoved {
        dns: DnsObservation,
    },
    DnsChanged {
        dns: DnsObservation,
    },
    ConnectionAdded {
        connection: ConnectionObservation,
    },
    ConnectionRemoved {
        connection: ConnectionObservation,
    },
    ProviderReportChanged {
        report: ProviderReport,
    },
    ProviderReportRemoved {
        provider: ProviderId,
    },
    /// Emitted after a monitoring reconnect: state was re-dumped, and the
    /// diff above (if any) may not cover every change that happened while
    /// the monitor was disconnected. Consumers should treat the latest
    /// snapshot as authoritative.
    Resynchronized,
}

/// Field names that changed between two interface observations (used by
/// `InterfaceChanged`).
pub fn interface_changed_fields(old: &Interface, new: &Interface) -> Vec<String> {
    let mut changed = Vec::new();
    if old.name != new.name {
        changed.push("name".into());
    }
    if old.kind != new.kind || old.kind_detail != new.kind_detail {
        changed.push("kind".into());
    }
    if old.oper_state != new.oper_state {
        changed.push("operState".into());
    }
    if old.admin_state != new.admin_state {
        changed.push("adminState".into());
    }
    if old.mac_address != new.mac_address {
        changed.push("macAddress".into());
    }
    if old.mtu != new.mtu {
        changed.push("mtu".into());
    }
    if old.tx_queue_len != new.tx_queue_len {
        changed.push("txQueueLen".into());
    }
    if old.master_ifindex != new.master_ifindex {
        changed.push("masterIfindex".into());
    }
    if old.parent_ifindex != new.parent_ifindex {
        changed.push("parentIfindex".into());
    }
    if old.peer_ifindex != new.peer_ifindex {
        changed.push("peerIfindex".into());
    }
    if old.link_netnsid != new.link_netnsid {
        changed.push("linkNetnsid".into());
    }
    changed
}

/// Canonical route identity for diffing (sources and notes excluded).
/// Complete identity: every field that distinguishes two kernel routes
/// participates (WO-03-F §5.5 — a change in any discriminator must be
/// visible as a change event, never silently absorbed).
#[derive(Debug, Clone, PartialEq)]
struct RouteKey {
    family: AddressFamily,
    table: Option<u32>,
    tos: Option<u8>,
    destination: Option<crate::model::IpNet>,
    gateway: Option<std::net::IpAddr>,
    output_interface: Option<u32>,
    output_interface_name: Option<String>,
    metric: Option<u32>,
    protocol: Option<crate::model::RouteProtocol>,
    scope: Option<crate::model::RouteScope>,
    route_type: Option<crate::model::RouteType>,
    flags: Option<Vec<String>>,
    next_hops: crate::model::NextHops,
}

fn route_key(route: &Route) -> RouteKey {
    RouteKey {
        family: route.family,
        table: route.table,
        tos: route.tos,
        destination: route.destination,
        gateway: route.gateway,
        output_interface: route.output_interface,
        output_interface_name: route.output_interface_name.clone(),
        metric: route.metric,
        protocol: route.protocol,
        scope: route.scope,
        route_type: route.route_type,
        flags: route.flags.clone(),
        next_hops: route.next_hops.clone(),
    }
}

fn dns_key(
    observation: &DnsObservation,
) -> (crate::model::DnsScope, Vec<crate::model::ObservationSource>) {
    (observation.scope.clone(), observation.sources.clone())
}

fn diff_dns(
    previous: Option<&[DnsObservation]>,
    current: Option<&[DnsObservation]>,
    events: &mut Vec<NetworkEvent>,
) {
    let empty: &[DnsObservation] = &[];
    let previous = previous.unwrap_or(empty);
    let current = current.unwrap_or(empty);

    let mut old: Vec<&DnsObservation> = previous.iter().collect();
    let mut new: Vec<&DnsObservation> = current.iter().collect();
    old.sort();
    new.sort();

    let mut old_used = vec![false; old.len()];
    let mut new_used = vec![false; new.len()];

    // Exact matches (identity and content) produce no event.
    for (new_index, new_item) in new.iter().enumerate() {
        if let Some(old_index) = old
            .iter()
            .enumerate()
            .position(|(old_index, old_item)| !old_used[old_index] && *old_item == *new_item)
        {
            old_used[old_index] = true;
            new_used[new_index] = true;
        }
    }

    // Same identity, changed content.
    for (new_index, new_item) in new.iter().enumerate() {
        if new_used[new_index] {
            continue;
        }
        if let Some(old_index) = old.iter().enumerate().position(|(old_index, old_item)| {
            !old_used[old_index] && dns_key(old_item) == dns_key(new_item)
        }) {
            old_used[old_index] = true;
            new_used[new_index] = true;
            events.push(NetworkEvent::DnsChanged {
                dns: (*new_item).clone(),
            });
        }
    }

    for (new_index, new_item) in new.iter().enumerate() {
        if !new_used[new_index] {
            events.push(NetworkEvent::DnsAdded {
                dns: (*new_item).clone(),
            });
        }
    }
    for (old_index, old_item) in old.iter().enumerate() {
        if !old_used[old_index] {
            events.push(NetworkEvent::DnsRemoved {
                dns: (*old_item).clone(),
            });
        }
    }
}

fn diff_connections(
    previous: Option<&[ConnectionObservation]>,
    current: Option<&[ConnectionObservation]>,
    events: &mut Vec<NetworkEvent>,
) {
    let empty: &[ConnectionObservation] = &[];
    let previous = previous.unwrap_or(empty);
    let current = current.unwrap_or(empty);

    let mut old: Vec<&ConnectionObservation> = previous.iter().collect();
    let mut new: Vec<&ConnectionObservation> = current.iter().collect();
    old.sort();
    new.sort();

    let mut old_used = vec![false; old.len()];
    let mut new_used = vec![false; new.len()];

    for (new_index, new_item) in new.iter().enumerate() {
        if let Some(old_index) = old
            .iter()
            .enumerate()
            .position(|(old_index, old_item)| !old_used[old_index] && *old_item == *new_item)
        {
            old_used[old_index] = true;
            new_used[new_index] = true;
        }
    }

    // A changed connection is removal plus addition, never a synthetic
    // ConnectionChanged event.
    for (new_index, new_item) in new.iter().enumerate() {
        if !new_used[new_index] {
            events.push(NetworkEvent::ConnectionAdded {
                connection: (*new_item).clone(),
            });
        }
    }
    for (old_index, old_item) in old.iter().enumerate() {
        if !old_used[old_index] {
            events.push(NetworkEvent::ConnectionRemoved {
                connection: (*old_item).clone(),
            });
        }
    }
}

fn diff_provider_reports(
    previous: &NetworkSnapshot,
    current: &NetworkSnapshot,
    events: &mut Vec<NetworkEvent>,
) {
    let mut old: Vec<&ProviderReport> = previous.providers.iter().collect();
    let mut new: Vec<&ProviderReport> = current.providers.iter().collect();
    old.sort_by(|a, b| a.provider.cmp(&b.provider));
    new.sort_by(|a, b| a.provider.cmp(&b.provider));

    let mut old_used = vec![false; old.len()];
    for new_report in &new {
        if let Some(old_index) = old.iter().enumerate().position(|(old_index, old_report)| {
            !old_used[old_index] && old_report.provider == new_report.provider
        }) {
            old_used[old_index] = true;
            if old[old_index] != *new_report {
                events.push(NetworkEvent::ProviderReportChanged {
                    report: (*new_report).clone(),
                });
            }
        } else {
            events.push(NetworkEvent::ProviderReportChanged {
                report: (*new_report).clone(),
            });
        }
    }
    for (old_index, old_report) in old.iter().enumerate() {
        if !old_used[old_index] {
            events.push(NetworkEvent::ProviderReportRemoved {
                provider: old_report.provider.clone(),
            });
        }
    }
}

/// Compute the events that turn `previous` into `current`. Pure function.
pub fn diff_snapshots(previous: &NetworkSnapshot, current: &NetworkSnapshot) -> Vec<NetworkEvent> {
    let mut events = Vec::new();

    // Interfaces.
    for new_iface in &current.interfaces {
        match previous
            .interfaces
            .iter()
            .find(|i| i.ifindex == new_iface.ifindex)
        {
            None => events.push(NetworkEvent::InterfaceAdded {
                interface: new_iface.clone(),
            }),
            Some(old) => {
                let changed = interface_changed_fields(old, new_iface);
                if !changed.is_empty() {
                    events.push(NetworkEvent::InterfaceChanged {
                        interface: new_iface.clone(),
                        changed,
                    });
                }
            }
        }
    }
    for old_iface in &previous.interfaces {
        if !current
            .interfaces
            .iter()
            .any(|i| i.ifindex == old_iface.ifindex)
        {
            events.push(NetworkEvent::InterfaceRemoved {
                interface: old_iface.clone(),
            });
        }
    }

    // Addresses (key: ifindex + family + address + prefix).
    for new_addr in &current.addresses {
        let key = |a: &Address| (a.ifindex, a.family, a.address, a.prefix_len);
        match previous.addresses.iter().find(|a| key(a) == key(new_addr)) {
            None => events.push(NetworkEvent::AddressAdded {
                address: new_addr.clone(),
            }),
            Some(old) if old != new_addr => events.push(NetworkEvent::AddressChanged {
                address: new_addr.clone(),
            }),
            _ => {}
        }
    }
    for old_addr in &previous.addresses {
        let key = |a: &Address| (a.ifindex, a.family, a.address, a.prefix_len);
        if !current.addresses.iter().any(|a| key(a) == key(old_addr)) {
            events.push(NetworkEvent::AddressRemoved {
                address: old_addr.clone(),
            });
        }
    }

    // Routes (key: canonical identity without sources/notes).
    for new_route in &current.routes {
        let key = route_key;
        match previous.routes.iter().find(|r| key(r) == key(new_route)) {
            None => events.push(NetworkEvent::RouteAdded {
                route: new_route.clone(),
            }),
            Some(old) if old != new_route => events.push(NetworkEvent::RouteChanged {
                route: new_route.clone(),
            }),
            _ => {}
        }
    }
    for old_route in &previous.routes {
        if !current
            .routes
            .iter()
            .any(|r| route_key(r) == route_key(old_route))
        {
            events.push(NetworkEvent::RouteRemoved {
                route: old_route.clone(),
            });
        }
    }

    diff_dns(previous.dns.as_deref(), current.dns.as_deref(), &mut events);
    diff_connections(
        previous.connections.as_deref(),
        current.connections.as_deref(),
        &mut events,
    );
    diff_provider_reports(previous, current, &mut events);

    events
}
