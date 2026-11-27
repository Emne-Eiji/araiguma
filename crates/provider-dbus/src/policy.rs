//! The explicit D-Bus property allow-list.
//!
//! The foundation exposes no free-form method-call API and no generic
//! property reader. Every property read is a `Properties.Get` request that
//! must match one exact service, object-path pattern, interface, and
//! property below. Anything else is rejected locally before a message is
//! built or transmitted.
//!
//! The list is intentionally narrow. NetworkManager manager metadata and
//! Wi-Fi/access-point data, resolved link reads, `PropertiesChanged`, and
//! every settings/secrets method are absent by design.

use std::fmt;

use crate::error::DbusError;

/// The two optional D-Bus services approved for WO-06 observation.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Service {
    NetworkManager,
    SystemdResolved,
}

impl Service {
    /// The well-known bus name used for ownership preflight and calls.
    pub const fn bus_name(self) -> &'static str {
        match self {
            Self::NetworkManager => NETWORK_MANAGER_SERVICE,
            Self::SystemdResolved => SYSTEMD_RESOLVED_SERVICE,
        }
    }
}

pub const NETWORK_MANAGER_SERVICE: &str = "org.freedesktop.NetworkManager";
pub const SYSTEMD_RESOLVED_SERVICE: &str = "org.freedesktop.resolve1";

pub const NETWORK_MANAGER_MANAGER_PATH: &str = "/org/freedesktop/NetworkManager";
pub const NETWORK_MANAGER_DEVICE_PATH_PREFIX: &str = "/org/freedesktop/NetworkManager/Devices/";
pub const NETWORK_MANAGER_ACTIVE_CONNECTION_PATH_PREFIX: &str =
    "/org/freedesktop/NetworkManager/ActiveConnection/";
pub const SYSTEMD_RESOLVED_MANAGER_PATH: &str = "/org/freedesktop/resolve1";

pub const NETWORK_MANAGER_MANAGER_INTERFACE: &str = "org.freedesktop.NetworkManager";
pub const NETWORK_MANAGER_DEVICE_INTERFACE: &str = "org.freedesktop.NetworkManager.Device";
/// The interface real NetworkManager exports on active-connection objects
/// at `/org/freedesktop/NetworkManager/ActiveConnection/{N}`
/// (WO-06C-C1; previously misrecorded as
/// `org.freedesktop.NetworkManager.ActiveConnection`, which real
/// NetworkManager does not export).
pub const NETWORK_MANAGER_ACTIVE_CONNECTION_INTERFACE: &str =
    "org.freedesktop.NetworkManager.Connection.Active";
pub const SYSTEMD_RESOLVED_MANAGER_INTERFACE: &str = "org.freedesktop.resolve1.Manager";

/// A property request exactly as a caller wants to make it.
///
/// Construction does not grant access: [`approve`] must validate the
/// service/path/interface/property combination. The fields are public so
/// callers can express dynamic device/connection paths; the allow-list is
/// the enforcement point.
#[derive(Clone, Copy)]
pub struct PropertyRequest<'a> {
    pub service: Service,
    pub path: &'a str,
    pub interface: &'a str,
    pub property: &'a str,
}

impl<'a> PropertyRequest<'a> {
    pub const fn new(
        service: Service,
        path: &'a str,
        interface: &'a str,
        property: &'a str,
    ) -> Self {
        Self {
            service,
            path,
            interface,
            property,
        }
    }
}

impl fmt::Debug for PropertyRequest<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PropertyRequest")
            .field("service", &self.service)
            .field("path", &"<redacted>")
            .field("interface", &self.interface)
            .field("property", &self.property)
            .finish()
    }
}

/// A property request that has passed the allow-list.
///
/// Fields are private and the path is redacted from `Debug`; the only way
/// to obtain one is [`approve`].
#[derive(Clone, PartialEq, Eq)]
pub struct ApprovedProperty {
    service: Service,
    path: String,
    interface: &'static str,
    property: &'static str,
    signature: &'static str,
}

impl ApprovedProperty {
    pub fn service(&self) -> Service {
        self.service
    }

    pub fn path(&self) -> &str {
        &self.path
    }

