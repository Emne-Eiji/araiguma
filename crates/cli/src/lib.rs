//! # araiguma-cli
//!
//! The read-only Araiguma command line interface.
//!
//! ```text
//! araiguma snapshot [--pretty]   one merged observation as JSON
//! araiguma watch                 NDJSON event stream (snapshot + changes)
//! araiguma --help | --version
//! ```
//!
//! The CLI never modifies network state; it observes through the netlink
//! and sysfs/procfs providers, merges them, and prints. Failures of
//! individual providers are visible in the `providers` section of the
//! snapshot, never hidden.

use std::io::Read as _;
use std::time::{SystemTime, UNIX_EPOCH};

use araiguma_core::provider::DiscoveryProvider;
use araiguma_core::{
    build_topology, diff_snapshots, merge, NetworkEvent, NetworkSnapshot, TopologyDocument,
};
use araiguma_netlink::{Monitor, NetlinkProvider, Signal};
use araiguma_sysfs::SysFsProvider;

/// How long the watch loop keeps absorbing bursty notifications before it
/// re-observes (bursty kernel updates collapse into one diff).
const DEBOUNCE: std::time::Duration = std::time::Duration::from_millis(150);

pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Parsed command line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    Snapshot {
        pretty: bool,
    },
    Watch,
    /// Project a snapshot JSON file (or stdin) into a topology document.
    Topology {
        input: Option<String>,
        pretty: bool,
    },
    Help,
    Version,
}

