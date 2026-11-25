//! # araiguma-engine
//!
//! The shared observation engine: providers, envelopes, and the change-feed
//! abstraction. Owned by the CLI and the desktop shell — one engine, two
//! transports (document 01 §4.1, document 02).
//!
//! Key types: [`Engine`] (the engine), [`ProviderRegistry`] (deterministic
//! provider registration), [`Envelope`] (document 04 §7),
//! [`ChangeFeed`]/[`NetlinkChangeFeed`]/[`FakeFeed`] (document 02,
//! `ChangeFeed`), and [`debounce`] (the burst-collapse window).
//!
//! Split of ownership: the engine owns the observation state (providers,
//! current document, envelope sequence, subscribers); the CONSUMER owns the
//! change feed and drives it. The consumer waits for a signal and then
//! calls [`Engine::apply_change`] or [`Engine::apply_resync`]. That keeps
//! the engine lock free of blocking waits, so the desktop shell can answer
//! IPC commands while it waits for the next network change. Optional
//! providers are refreshed independently on the optional poll cadence and
//! never delay the baseline opening snapshot.

pub mod change_feed;
pub mod engine;

pub use change_feed::{
    debounce, default_change_feed, Burst, ChangeFeed, FakeFeed, NetlinkChangeFeed,
};
pub use engine::{
    Engine, Envelope, EnvelopeKind, MonitorHealth, MonitorHealthState, ProviderRegistry, DEBOUNCE,
    OPTIONAL_POLL_INTERVAL,
};
