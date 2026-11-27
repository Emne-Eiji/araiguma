//! # araiguma-netlink
//!
//! The rtnetlink discovery provider (the primary, authoritative source) and
//! the multicast change [`Monitor`].
//!
//! Everything here is strictly read-only: dump requests (`RTM_GETLINK`,
//! `RTM_GETADDR`, `RTM_GETROUTE`) and multicast subscriptions. No
//! set/append/delete requests are constructed anywhere in this crate.
//!
//! The kernel-message mapping lives in [`translate`] as pure functions;
//! [`NetlinkProvider`] wires those to live sockets, and the netns
//! integration tests exercise both against a real kernel inside an
//! unprivileged transient network namespace.

pub mod monitor;
pub mod translate;

use std::net::{Ipv4Addr, Ipv6Addr};

use araiguma_core::model::{Address, AddressFamily, Route};
use araiguma_core::provider::{DiscoveryProvider, ProviderId, ProviderObservation, SectionResult};
use futures::TryStreamExt;
use rtnetlink::{new_connection, Handle, RouteMessageBuilder};

pub use monitor::{Monitor, Signal};

/// The rtnetlink discovery provider.
///
/// A netlink connection is created lazily and reused across observations.
/// Connection lifecycle (WO-03-F / AUD-14): EVERY dump failure — link,
/// address, or route — resets the cached connection the same way, so a
/// broken socket degrades exactly one observation run instead of poisoning
/// the provider forever; the next observation re-establishes a fresh
/// connection. There is no retry loop: one attempt per section per run,
/// failures surface as `Failed` sections with the error preserved.
#[derive(Debug, Default)]
pub struct NetlinkProvider {
    handle: Option<Handle>,
}

impl NetlinkProvider {
    pub fn new() -> Self {
        Self { handle: None }
    }

    /// Test seam (WO-03-F / AUD-14): a provider wired to an injected
    /// connection handle, so dump-failure recovery is testable against a
    /// real-but-broken socket without depending on kernel failure modes.
    #[doc(hidden)]
    pub fn with_handle(handle: Handle) -> Self {
        Self {
            handle: Some(handle),
        }
    }

    async fn handle(&mut self) -> Result<Handle, String> {
        if let Some(handle) = &self.handle {
            return Ok(handle.clone());
        }
        match new_connection() {
            Ok((connection, handle, _)) => {
                tokio::spawn(connection);
                self.handle = Some(handle.clone());
                Ok(handle)
            }
            Err(error) => Err(format!("netlink socket: {error}")),
        }
    }

    /// Drop the cached connection (called on ANY dump failure; see the
    /// type documentation — the next observation reconnects).
    fn reset_connection(&mut self) {
        self.handle = None;
    }
}

fn failed_observation(error: &str) -> ProviderObservation {
    ProviderObservation {
        provider: ProviderId::Netlink,
        priority: 0,
        interfaces: SectionResult::Failed(error.to_string()),
        addresses_v4: SectionResult::Failed(error.to_string()),
        addresses_v6: SectionResult::Failed(error.to_string()),
        routes_v4: SectionResult::Failed(error.to_string()),
        routes_v6: SectionResult::Failed(error.to_string()),
        dns: SectionResult::Unsupported,
        connections: SectionResult::Unsupported,
        fidelity_limited: false,
        notes: Vec::new(),
    }
}

/// One route-family dump. Errors are returned to the caller, which
/// applies the uniform connection-reset policy (see [`NetlinkProvider`]).
async fn dump_routes(
    handle: &Handle,
    family: AddressFamily,
) -> Result<Vec<netlink_packet_route::route::RouteMessage>, rtnetlink::Error> {
    let message = match family {
        AddressFamily::Ipv4 => RouteMessageBuilder::<Ipv4Addr>::new().build(),
        AddressFamily::Ipv6 => RouteMessageBuilder::<Ipv6Addr>::new().build(),
    };
    handle
        .route()
        .get(message)
        .execute()
        .try_collect::<Vec<_>>()
        .await
}

/// Translate route messages, counting untranslatable ones (§5.2).
fn translate_routes(messages: &[netlink_packet_route::route::RouteMessage]) -> (Vec<Route>, usize) {
    let mut items = Vec::with_capacity(messages.len());
    let mut dropped = 0usize;
    for message in messages {
        match translate::translate_route(message) {
            Some(route) => items.push(route),
            None => dropped += 1,
        }
    }
    (items, dropped)
}