pub fn usage() -> String {
    "\
araiguma — read-only network observability (observe, do not control)

USAGE:
    araiguma snapshot [--pretty]   Print one merged observation as JSON
    araiguma watch                 Stream observations as NDJSON events
    araiguma topology [--input FILE] [--pretty]
                                   Project a snapshot document (file, or
                                   stdin when omitted) into a topology
                                   document (nodes, edges, evidence)
    araiguma --help                Show this help
    araiguma --version             Show the version

The snapshot JSON contains the merged observation (interfaces, addresses,
routes in all tables) and a provider report describing what each provider
observed, what failed, and what is unsupported.

watch prints:
    {\"event\":\"snapshot\",...}       initial state
    {\"event\":\"changed\",...}        debounced change diff
    {\"event\":\"resynchronized\",...} full refresh after a monitoring
                                    reconnect (changes while offline were
                                    not observed; the snapshot is
                                    authoritative again)

Topology documents are a pure projection of a snapshot: nodes and edges
exist only where the observation model provides evidence.

Diagnostics go to stderr; set ARAIGUMA_LOG=debug for provider tracing."
        .to_string()
}

pub fn parse_args(args: &[String]) -> Result<Command, String> {
    match args {
        [] => Ok(Command::Snapshot { pretty: false }),
        [single] => match single.as_str() {
            "snapshot" => Ok(Command::Snapshot { pretty: false }),
            "watch" => Ok(Command::Watch),
            "topology" => Ok(Command::Topology {
                input: None,
                pretty: false,
            }),
            "--help" | "-h" | "help" => Ok(Command::Help),
            "--version" | "-V" => Ok(Command::Version),
            other => Err(format!("unknown command: {other}")),
        },
        [cmd, flag] if cmd == "snapshot" && flag == "--pretty" => {
            Ok(Command::Snapshot { pretty: true })
        }
        [cmd, flag] if cmd == "topology" && flag == "--pretty" => Ok(Command::Topology {
            input: None,
            pretty: true,
        }),
        [cmd, flag, value] if cmd == "topology" && flag == "--input" => Ok(Command::Topology {
            input: Some(value.clone()),
            pretty: false,
        }),
        [cmd, flag, value, pretty]
            if cmd == "topology" && flag == "--input" && pretty == "--pretty" =>
        {
            Ok(Command::Topology {
                input: Some(value.clone()),
                pretty: true,
            })
        }
        _ => Err(format!("unrecognized arguments: {args:?}")),
    }
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// Observe once through both providers and merge.
pub async fn observe_merged() -> NetworkSnapshot {
    let mut netlink = NetlinkProvider::new();
    let mut sysfs = SysFsProvider::new();
    // Observe netlink first: it is the authoritative source, and sysfs
    // only fills gaps during the merge.
    let observations = vec![netlink.observe().await, sysfs.observe().await];
    merge(observations, now_ms())
}

/// Serialize a snapshot for `araiguma snapshot`.
pub fn render_snapshot(snapshot: &NetworkSnapshot, pretty: bool) -> String {
    if pretty {
        serde_json::to_string_pretty(snapshot).expect("snapshot serializes")
    } else {
        serde_json::to_string(snapshot).expect("snapshot serializes")
    }
}

/// Serialize a topology document.
pub fn render_topology(document: &TopologyDocument, pretty: bool) -> String {
    if pretty {
        serde_json::to_string_pretty(document).expect("topology serializes")
    } else {
        serde_json::to_string(document).expect("topology serializes")
    }
}

/// Load a snapshot from a file (or stdin when `input` is None) and project
/// it. Read-only; never observes the live system.
pub fn project_snapshot_to_topology(input: Option<&str>) -> Result<TopologyDocument, String> {
    let raw = match input {
        Some(path) => {
            std::fs::read_to_string(path).map_err(|error| format!("cannot read {path}: {error}"))?
        }
        None => {
            let mut buffer = String::new();
            std::io::stdin()
                .read_to_string(&mut buffer)
                .map_err(|error| format!("cannot read stdin: {error}"))?;
            buffer
        }
    };
    let snapshot: NetworkSnapshot = serde_json::from_str(&raw)
        .map_err(|error| format!("invalid snapshot document: {error}"))?;
    Ok(build_topology(&snapshot))
}

/// The watch event envelopes (newline-delimited JSON, one per line).
pub fn snapshot_envelope(snapshot: &NetworkSnapshot) -> String {
    serde_json::json!({
        "event": "snapshot",
        "snapshot": snapshot,
    })
    .to_string()
}

pub fn changed_envelope(kinds: &[&str], changes: &[NetworkEvent]) -> String {
    serde_json::json!({
        "event": "changed",
        "notifiedBy": kinds,
        "changes": changes,
    })
    .to_string()
}

pub fn resynchronized_envelope(changes: &[NetworkEvent], snapshot: &NetworkSnapshot) -> String {
    serde_json::json!({
        "event": "resynchronized",
        "changes": changes,
        "snapshot": snapshot,
    })
    .to_string()
}

/// Run the watch loop, writing NDJSON events to `out`. Stops when `stop`
/// resolves (SIGINT/SIGTERM in the binary; anything else in tests).
pub async fn run_watch<F: std::future::Future<Output = ()>>(
    out: &mut dyn std::io::Write,
    stop: &mut std::pin::Pin<&mut F>,
) -> Result<(), String> {
    let mut monitor = Monitor::start().map_err(|error| {
        format!("cannot start netlink monitoring: {error}. Observation-only mode is available via `araiguma snapshot`.")
    })?;

    let mut last = observe_merged().await;
    writeln!(out, "{}", snapshot_envelope(&last)).map_err(io_error)?;

    loop {
        let signal = tokio::select! {
            _ = &mut *stop => return Ok(()),
            signal = monitor.next_signal() => signal,
        };
        match signal {
            Signal::Resynchronized => {
                let fresh = observe_merged().await;
                let changes = diff_snapshots(&last, &fresh);
                writeln!(out, "{}", resynchronized_envelope(&changes, &fresh)).map_err(io_error)?;
                last = fresh;
            }
            Signal::Changed { kind } => {
                // Debounce: absorb the rest of a notification burst.
                let mut kinds = vec![kind];
                let mut resync = false;
                let deadline = tokio::time::Instant::now() + DEBOUNCE;
                while !resync {
                    let next = tokio::select! {
                        _ = &mut *stop => return Ok(()),
                        next = tokio::time::timeout_at(deadline, monitor.next_signal()) => next,
                    };
                    match next {
                        Ok(Signal::Changed { kind }) => kinds.push(kind),
                        Ok(Signal::Resynchronized) => resync = true,
                        Err(_elapsed) => break, // quiet window over
                    }
                }
                let fresh = observe_merged().await;
                let changes = diff_snapshots(&last, &fresh);
                if resync {
                    writeln!(out, "{}", resynchronized_envelope(&changes, &fresh))
                        .map_err(io_error)?;
                } else {
                    writeln!(out, "{}", changed_envelope(&kinds, &changes)).map_err(io_error)?;
                }
                last = fresh;
            }
        }
    }
}

fn io_error(error: std::io::Error) -> String {
    format!("write failed: {error}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn argument_parsing() {
        let a = |args: &[&str]| parse_args(&args.iter().map(|s| s.to_string()).collect::<Vec<_>>());
        assert_eq!(a(&[]), Ok(Command::Snapshot { pretty: false }));
        assert_eq!(a(&["snapshot"]), Ok(Command::Snapshot { pretty: false }));
        assert_eq!(
            a(&["snapshot", "--pretty"]),
            Ok(Command::Snapshot { pretty: true })
        );
        assert_eq!(a(&["watch"]), Ok(Command::Watch));
        assert_eq!(a(&["--help"]), Ok(Command::Help));
        assert_eq!(a(&["--version"]), Ok(Command::Version));
        assert!(a(&["frobnicate"]).is_err());
        assert!(a(&["watch", "--pretty"]).is_err());
    }

    #[test]
    fn snapshot_envelope_shape() {
        let snapshot = NetworkSnapshot {
            schema_version: 1,
            timestamp_ms: 42,
            interfaces: vec![],
            addresses: vec![],
            routes: vec![],
            providers: vec![],
        };
        let value: serde_json::Value = serde_json::from_str(&snapshot_envelope(&snapshot)).unwrap();
        assert_eq!(value["event"], "snapshot");
        assert_eq!(value["snapshot"]["schemaVersion"], 1);
        assert_eq!(value["snapshot"]["timestampMs"], 42);
    }

    #[test]
    fn changed_envelope_shape() {
        let value: serde_json::Value = serde_json::from_str(&changed_envelope(
            &["newLink", "newRoute"],
            &[NetworkEvent::Resynchronized],
        ))
        .unwrap();
        assert_eq!(value["event"], "changed");
        assert_eq!(value["notifiedBy"][0], "newLink");
        assert_eq!(value["changes"][0], "resynchronized");
    }

    #[test]
    fn resynchronized_envelope_includes_full_snapshot() {
        let snapshot = NetworkSnapshot {
            schema_version: 1,
            timestamp_ms: 43,
            interfaces: vec![],
            addresses: vec![],
            routes: vec![],
            providers: vec![],
        };
        let value: serde_json::Value =
            serde_json::from_str(&resynchronized_envelope(&[], &snapshot)).unwrap();
        assert_eq!(value["event"], "resynchronized");
        assert!(value["snapshot"]["interfaces"].is_array());
    }
}
