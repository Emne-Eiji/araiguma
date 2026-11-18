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
/// If a request fails (for example after the socket died), the connection
/// is dropped and recreated on the next observation, so a single broken
/// socket degrades one run instead of poisoning the provider forever.
#[derive(Debug, Default)]
pub struct NetlinkProvider {
    handle: Option<Handle>,
}

impl NetlinkProvider {
    pub fn new() -> Self {
        Self { handle: None }
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
        fidelity_limited: false,
        notes: Vec::new(),
    }
}

async fn dump_routes(handle: &Handle, family: AddressFamily) -> SectionResult<Route> {
    let message = match family {
        AddressFamily::Ipv4 => RouteMessageBuilder::<Ipv4Addr>::new().build(),
        AddressFamily::Ipv6 => RouteMessageBuilder::<Ipv6Addr>::new().build(),
    };
    match handle
        .route()
        .get(message)
        .execute()
        .try_collect::<Vec<_>>()
        .await
    {
        Ok(messages) => SectionResult::Observed(
            messages
                .iter()
                .filter_map(translate::translate_route)
                .collect(),
        ),
        Err(error) => SectionResult::Failed(format!("route dump: {error}")),
    }
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

        let interfaces = match handle.link().get().execute().try_collect::<Vec<_>>().await {
            Ok(messages) => SectionResult::Observed(
                messages
                    .iter()
                    .filter_map(translate::translate_link)
                    .collect(),
            ),
            Err(error) => {
                self.handle = None;
                SectionResult::Failed(format!("link dump: {error}"))
            }
        };

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
                for message in &messages {
                    if let Some(address) = translate::translate_address(message) {
                        match address.family {
                            AddressFamily::Ipv4 => v4.push(address),
                            AddressFamily::Ipv6 => v6.push(address),
                        }
                    }
                }
                (SectionResult::Observed(v4), SectionResult::Observed(v6))
            }
            Err(error) => {
                self.handle = None;
                (
                    SectionResult::Failed(format!("address dump: {error}")),
                    SectionResult::Failed(format!("address dump: {error}")),
                )
            }
        };

        let routes_v4 = dump_routes(&handle, AddressFamily::Ipv4).await;
        let routes_v6 = dump_routes(&handle, AddressFamily::Ipv6).await;

        ProviderObservation {
            provider: ProviderId::Netlink,
            priority: 0,
            interfaces,
            addresses_v4,
            addresses_v6,
            routes_v4,
            routes_v6,
            // Netlink is the authoritative source; no fidelity caveats.
            fidelity_limited: false,
            notes: Vec::new(),
        }
    }
}
