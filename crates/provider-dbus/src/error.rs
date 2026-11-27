//! Bounded error categories for the constrained D-Bus foundation.
//!
//! zbus errors and service-returned D-Bus error text are never retained in
//! this type. The public representation is a fixed code and a fixed
//! description; callers can map the category onto the accepted provider
//! section outcomes without ever seeing a service path, a DNS value, a
//! connection identifier, or arbitrary error text.

use std::fmt;

/// A bounded failure category produced by the D-Bus foundation.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum DbusError {
    /// The system bus could not be reached or the connection was lost.
    BusUnavailable,
    /// The target service has no owner, or disappeared.
    ServiceUnavailable,
    /// Access was denied by bus policy or the service.
    PermissionDenied,
    /// A bounded operation exceeded its deadline.
    Timeout,
    /// The reply could not be decoded into the expected shape.
    MalformedResponse,
    /// The request is not on the approved allow-list.
    UnsupportedRequest,
    /// A delivered message did not match the approved signal rule.
    RejectedSignal,
    /// Any other bounded protocol failure.
    Protocol,
}

impl DbusError {
    /// Stable, bounded code suitable for provider notes and diagnostics.
    pub const fn code(self) -> &'static str {
        match self {
            Self::BusUnavailable => "busUnavailable",
            Self::ServiceUnavailable => "serviceUnavailable",
            Self::PermissionDenied => "permissionDenied",
            Self::Timeout => "timeout",
            Self::MalformedResponse => "malformedResponse",
            Self::UnsupportedRequest => "unsupportedRequest",
            Self::RejectedSignal => "rejectedSignal",
            Self::Protocol => "protocolError",
        }
    }
}

impl fmt::Display for DbusError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let description = match self {
            Self::BusUnavailable => "D-Bus bus unavailable",
            Self::ServiceUnavailable => "D-Bus service unavailable",
            Self::PermissionDenied => "D-Bus access denied",
            Self::Timeout => "D-Bus operation timed out",
            Self::MalformedResponse => "D-Bus response malformed",
            Self::UnsupportedRequest => "D-Bus request not permitted",
            Self::RejectedSignal => "D-Bus signal rejected",
            Self::Protocol => "D-Bus protocol error",
        };
        f.write_str(description)
    }
}

impl std::error::Error for DbusError {}

