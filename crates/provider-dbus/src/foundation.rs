//! The constrained bus connection and the two approved operations.
//!
//! One [`DbusFoundation`] owns exactly one system-bus connection for exactly
//! one optional service (NetworkManager or systemd-resolved). This matches the
//! per-provider connection rule: the two optional providers never share a
//! connection, and the desktop application never constructs one.
//!
//! The only application-service operations are:
//!
//! * `org.freedesktop.DBus.Properties.Get` for a property on the explicit
//!   allow-list in [`crate::policy`], and
//! * the three NetworkManager signals in [`crate::signals`].
//!
//! Ownership is checked before any application call with `NameHasOwner`, and
//! the unique owner is pinned with `GetNameOwner` for signal validation. Both
//! are bus-driver operations. Every application call carries `NO_AUTO_START`
//! and every proxy is built with property caching disabled, so no `GetAll`
//! request is ever generated.

use std::future::Future;
use std::time::Duration;

use zbus::names::OwnedUniqueName;
use zbus::proxy::{CacheProperties, MethodFlags};
use zbus::{Connection, OwnedVariant, Proxy};

use crate::error::DbusError;
use crate::policy::{self, PropertyRequest, Service};
use crate::signals::{self, SignalKind, SignalSubscription};

/// Default deadline for connecting, preflight, one property read, signal
/// setup, and shutdown. Providers may choose a different bounded value.
pub const DEFAULT_OPERATION_TIMEOUT: Duration = Duration::from_secs(5);

const BUS_DRIVER_SERVICE: &str = "org.freedesktop.DBus";
const BUS_DRIVER_PATH: &str = "/org/freedesktop/DBus";
const BUS_DRIVER_INTERFACE: &str = "org.freedesktop.DBus";
const PROPERTIES_INTERFACE: &str = "org.freedesktop.DBus.Properties";

/// A constrained connection bound to one optional service.
pub struct DbusFoundation {
    service: Service,
    connection: Connection,
    owner: OwnedUniqueName,
    timeout: Duration,
}

impl DbusFoundation {
    /// Connect to the system bus and preflight ownership of `service`.
    ///
    /// The system bus address is resolved by zbus from
    /// `DBUS_SYSTEM_BUS_ADDRESS` and the platform default, exactly like any
    /// other system-bus client. If the service has no owner the call fails
    /// with [`DbusError::ServiceUnavailable`] before any application message
    /// is sent; no service activation is attempted.
    pub async fn connect(service: Service) -> Result<Self, DbusError> {
        Self::connect_with_timeout(service, DEFAULT_OPERATION_TIMEOUT).await
    }

    /// [`DbusFoundation::connect`] with an explicit bounded deadline.
    pub async fn connect_with_timeout(
        service: Service,
        timeout: Duration,
    ) -> Result<Self, DbusError> {
        if timeout.is_zero() {
            return Err(DbusError::UnsupportedRequest);
        }

        let connection = match tokio::time::timeout(
            timeout,
            zbus::connection::Builder::system()
                .method_timeout(timeout)
                .build(),
        )
        .await
        {
            Ok(Ok(connection)) => connection,
            Ok(Err(error)) => return Err(DbusError::from(error)),
            Err(_) => return Err(DbusError::Timeout),
        };

        let owner = match preflight(&connection, service, timeout).await {
            Ok(owner) => owner,
            Err(error) => {
                let _ = connection.close().await;
                return Err(error);
            }
        };

        Ok(Self {
            service,
            connection,
            owner,
            timeout,
        })
    }

    /// The service this connection is bound to.
    pub fn service(&self) -> Service {
        self.service
    }