    pub fn interface(&self) -> &'static str {
        self.interface
    }

    pub fn property(&self) -> &'static str {
        self.property
    }

    /// The D-Bus type signature the service must return for this property.
    ///
    /// `Properties.Get` wraps the value in a variant, so the signature is that
    /// of the variant's contents. A reply with any other signature is rejected
    /// as [`DbusError::MalformedResponse`] rather than handed to a provider.
    pub fn signature(&self) -> &'static str {
        self.signature
    }
}

impl fmt::Debug for ApprovedProperty {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ApprovedProperty")
            .field("service", &self.service)
            .field("path", &"<redacted>")
            .field("interface", &self.interface)
            .field("property", &self.property)
            .finish()
    }
}

/// Validate one request against the exact approved combinations.
///
/// Returns [`DbusError::UnsupportedRequest`] for every unapproved service,
/// path, interface, or property. The check happens before any proxy or
/// message is built, so a rejected request cannot reach the bus.
pub fn approve(request: PropertyRequest<'_>) -> Result<ApprovedProperty, DbusError> {
    let (interface, property, signature) = match request.service {
        Service::NetworkManager => approve_network_manager(&request)?,
        Service::SystemdResolved => approve_systemd_resolved(&request)?,
    };

    Ok(ApprovedProperty {
        service: request.service,
        path: request.path.to_owned(),
        interface,
        property,
        signature,
    })
}

fn approve_network_manager(
    request: &PropertyRequest<'_>,
) -> Result<(&'static str, &'static str, &'static str), DbusError> {
    if request.path == NETWORK_MANAGER_MANAGER_PATH {
        let property = exact_property(
            request,
            NETWORK_MANAGER_MANAGER_INTERFACE,
            &[("ActiveConnections", "ao"), ("Devices", "ao")],
        )?;
        return Ok((NETWORK_MANAGER_MANAGER_INTERFACE, property.0, property.1));
    }

    if numeric_suffix(request.path, NETWORK_MANAGER_DEVICE_PATH_PREFIX) {
        let property = exact_property(
            request,
            NETWORK_MANAGER_DEVICE_INTERFACE,
            &[("Interface", "s"), ("IpInterface", "s")],
        )?;
        return Ok((NETWORK_MANAGER_DEVICE_INTERFACE, property.0, property.1));
    }

    if numeric_suffix(request.path, NETWORK_MANAGER_ACTIVE_CONNECTION_PATH_PREFIX) {
        let property = exact_property(
            request,
            NETWORK_MANAGER_ACTIVE_CONNECTION_INTERFACE,
            &[
                ("Devices", "ao"),
                ("State", "u"),
                ("Type", "s"),
                ("Vpn", "b"),
            ],
        )?;
        return Ok((
            NETWORK_MANAGER_ACTIVE_CONNECTION_INTERFACE,
            property.0,
            property.1,
        ));
    }

    Err(DbusError::UnsupportedRequest)
}

fn approve_systemd_resolved(
    request: &PropertyRequest<'_>,
) -> Result<(&'static str, &'static str, &'static str), DbusError> {
    if request.path != SYSTEMD_RESOLVED_MANAGER_PATH {
        return Err(DbusError::UnsupportedRequest);
    }

    let property = exact_property(
        request,
        SYSTEMD_RESOLVED_MANAGER_INTERFACE,
        &[
            ("DNSEx", "a(iiayqs)"),
            ("Domains", "a(isb)"),
            ("ResolvConfMode", "s"),
        ],
    )?;
    Ok((SYSTEMD_RESOLVED_MANAGER_INTERFACE, property.0, property.1))
}

fn exact_property(
    request: &PropertyRequest<'_>,
    expected_interface: &'static str,
    allowed: &[(&'static str, &'static str)],
) -> Result<(&'static str, &'static str), DbusError> {
    if request.interface != expected_interface {
        return Err(DbusError::UnsupportedRequest);
    }
    allowed
        .iter()
        .copied()
        .find(|(property, _)| *property == request.property)
        .ok_or(DbusError::UnsupportedRequest)
}

