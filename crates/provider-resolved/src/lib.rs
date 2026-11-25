//! # araiguma-provider-resolved
//!
//! The read-only systemd-resolved discovery provider.
//!
//! The provider observes only the approved manager-level properties:
//!
//! * `DNSEx` (`a(iiayqs)`),
//! * `Domains` (`a(isb)`), and
//! * `ResolvConfMode` (`s`).
//!
//! It never reads per-link objects, never calls `GetLink`, never invokes a
//! resolution method, never mutates configuration, and never subscribes to
//! resolved signals. All D-Bus access goes through
//! [`araiguma_provider_dbus::DbusFoundation`], so the property allow-list,
//! `NO_AUTO_START`, disabled property caching, ownership preflight, and
//! bounded timeouts are inherited.
//!
//! Construction is side-effect-free. Each `observe()` connects if needed,
//! reads the three approved properties, and assembles deterministic global
//! and per-link [`DnsObservation`] values. A failed refresh never leaves a
//! previous DNS observation in place.
//!
//! DNS configuration is evidence only. The provider never claims that a
//! server is reachable or that name resolution works.

#![forbid(unsafe_code)]

use std::collections::BTreeMap;
use std::fmt;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};
use std::time::Duration;

use araiguma_core::model::{DnsObservation, DnsScope, DnsServer, ObservationSource};
use araiguma_core::provider::{DiscoveryProvider, ProviderId, ProviderObservation, SectionResult};
use araiguma_provider_dbus::policy::{
    SYSTEMD_RESOLVED_MANAGER_INTERFACE, SYSTEMD_RESOLVED_MANAGER_PATH,
};
use araiguma_provider_dbus::{DbusError, DbusFoundation, PropertyRequest, Service};
use async_trait::async_trait;
use zbus::OwnedVariant;

/// Default bounded deadline for connect, preflight, and each property read.
pub const DEFAULT_OPERATION_TIMEOUT: Duration = Duration::from_secs(5);

const NOTE_DERIVED_FROM_MANAGER: &str = "derivedFromManagerAggregate";
const NOTE_MODE_UNAVAILABLE: &str = "resolvConfModeUnavailable";
const PROVIDER_NOTE_POLL_ONLY: &str = "pollOnly";

const ERROR_TIMEOUT: &str = "timeout";
const ERROR_MALFORMED: &str = "malformedResponse";
const ERROR_INTERNAL: &str = "internalError";

/// The systemd-resolved discovery provider.
pub struct SystemdResolvedProvider {
    timeout: Duration,
    connection: Option<DbusFoundation>,
}

impl SystemdResolvedProvider {
    /// Construct a provider with the default bounded timeout.
    ///
    /// This performs no I/O and opens no connection.
    pub fn new() -> Self {
        Self::with_timeout(DEFAULT_OPERATION_TIMEOUT)
    }

    /// Construct a provider with an explicit bounded timeout.
    pub fn with_timeout(timeout: Duration) -> Self {
        Self {
            timeout,
            connection: None,
        }
    }

    async fn read_dns(&mut self) -> DnsResult {
        if self.connection.is_none() {
            match DbusFoundation::connect_with_timeout(Service::SystemdResolved, self.timeout).await
            {
                Ok(connection) => self.connection = Some(connection),
                Err(error) => return DnsResult::failure(failure_from_dbus(error)),
            }
        }

        let foundation = self
            .connection
            .as_ref()
            .expect("connection was just established");
        let dns_ex = foundation
            .read_property(PropertyRequest::new(
                Service::SystemdResolved,
                SYSTEMD_RESOLVED_MANAGER_PATH,
                SYSTEMD_RESOLVED_MANAGER_INTERFACE,
                "DNSEx",
            ))
            .await;
        let domains = foundation
            .read_property(PropertyRequest::new(
                Service::SystemdResolved,
                SYSTEMD_RESOLVED_MANAGER_PATH,
                SYSTEMD_RESOLVED_MANAGER_INTERFACE,
                "Domains",
            ))
            .await;
        let mode = foundation
            .read_property(PropertyRequest::new(
                Service::SystemdResolved,
                SYSTEMD_RESOLVED_MANAGER_PATH,
                SYSTEMD_RESOLVED_MANAGER_INTERFACE,
                "ResolvConfMode",
            ))
            .await;

        let connection_lost = [&dns_ex, &domains, &mode].iter().any(|result| {
            matches!(
                result,
                Err(DbusError::BusUnavailable | DbusError::ServiceUnavailable)
            )
        });
        let result = assemble_dns(dns_ex, domains, mode);
        if connection_lost {
            self.connection = None;
        }
        result
    }
}

