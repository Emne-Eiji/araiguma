//! # araiguma-provider-dbus
//!
//! A deliberately small, read-only D-Bus foundation for Araiguma's two
//! optional providers. It is not a general-purpose D-Bus client.
//!
//! ## What this crate may do
//!
//! * Open one system-bus connection per optional service.
//! * Ask the bus driver whether the service is owned (`NameHasOwner`) and pin
//!   its unique owner (`GetNameOwner`).
//! * Read the exact allow-listed properties through
//!   `org.freedesktop.DBus.Properties.Get`.
//! * Subscribe to exactly three NetworkManager signals: `StateChanged`,
//!   `DeviceAdded`, and `DeviceRemoved`.
//!
//! ## What this crate may never do
//!
//! * Activate a service. Every application call carries `NO_AUTO_START`.
//! * Read a property outside [`policy`], including `GetAll`, `Set`,
//!   `GetManagedObjects`, `Introspect`, `GetSettings`, `GetSecrets`, any
//!   link-level resolved property, NetworkManager manager metadata, Wi-Fi or
//!   access-point data, or secrets.
//! * Enumerate bus names, objects, or services.
//! * Subscribe to `PropertiesChanged`, `NameOwnerChanged`, or any other signal.
//! * Retain or expose raw zbus errors, D-Bus error strings, object paths, DNS
//!   values, connection identifiers, addresses, or arbitrary service text in
//!   public diagnostics.
//!
//! Property values are returned as [`zbus::OwnedVariant`] after the approved
//! signature is verified. The caller (the eventual NetworkManager and
//! systemd-resolved providers) owns decoding and must not log the raw value
//! into public diagnostics.
//!
//! The dependency is pinned exactly to `zbus = 6.5.0` with only the `tokio`
//! and `proxy` features. No desktop/frontend crate depends on this crate.

#![forbid(unsafe_code)]

mod error;
mod foundation;
pub mod policy;
mod signals;

pub use error::DbusError;
pub use foundation::{DbusFoundation, DEFAULT_OPERATION_TIMEOUT};
pub use policy::{PropertyRequest, Service};
pub use signals::{SignalEvent, SignalKind, SignalSubscription};