/// Match one canonical numeric object-path id after `prefix`.
fn numeric_suffix(path: &str, prefix: &str) -> bool {
    let Some(suffix) = path.strip_prefix(prefix) else {
        return false;
    };
    if suffix.is_empty() || !suffix.bytes().all(|byte| byte.is_ascii_digit()) {
        return false;
    }
    !(suffix.len() > 1 && suffix.starts_with('0'))
}

/// Whether `path` is one exact NetworkManager device object path.
///
/// Device signals carry this path in their body; validating it keeps a
/// malformed or hostile signal from introducing an arbitrary object path into
/// the provider.
pub(crate) fn is_device_object_path(path: &str) -> bool {
    numeric_suffix(path, NETWORK_MANAGER_DEVICE_PATH_PREFIX)
}

#[cfg(test)]
mod tests {
    use super::{
        approve, PropertyRequest, Service, NETWORK_MANAGER_ACTIVE_CONNECTION_INTERFACE,
        NETWORK_MANAGER_ACTIVE_CONNECTION_PATH_PREFIX, NETWORK_MANAGER_DEVICE_INTERFACE,
        NETWORK_MANAGER_DEVICE_PATH_PREFIX, NETWORK_MANAGER_MANAGER_INTERFACE,
        NETWORK_MANAGER_MANAGER_PATH, SYSTEMD_RESOLVED_MANAGER_INTERFACE,
        SYSTEMD_RESOLVED_MANAGER_PATH,
    };
    use crate::error::DbusError;

    /// The interface a real NetworkManager service exports on
    /// active-connection objects, written independently of the production
    /// constant (WO-06C-C1) so the policy and the mocks cannot repeat the
    /// same interface mistake without detection.
    const REAL_ACTIVE_CONNECTION_INTERFACE: &str =
        "org.freedesktop.NetworkManager.Connection.Active";

    /// The pre-correction interface name, which real NetworkManager does not
    /// export; retained as a literal so its rejection is regression-tested.
    const LEGACY_ACTIVE_CONNECTION_INTERFACE: &str =
        "org.freedesktop.NetworkManager.ActiveConnection";