impl Default for SystemdResolvedProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Debug for SystemdResolvedProvider {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SystemdResolvedProvider")
            .field("timeout", &self.timeout)
            .field("connected", &self.connection.is_some())
            .finish_non_exhaustive()
    }
}

#[async_trait]
impl DiscoveryProvider for SystemdResolvedProvider {
    fn id(&self) -> ProviderId {
        ProviderId::SystemdResolved
    }

    fn priority(&self) -> u8 {
        // Optional providers sort after the universal baseline and the
        // NetworkManager provider. The DNS section has a single owner, so
        // this only makes provider ordering deterministic.
        3
    }

    async fn observe(&mut self) -> ProviderObservation {
        let result = self.read_dns().await;
        ProviderObservation {
            provider: ProviderId::SystemdResolved,
            priority: self.priority(),
            interfaces: SectionResult::Unsupported,
            addresses_v4: SectionResult::Unsupported,
            addresses_v6: SectionResult::Unsupported,
            routes_v4: SectionResult::Unsupported,
            routes_v6: SectionResult::Unsupported,
            dns: result.section,
            connections: SectionResult::Unsupported,
            fidelity_limited: result.partial,
            notes: vec![PROVIDER_NOTE_POLL_ONLY.to_owned()],
        }
    }
}

struct DnsResult {
    section: SectionResult<DnsObservation>,
    partial: bool,
}