#[async_trait::async_trait]
impl DiscoveryProvider for NetlinkProvider {
    fn id(&self) -> ProviderId {
        ProviderId::Netlink
    }

    fn priority(&self) -> u8 {
        0
    }

    async fn observe(&mut self) -> ProviderObservation {
        let handle = match self.handle().await {
            Ok(handle) => handle,
            Err(error) => return failed_observation(&error),
        };

        let mut notes: Vec<String> = Vec::new();

        // Link dump. Any failure resets the connection (AUD-14) — the
        // next observation builds a fresh one.
        let interfaces = match handle.link().get().execute().try_collect::<Vec<_>>().await {
            Ok(messages) => {
                let mut items = Vec::with_capacity(messages.len());
                let mut dropped = 0usize;
                for message in &messages {
                    match translate::translate_link(message) {
                        Some(interface) => items.push(interface),
                        None => dropped += 1,
                    }
                }
                if dropped > 0 {
                    notes.push(format!(
                        "{dropped} link message(s) from the dump could not be translated into the model \
                         (e.g. missing name or ifindex); they are not represented — the rest of the \
                         dump stands"
                    ));
                }
                SectionResult::Observed(items)
            }
            Err(error) => {
                self.reset_connection();
                SectionResult::Failed(format!("link dump: {error}"))
            }
        };

        // Address dump (v4 and v6 share one request; the split is by family).
        let (addresses_v4, addresses_v6) = match handle
            .address()
            .get()
            .execute()
            .try_collect::<Vec<_>>()
            .await
        {
            Ok(messages) => {
                let mut v4: Vec<Address> = Vec::new();
                let mut v6: Vec<Address> = Vec::new();
                let mut dropped = 0usize;
                for message in &messages {
                    match translate::translate_address(message) {
                        Some(address) => match address.family {
                            AddressFamily::Ipv4 => v4.push(address),
                            AddressFamily::Ipv6 => v6.push(address),
                        },
                        None => dropped += 1,
                    }
                }
                if dropped > 0 {
                    notes.push(format!(
                        "{dropped} address message(s) from the dump could not be translated into the model \
                         (e.g. no address attribute or unknown family); they are not represented — the \
                         rest of the dump stands"
                    ));
                }
                (SectionResult::Observed(v4), SectionResult::Observed(v6))
            }
            Err(error) => {
                self.reset_connection();
                (
                    SectionResult::Failed(format!("address dump: {error}")),
                    SectionResult::Failed(format!("address dump: {error}")),
                )
            }
        };

        // Route dumps — the SAME recovery policy as link and address dumps
        // (AUD-14: previously only link/address failures reset the
        // connection; a route-dump failure left the broken connection
        // cached for the next run).
        let routes_v4 = match dump_routes(&handle, AddressFamily::Ipv4).await {
            Ok(messages) => {
                let (items, dropped) = translate_routes(&messages);
                if dropped > 0 {
                    notes.push(format!(
                        "{dropped} IPv4 route message(s) from the dump could not be translated into \
                         the model; they are not represented — the rest of the dump stands"
                    ));
                }
                SectionResult::Observed(items)
            }
            Err(error) => {
                self.reset_connection();
                SectionResult::Failed(format!("route dump: {error}"))
            }
        };
        let routes_v6 = match dump_routes(&handle, AddressFamily::Ipv6).await {
            Ok(messages) => {
                let (items, dropped) = translate_routes(&messages);
                if dropped > 0 {
                    notes.push(format!(
                        "{dropped} IPv6 route message(s) from the dump could not be translated into \
                         the model; they are not represented — the rest of the dump stands"
                    ));
                }
                SectionResult::Observed(items)
            }
            Err(error) => {
                self.reset_connection();
                SectionResult::Failed(format!("route dump: {error}"))
            }
        };

        ProviderObservation {
            provider: ProviderId::Netlink,
            priority: 0,
            interfaces,
            addresses_v4,
            addresses_v6,
            routes_v4,
            routes_v6,
            dns: SectionResult::Unsupported,
            connections: SectionResult::Unsupported,
            // Netlink is the authoritative source; no fidelity caveats.
            fidelity_limited: false,
            notes,
        }
    }
}