    fn nm<'a>(path: &'a str, interface: &'a str, property: &'a str) -> PropertyRequest<'a> {
        PropertyRequest::new(Service::NetworkManager, path, interface, property)
    }

    fn resolved<'a>(path: &'a str, interface: &'a str, property: &'a str) -> PropertyRequest<'a> {
        PropertyRequest::new(Service::SystemdResolved, path, interface, property)
    }

    #[test]
    fn approved_network_manager_manager_properties() {
        for property in ["ActiveConnections", "Devices"] {
            let approved = approve(nm(
                NETWORK_MANAGER_MANAGER_PATH,
                NETWORK_MANAGER_MANAGER_INTERFACE,
                property,
            ))
            .expect("approved manager property");
            assert_eq!(approved.interface(), NETWORK_MANAGER_MANAGER_INTERFACE);
            assert_eq!(approved.property(), property);
        }
    }

    #[test]
    fn approved_network_manager_device_properties() {
        let path = format!("{NETWORK_MANAGER_DEVICE_PATH_PREFIX}2");
        for property in ["Interface", "IpInterface"] {
            let approved = approve(nm(&path, NETWORK_MANAGER_DEVICE_INTERFACE, property))
                .expect("approved device property");
            assert_eq!(approved.interface(), NETWORK_MANAGER_DEVICE_INTERFACE);
            assert_eq!(approved.property(), property);
        }
    }

    #[test]
    fn active_connection_interface_is_the_real_network_manager_interface() {
        assert_eq!(
            NETWORK_MANAGER_ACTIVE_CONNECTION_INTERFACE,
            REAL_ACTIVE_CONNECTION_INTERFACE
        );
        assert_ne!(
            NETWORK_MANAGER_ACTIVE_CONNECTION_INTERFACE,
            LEGACY_ACTIVE_CONNECTION_INTERFACE
        );
    }

    #[test]
    fn legacy_active_connection_interface_is_rejected() {
        let path = format!("{NETWORK_MANAGER_ACTIVE_CONNECTION_PATH_PREFIX}7");
        for property in ["Devices", "State", "Type", "Vpn"] {
            assert_eq!(
                approve(nm(&path, LEGACY_ACTIVE_CONNECTION_INTERFACE, property)),
                Err(DbusError::UnsupportedRequest),
                "property {property}"
            );
        }
    }

    #[test]
    fn approved_network_manager_active_connection_properties() {
        let path = format!("{NETWORK_MANAGER_ACTIVE_CONNECTION_PATH_PREFIX}7");
        for property in ["Devices", "State", "Type", "Vpn"] {
            let approved = approve(nm(
                &path,
                NETWORK_MANAGER_ACTIVE_CONNECTION_INTERFACE,
                property,
            ))
            .expect("approved active-connection property");
            assert_eq!(
                approved.interface(),
                NETWORK_MANAGER_ACTIVE_CONNECTION_INTERFACE
            );
            assert_eq!(approved.property(), property);
        }
    }

    #[test]
    fn approved_systemd_resolved_manager_properties() {
        for property in ["DNSEx", "Domains", "ResolvConfMode"] {
            let approved = approve(resolved(
                SYSTEMD_RESOLVED_MANAGER_PATH,
                SYSTEMD_RESOLVED_MANAGER_INTERFACE,
                property,
            ))
            .expect("approved resolved manager property");
            assert_eq!(approved.interface(), SYSTEMD_RESOLVED_MANAGER_INTERFACE);
            assert_eq!(approved.property(), property);
        }
    }

    #[test]
    fn approved_properties_declare_expected_signatures() {
        let device = format!("{NETWORK_MANAGER_DEVICE_PATH_PREFIX}2");
        let active = format!("{NETWORK_MANAGER_ACTIVE_CONNECTION_PATH_PREFIX}7");
        let cases = [
            (
                nm(
                    NETWORK_MANAGER_MANAGER_PATH,
                    NETWORK_MANAGER_MANAGER_INTERFACE,
                    "ActiveConnections",
                ),
                "ao",
            ),
            (
                nm(
                    NETWORK_MANAGER_MANAGER_PATH,
                    NETWORK_MANAGER_MANAGER_INTERFACE,
                    "Devices",
                ),
                "ao",
            ),
            (
                nm(&device, NETWORK_MANAGER_DEVICE_INTERFACE, "Interface"),
                "s",
            ),
            (
                nm(&device, NETWORK_MANAGER_DEVICE_INTERFACE, "IpInterface"),
                "s",
            ),
            (
                nm(
                    &active,
                    NETWORK_MANAGER_ACTIVE_CONNECTION_INTERFACE,
                    "Devices",
                ),
                "ao",
            ),
            (
                nm(
                    &active,
                    NETWORK_MANAGER_ACTIVE_CONNECTION_INTERFACE,
                    "State",
                ),
                "u",
            ),
            (
                nm(&active, NETWORK_MANAGER_ACTIVE_CONNECTION_INTERFACE, "Type"),
                "s",
            ),
            (
                nm(&active, NETWORK_MANAGER_ACTIVE_CONNECTION_INTERFACE, "Vpn"),
                "b",
            ),
            (
                resolved(
                    SYSTEMD_RESOLVED_MANAGER_PATH,
                    SYSTEMD_RESOLVED_MANAGER_INTERFACE,
                    "DNSEx",
                ),
                "a(iiayqs)",
            ),
            (
                resolved(
                    SYSTEMD_RESOLVED_MANAGER_PATH,
                    SYSTEMD_RESOLVED_MANAGER_INTERFACE,
                    "Domains",
                ),
                "a(isb)",
            ),
            (
                resolved(
                    SYSTEMD_RESOLVED_MANAGER_PATH,
                    SYSTEMD_RESOLVED_MANAGER_INTERFACE,
                    "ResolvConfMode",
                ),
                "s",
            ),
        ];
        for (request, signature) in cases {
            assert_eq!(approve(request).expect("approved").signature(), signature);
        }
    }

    #[test]
    fn prohibited_operations_are_rejected() {
        for property in [
            "GetAll",
            "Set",
            "GetManagedObjects",
            "Introspect",
            "GetSettings",
            "GetSecrets",
            "Get",
        ] {
            assert_eq!(
                approve(nm(
                    NETWORK_MANAGER_MANAGER_PATH,
                    NETWORK_MANAGER_MANAGER_INTERFACE,
                    property,
                )),
                Err(DbusError::UnsupportedRequest),
                "property {property}"
            );
        }
    }

    #[test]
    fn manager_metadata_and_wifi_are_rejected() {
        for property in [
            "State",
            "Version",
            "Connectivity",
            "NetworkingEnabled",
            "WirelessEnabled",
            "WwanEnabled",
            "Metered",
            "Startup",
            "AllDevices",
            "PrimaryConnection",
            "PrimaryConnectionType",
            "GetDevices",
            "GetAllDevices",
            "Wireless",
            "WirelessCapabilities",
            "AccessPoints",
            "LastScan",
        ] {
            assert_eq!(
                approve(nm(
                    NETWORK_MANAGER_MANAGER_PATH,
                    NETWORK_MANAGER_MANAGER_INTERFACE,
                    property,
                )),
                Err(DbusError::UnsupportedRequest),
                "property {property}"
            );
        }
    }

    #[test]
    fn device_properties_outside_interface_mapping_are_rejected() {
        let path = format!("{NETWORK_MANAGER_DEVICE_PATH_PREFIX}2");
        for property in [
            "DeviceType",
            "State",
            "Driver",
            "Real",
            "Metered",
            "HwAddress",
            "ActiveConnection",
            "Ip4Config",
            "Ip6Config",
            "Dhcp4Config",
            "Dhcp6Config",
        ] {
            assert_eq!(
                approve(nm(&path, NETWORK_MANAGER_DEVICE_INTERFACE, property)),
                Err(DbusError::UnsupportedRequest),
                "property {property}"
            );
        }
    }

    #[test]
    fn active_connection_metadata_is_rejected() {
        let path = format!("{NETWORK_MANAGER_ACTIVE_CONNECTION_PATH_PREFIX}7");
        for property in [
            "Connection",
            "Ip4Config",
            "Ip6Config",
            "Master",
            "SpecificObject",
            "VpnType",
            "vpn-type",
            "Uuid",
            "Id",
        ] {
            assert_eq!(
                approve(nm(
                    &path,
                    NETWORK_MANAGER_ACTIVE_CONNECTION_INTERFACE,
                    property,
                )),
                Err(DbusError::UnsupportedRequest),
                "property {property}"
            );
        }
    }

    #[test]
    fn active_connection_object_path_is_strict() {
        for path in [
            "/org/freedesktop/NetworkManager/ActiveConnection",
            "/org/freedesktop/NetworkManager/ActiveConnection/",
            "/org/freedesktop/NetworkManager/ActiveConnection/01",
            "/org/freedesktop/NetworkManager/ActiveConnection/1/extra",
            "/org/freedesktop/NetworkManager/ActiveConnection/1evil",
            "/org/freedesktop/NetworkManager/ActiveConnection/abc",
            "/org/freedesktop/NetworkManager/ActiveConnection/-1",
            "/org/freedesktop/NetworkManager/ActiveConnection/ 1",
        ] {
            assert_eq!(
                approve(nm(
                    path,
                    NETWORK_MANAGER_ACTIVE_CONNECTION_INTERFACE,
                    "State",
                )),
                Err(DbusError::UnsupportedRequest),
                "path {path}"
            );
        }
    }

    #[test]
    fn resolved_non_approved_and_link_properties_are_rejected() {
        for property in [
            "DNS",
            "FallbackDNS",
            "LLMNR",
            "MulticastDNS",
            "DNSOverTLS",
            "DNSSEC",
            "DNSSECNegativeTrustAnchors",
            "CurrentDNSServer",
            "DNSStubListener",
            "CacheStatistics",
            "ScopesMask",
            "dnsex",
            "DomainsEx",
        ] {
            assert_eq!(
                approve(resolved(
                    SYSTEMD_RESOLVED_MANAGER_PATH,
                    SYSTEMD_RESOLVED_MANAGER_INTERFACE,
                    property,
                )),
                Err(DbusError::UnsupportedRequest),
                "property {property}"
            );
        }

        let link = format!("{SYSTEMD_RESOLVED_MANAGER_PATH}/link/2");
        assert_eq!(
            approve(resolved(&link, "org.freedesktop.resolve1.Link", "DNSEx",)),
            Err(DbusError::UnsupportedRequest)
        );
    }

    #[test]
    fn service_interface_and_path_mismatches_are_rejected() {
        assert_eq!(
            approve(resolved(
                SYSTEMD_RESOLVED_MANAGER_PATH,
                NETWORK_MANAGER_MANAGER_INTERFACE,
                "DNSEx",
            )),
            Err(DbusError::UnsupportedRequest)
        );
        assert_eq!(
            approve(nm(
                NETWORK_MANAGER_MANAGER_PATH,
                SYSTEMD_RESOLVED_MANAGER_INTERFACE,
                "Devices",
            )),
            Err(DbusError::UnsupportedRequest)
        );
        assert_eq!(
            approve(PropertyRequest::new(
                Service::NetworkManager,
                "/org/freedesktop/NetworkManager/",
                NETWORK_MANAGER_MANAGER_INTERFACE,
                "Devices",
            )),
            Err(DbusError::UnsupportedRequest)
        );
    }

    #[test]
    fn object_path_pattern_is_strict() {
        for path in [
            NETWORK_MANAGER_DEVICE_PATH_PREFIX.to_owned(),
            format!("{NETWORK_MANAGER_DEVICE_PATH_PREFIX}01"),
            format!("{NETWORK_MANAGER_DEVICE_PATH_PREFIX}1/extra"),
            format!("{NETWORK_MANAGER_DEVICE_PATH_PREFIX}1evil"),
            format!("{NETWORK_MANAGER_DEVICE_PATH_PREFIX}abc"),
            format!("{NETWORK_MANAGER_DEVICE_PATH_PREFIX}1/"),
            format!("{NETWORK_MANAGER_DEVICE_PATH_PREFIX}-1"),
            format!("{NETWORK_MANAGER_DEVICE_PATH_PREFIX} 1"),
        ] {
            assert_eq!(
                approve(nm(&path, NETWORK_MANAGER_DEVICE_INTERFACE, "Interface")),
                Err(DbusError::UnsupportedRequest),
                "path {path:?}"
            );
        }

        for path in [
            "/org/freedesktop/resolve1/link/2".to_owned(),
            "/org/freedesktop/resolve1/".to_owned(),
            "/org/freedesktop/resolve1/manager".to_owned(),
            "/org/freedesktop/resolve1/link".to_owned(),
        ] {
            assert_eq!(
                approve(resolved(&path, SYSTEMD_RESOLVED_MANAGER_INTERFACE, "DNSEx")),
                Err(DbusError::UnsupportedRequest),
                "path {path:?}"
            );
        }
    }

    #[test]
    fn empty_values_are_rejected() {
        for request in [
            nm("", NETWORK_MANAGER_MANAGER_INTERFACE, "Devices"),
            nm(NETWORK_MANAGER_MANAGER_PATH, "", "Devices"),
            nm(
                NETWORK_MANAGER_MANAGER_PATH,
                NETWORK_MANAGER_MANAGER_INTERFACE,
                "",
            ),
            resolved("", SYSTEMD_RESOLVED_MANAGER_INTERFACE, "DNSEx"),
            resolved(SYSTEMD_RESOLVED_MANAGER_PATH, "", "DNSEx"),
            resolved(
                SYSTEMD_RESOLVED_MANAGER_PATH,
                SYSTEMD_RESOLVED_MANAGER_INTERFACE,
                "",
            ),
        ] {
            assert_eq!(approve(request), Err(DbusError::UnsupportedRequest));
        }
    }

    #[test]
    fn debug_redacts_dynamic_paths() {
        let path = format!("{NETWORK_MANAGER_DEVICE_PATH_PREFIX}42");
        let approved =
            approve(nm(&path, NETWORK_MANAGER_DEVICE_INTERFACE, "Interface")).expect("approved");
        let request = nm(&path, NETWORK_MANAGER_DEVICE_INTERFACE, "Interface");
        for text in [format!("{approved:?}"), format!("{request:?}")] {
            assert!(!text.contains("42"), "path leaked in {text}");
            assert!(text.contains("<redacted>"));
        }
    }
}