impl DnsResult {
    fn failure(failure: Failure) -> Self {
        Self {
            section: section_from_failure(failure),
            partial: false,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Failure {
    PermissionDenied,
    Unavailable,
    Timeout,
    Malformed,
    Unsupported,
    Internal,
}

impl Failure {
    fn rank(self) -> u8 {
        match self {
            Self::PermissionDenied => 6,
            Self::Unavailable => 5,
            Self::Timeout => 4,
            Self::Malformed => 3,
            Self::Unsupported => 2,
            Self::Internal => 1,
        }
    }
}

fn failure_from_dbus(error: DbusError) -> Failure {
    match error {
        DbusError::BusUnavailable | DbusError::ServiceUnavailable => Failure::Unavailable,
        DbusError::PermissionDenied => Failure::PermissionDenied,
        DbusError::Timeout => Failure::Timeout,
        DbusError::MalformedResponse => Failure::Malformed,
        DbusError::UnsupportedRequest => Failure::Unsupported,
        DbusError::RejectedSignal | DbusError::Protocol => Failure::Internal,
    }
}

fn section_from_failure(failure: Failure) -> SectionResult<DnsObservation> {
    match failure {
        Failure::PermissionDenied => SectionResult::PermissionDenied,
        Failure::Unavailable => SectionResult::Unavailable,
        Failure::Timeout => SectionResult::Failed(ERROR_TIMEOUT.to_owned()),
        Failure::Malformed => SectionResult::Failed(ERROR_MALFORMED.to_owned()),
        Failure::Unsupported => SectionResult::Unsupported,
        Failure::Internal => SectionResult::Failed(ERROR_INTERNAL.to_owned()),
    }
}

fn set_failure(current: &mut Option<Failure>, candidate: Failure) {
    if current.is_none_or(|existing| candidate.rank() > existing.rank()) {
        *current = Some(candidate);
    }
}

#[derive(Default)]
struct ScopeData {
    servers: Vec<DnsServer>,
    search_domains: Vec<String>,
    routing_domains: Vec<String>,
}

fn assemble_dns(
    dns_ex: Result<OwnedVariant, DbusError>,
    domains: Result<OwnedVariant, DbusError>,
    mode: Result<OwnedVariant, DbusError>,
) -> DnsResult {
    let mut failure = None;
    let mut partial = false;

    let (servers, dns_ex_malformed) = match dns_ex {
        Ok(value) => parse_dns_ex(value),
        Err(error) => {
            set_failure(&mut failure, failure_from_dbus(error));
            partial = true;
            (Vec::new(), false)
        }
    };
    if dns_ex_malformed {
        set_failure(&mut failure, Failure::Malformed);
        partial = true;
    }

    let (domains, domains_malformed) = match domains {
        Ok(value) => parse_domains(value),
        Err(error) => {
            set_failure(&mut failure, failure_from_dbus(error));
            partial = true;
            (Vec::new(), false)
        }
    };
    if domains_malformed {
        set_failure(&mut failure, Failure::Malformed);
        partial = true;
    }

    let (mode_value, mode_malformed) = match mode {
        Ok(value) => parse_mode(value),
        Err(error) => {
            set_failure(&mut failure, failure_from_dbus(error));
            partial = true;
            (None, true)
        }
    };
    if mode_malformed {
        set_failure(&mut failure, Failure::Malformed);
        partial = true;
    }
    let mode_unavailable = mode_malformed || mode_value.is_none();

    let mut scopes: BTreeMap<DnsScope, ScopeData> = BTreeMap::new();
    for (scope, server) in servers {
        scopes.entry(scope).or_default().servers.push(server);
    }
    for (scope, domain, route_only) in domains {
        let entry = scopes.entry(scope).or_default();
        if route_only {
            entry.routing_domains.push(domain);
        } else {
            entry.search_domains.push(domain);
        }
    }

    if scopes.is_empty() {
        let section = match failure {
            Some(failure) => section_from_failure(failure),
            None => SectionResult::Observed(Vec::new()),
        };
        return DnsResult {
            section,
            partial: false,
        };
    }

    let mut observations = Vec::with_capacity(scopes.len());
    for (scope, mut data) in scopes {
        data.servers.sort();
        data.search_domains.sort();
        data.routing_domains.sort();

        let mut notes = vec![NOTE_DERIVED_FROM_MANAGER.to_owned()];
        let mode = if scope == DnsScope::Global {
            if mode_unavailable {
                notes.push(NOTE_MODE_UNAVAILABLE.to_owned());
            }
            mode_value.clone()
        } else {
            None
        };

        observations.push(DnsObservation {
            scope,
            mode,
            servers: data.servers,
            search_domains: data.search_domains,
            routing_domains: data.routing_domains,
            sources: vec![ObservationSource::SystemdResolved],
            notes,
        });
    }

    DnsResult {
        section: SectionResult::Observed(observations),
        partial,
    }
}

fn parse_dns_ex(value: OwnedVariant) -> (Vec<(DnsScope, DnsServer)>, bool) {
    let entries: Vec<(i32, i32, Vec<u8>, u16, String)> = match value.try_into() {
        Ok(entries) => entries,
        Err(_) => return (Vec::new(), true),
    };

    let mut parsed = Vec::new();
    let mut malformed = false;
    for (ifindex, family, address, port, _server_name) in entries {
        let Some(scope) = scope_from_ifindex(ifindex) else {
            malformed = true;
            continue;
        };
        let Some(address) = parse_ip(family, &address) else {
            malformed = true;
            continue;
        };
        parsed.push((
            scope,
            DnsServer {
                address,
                port: Some(port),
            },
        ));
    }
    (parsed, malformed)
}

fn parse_domains(value: OwnedVariant) -> (Vec<(DnsScope, String, bool)>, bool) {
    let entries: Vec<(i32, String, bool)> = match value.try_into() {
        Ok(entries) => entries,
        Err(_) => return (Vec::new(), true),
    };

    let mut parsed = Vec::new();
    let mut malformed = false;
    for (ifindex, domain, route_only) in entries {
        let Some(scope) = scope_from_ifindex(ifindex) else {
            malformed = true;
            continue;
        };
        if domain.trim().is_empty() {
            malformed = true;
            continue;
        }
        parsed.push((scope, domain, route_only));
    }
    (parsed, malformed)
}

fn parse_mode(value: OwnedVariant) -> (Option<String>, bool) {
    let mode = match String::try_from(value) {
        Ok(mode) => mode,
        Err(_) => return (None, true),
    };
    if valid_mode(&mode) {
        (Some(mode), false)
    } else {
        (None, true)
    }
}

fn scope_from_ifindex(ifindex: i32) -> Option<DnsScope> {
    if ifindex == 0 {
        Some(DnsScope::Global)
    } else if ifindex > 0 {
        u32::try_from(ifindex)
            .ok()
            .map(|ifindex| DnsScope::Link { ifindex })
    } else {
        None
    }
}

fn parse_ip(family: i32, bytes: &[u8]) -> Option<IpAddr> {
    match family {
        2 if bytes.len() == 4 => Some(IpAddr::V4(Ipv4Addr::new(
            bytes[0], bytes[1], bytes[2], bytes[3],
        ))),
        10 if bytes.len() == 16 => {
            let mut octets = [0u8; 16];
            octets.copy_from_slice(bytes);
            Some(IpAddr::V6(Ipv6Addr::from(octets)))
        }
        _ => None,
    }
}

fn valid_mode(mode: &str) -> bool {
    !mode.is_empty()
        && mode.len() <= 64
        && mode.bytes().any(|byte| byte.is_ascii_alphanumeric())
        && mode
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'/'))
}