    /// Read one allow-listed property with an explicit `Properties.Get`.
    ///
    /// The request is validated locally before a proxy or message is built.
    /// Unapproved service/path/interface/property combinations return
    /// [`DbusError::UnsupportedRequest`] and produce no wire traffic. The
    /// reply signature is checked against the approved property type; a
    /// mismatch returns [`DbusError::MalformedResponse`].
    pub async fn read_property(
        &self,
        request: PropertyRequest<'_>,
    ) -> Result<OwnedVariant, DbusError> {
        let approved = policy::approve(request)?;
        if approved.service() != self.service {
            return Err(DbusError::UnsupportedRequest);
        }

        let proxy = zbus::proxy::Builder::<Proxy<'_>>::new(&self.connection)
            .destination(self.service.bus_name())
            .path(approved.path())
            .interface(PROPERTIES_INTERFACE)
            .cache_properties(CacheProperties::No)
            .build()
            .await
            .map_err(DbusError::from)?;

        let value = bounded(
            self.timeout,
            proxy.call_with_flags::<_, _, OwnedVariant>(
                "Get",
                MethodFlags::NO_AUTO_START,
                &(approved.interface(), approved.property()),
            ),
        )
        .await?;

        if value.value_signature().to_string() != approved.signature() {
            return Err(DbusError::MalformedResponse);
        }

        Ok(value)
    }

    /// Subscribe to a subset of the three approved NetworkManager signals.
    ///
    /// Only [`Service::NetworkManager`] has approved signals; any request on a
    /// resolved connection returns [`DbusError::UnsupportedRequest`]. The
    /// slice must be non-empty and free of duplicates.
    pub async fn subscribe(&self, kinds: &[SignalKind]) -> Result<SignalSubscription, DbusError> {
        if self.service != Service::NetworkManager {
            return Err(DbusError::UnsupportedRequest);
        }
        signals::subscribe(&self.connection, &self.owner, kinds, self.timeout).await
    }

    /// Close the connection within the configured deadline.
    pub async fn shutdown(self) -> Result<(), DbusError> {
        match tokio::time::timeout(self.timeout, self.connection.close()).await {
            Ok(Ok(())) => Ok(()),
            Ok(Err(error)) => Err(DbusError::from(error)),
            Err(_) => Err(DbusError::Timeout),
        }
    }
}

impl std::fmt::Debug for DbusFoundation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DbusFoundation")
            .field("service", &self.service)
            .finish_non_exhaustive()
    }
}

async fn preflight(
    connection: &Connection,
    service: Service,
    timeout: Duration,
) -> Result<OwnedUniqueName, DbusError> {
    let bus = zbus::proxy::Builder::<Proxy<'static>>::new(connection)
        .destination(BUS_DRIVER_SERVICE)
        .path(BUS_DRIVER_PATH)
        .interface(BUS_DRIVER_INTERFACE)
        .cache_properties(CacheProperties::No)
        .build()
        .await
        .map_err(DbusError::from)?;

    let has_owner: bool = bounded(
        timeout,
        bus.call_with_flags(
            "NameHasOwner",
            MethodFlags::NO_AUTO_START,
            &(service.bus_name(),),
        ),
    )
    .await?;
    if !has_owner {
        return Err(DbusError::ServiceUnavailable);
    }

    bounded(
        timeout,
        bus.call_with_flags(
            "GetNameOwner",
            MethodFlags::NO_AUTO_START,
            &(service.bus_name(),),
        ),
    )
    .await
}

async fn bounded<T>(
    timeout: Duration,
    call: impl Future<Output = zbus::Result<Option<T>>>,
) -> Result<T, DbusError> {
    match tokio::time::timeout(timeout, call).await {
        Ok(Ok(Some(value))) => Ok(value),
        Ok(Ok(None)) => Err(DbusError::MalformedResponse),
        Ok(Err(error)) => Err(DbusError::from(error)),
        Err(_) => Err(DbusError::Timeout),
    }
}

#[cfg(test)]
mod tests {
    use super::DEFAULT_OPERATION_TIMEOUT;
    use std::time::Duration;

    #[test]
    fn default_timeout_is_bounded_and_non_zero() {
        assert!(DEFAULT_OPERATION_TIMEOUT > Duration::ZERO);
        assert!(DEFAULT_OPERATION_TIMEOUT <= Duration::from_secs(30));
    }
}