impl From<zbus::Error> for DbusError {
    fn from(error: zbus::Error) -> Self {
        use zbus::fdo;
        use zbus::Error;

        match error {
            Error::FDO(error) => match *error {
                fdo::Error::ServiceUnknown(_) | fdo::Error::NameHasNoOwner(_) => {
                    Self::ServiceUnavailable
                }
                fdo::Error::AccessDenied(_) | fdo::Error::AuthFailed(_) => Self::PermissionDenied,
                fdo::Error::NoReply(_) | fdo::Error::TimedOut(_) | fdo::Error::Timeout(_) => {
                    Self::Timeout
                }
                fdo::Error::Disconnected(_)
                | fdo::Error::NoServer(_)
                | fdo::Error::IOError(_)
                | fdo::Error::NoNetwork(_) => Self::BusUnavailable,
                fdo::Error::UnknownMethod(_)
                | fdo::Error::UnknownProperty(_)
                | fdo::Error::UnknownInterface(_)
                | fdo::Error::UnknownObject(_)
                | fdo::Error::InvalidArgs(_)
                | fdo::Error::InvalidSignature(_) => Self::MalformedResponse,
                fdo::Error::ZBus(error) => Self::from(error),
                _ => Self::Protocol,
            },
            Error::MethodError(name, _, _) => {
                if name.as_str().ends_with(".AccessDenied") {
                    Self::PermissionDenied
                } else if name.as_str().ends_with(".ServiceUnknown")
                    || name.as_str().ends_with(".NameHasNoOwner")
                {
                    Self::ServiceUnavailable
                } else if name.as_str().ends_with(".NoReply")
                    || name.as_str().ends_with(".TimedOut")
                    || name.as_str().ends_with(".Timeout")
                {
                    Self::Timeout
                } else {
                    Self::Protocol
                }
            }
            Error::InputOutput(_)
            | Error::Connection(_, _)
            | Error::Handshake(_)
            | Error::Address(_) => Self::BusUnavailable,
            Error::Failure(_)
            | Error::IncorrectType
            | Error::Utf8(_)
            | Error::PaddingNot0(_)
            | Error::UnknownFd
            | Error::SignatureMismatch(_, _)
            | Error::OutOfBounds
            | Error::MaxDepthExceeded(_)
            | Error::SignatureParse(_)
            | Error::EmptyStructure
            | Error::InvalidObjectPath
            | Error::InvalidField
            | Error::ExcessData
            | Error::IncorrectEndian
            | Error::InvalidReply
            | Error::MissingField
            | Error::InvalidSerial => Self::MalformedResponse,
            _ => Self::Protocol,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::DbusError;
    use zbus::fdo;

    const SENSITIVE: &str = "org.example.Error: secret /org/freedesktop/NetworkManager/Devices/3";

    fn assert_bounded(error: DbusError) {
        let display = error.to_string();
        let debug = format!("{error:?}");
        assert!(!display.contains("secret"));
        assert!(!display.contains("/org/"));
        assert!(!debug.contains("secret"));
        assert!(!debug.contains("/org/"));
        assert!(!error.code().is_empty());
    }

    #[test]
    fn categories_have_static_bounded_text() {
        for error in [
            DbusError::BusUnavailable,
            DbusError::ServiceUnavailable,
            DbusError::PermissionDenied,
            DbusError::Timeout,
            DbusError::MalformedResponse,
            DbusError::UnsupportedRequest,
            DbusError::RejectedSignal,
            DbusError::Protocol,
        ] {
            assert_bounded(error);
        }
    }

    #[test]
    fn fdo_errors_map_to_bounded_categories_without_text() {
        let cases = [
            (
                fdo::Error::ServiceUnknown(SENSITIVE.into()),
                DbusError::ServiceUnavailable,
            ),
            (
                fdo::Error::NameHasNoOwner(SENSITIVE.into()),
                DbusError::ServiceUnavailable,
            ),
            (
                fdo::Error::AccessDenied(SENSITIVE.into()),
                DbusError::PermissionDenied,
            ),
            (
                fdo::Error::AuthFailed(SENSITIVE.into()),
                DbusError::PermissionDenied,
            ),
            (fdo::Error::NoReply(SENSITIVE.into()), DbusError::Timeout),
            (fdo::Error::TimedOut(SENSITIVE.into()), DbusError::Timeout),
            (
                fdo::Error::UnknownProperty(SENSITIVE.into()),
                DbusError::MalformedResponse,
            ),
            (
                fdo::Error::Disconnected(SENSITIVE.into()),
                DbusError::BusUnavailable,
            ),
            (fdo::Error::Failed(SENSITIVE.into()), DbusError::Protocol),
        ];

        for (source, expected) in cases {
            let mapped = DbusError::from(zbus::Error::FDO(Box::new(source)));
            assert_eq!(mapped, expected);
            assert_bounded(mapped);
        }
    }

    #[test]
    fn generic_zbus_errors_are_bounded() {
        let cases = [
            (
                zbus::Error::Failure(SENSITIVE.into()),
                DbusError::MalformedResponse,
            ),
            (
                zbus::Error::InputOutput(std::sync::Arc::new(std::io::Error::other(SENSITIVE))),
                DbusError::BusUnavailable,
            ),
            (zbus::Error::InvalidReply, DbusError::MalformedResponse),
        ];
        for (source, expected) in cases {
            let mapped = DbusError::from(source);
            assert_eq!(mapped, expected);
            assert_bounded(mapped);
        }
    }
}
