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

use araiguma_core::{build_topology, NetworkSnapshot, TopologyDocument};
use araiguma_engine::{ChangeFeed, Engine};
use araiguma_netlink::Signal;

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

/// Observe once through the shared engine and merge.
pub async fn observe_merged() -> NetworkSnapshot {
    Engine::default_providers().observe_now().await
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
///
/// Schema policy: current documents are v3 (three-state `macAddress`,
/// optional veth `peerIfindex` — WO-03 M2, optional route `nextHops` —
/// WO-03 M3; both defaulted, so pre-v3 documents parse unchanged).
/// v2 documents are accepted as-is. v1 documents are accepted **only
/// when unambiguous** — every `macAddress` must be a non-null string; a
/// v1 `null` cannot distinguish "observed no hardware address" from
/// "not observed" and is rejected with an explanation instead of being
/// silently assigned a meaning. Unknown versions are rejected.
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
    let value: serde_json::Value =
        serde_json::from_str(&raw).map_err(|error| format!("invalid JSON: {error}"))?;
    // The ONE document boundary (OD-03, document 04 §5): the same
    // validation the desktop IPC path applies to a caller-supplied
    // document. Version policy and the provenance contract are enforced
    // there, so the CLI and the IPC accept and reject the same documents
    // for the same reasons.
    let snapshot = araiguma_core::parse_snapshot_document(value)?;
    Ok(build_topology(&snapshot))
}

/// Run the watch loop, writing NDJSON events to `out`. Stops when `stop`
/// resolves (SIGINT/SIGTERM in the binary; anything else in tests).
///
/// The output is the existing CLI watch contract, unchanged: one envelope
/// per line — `snapshot` (the initial state), `changed` (a debounced pure
/// diff), `resynchronized` (a monitoring gap: changes during the gap were
/// NOT observed, and the fresh snapshot is authoritative). The engine is
/// shared with the desktop shell; only the transport differs.
pub async fn run_watch<F: std::future::Future<Output = ()>>(
    out: &mut dyn std::io::Write,
    stop: &mut std::pin::Pin<&mut F>,
) -> Result<(), String> {
    let feed = araiguma_engine::default_change_feed().map_err(|error| {
        format!(
            "cannot start netlink monitoring: {error}. \
             Observation-only mode is available via `araiguma snapshot`."
        )
    })?;
    run_watch_with_feed(out, stop, feed, Engine::default_providers()).await
}

async fn run_watch_with_feed<F, S>(
    out: &mut dyn std::io::Write,
    stop: &mut std::pin::Pin<&mut S>,
    mut feed: F,
    mut engine: Engine,
) -> Result<(), String>
where
    F: ChangeFeed,
    S: std::future::Future<Output = ()>,
{
    // The opening envelope: the current state, before any change arrives.
    let opening = engine.snapshot_envelope().await;
    writeln!(out, "{}", opening.to_cli_json_string()).map_err(io_error)?;

    loop {
        let signal = tokio::select! {
            _ = &mut *stop => return Ok(()),
            signal = feed.next_signal() => signal,
        };
        let Some(signal) = signal else {
            return Ok(());
        };
        let envelope = match signal {
            // Monitor-health transitions are desktop IPC control signals.
            // The CLI watch contract remains exactly three envelopes: it
            // waits through a disconnect/reconnect and emits the same
            // `resynchronized` document event as before.
            Signal::Disconnected | Signal::ReconnectFailed { .. } => continue,
            Signal::Resynchronized | Signal::Reconnected => engine.apply_resync().await,
            Signal::Changed { kind } => {
                // Debounce: absorb the rest of a notification burst.
                let Some(burst) =
                    araiguma_engine::debounce(&mut feed, kind.to_string(), stop).await
                else {
                    return Ok(());
                };
                if burst.monitor_degraded.is_some() {
                    // The monitor-health fact is desktop-only. The CLI
                    // waits through the disconnect/reconnect and emits the
                    // same resynchronized document event as before; the
                    // interrupted change burst is not a trusted pure diff.
                    continue;
                } else if burst.resync {
                    engine.apply_resync().await
                } else {
                    engine.apply_change(burst.notified_by).await
                }
            }
        };
        writeln!(out, "{}", envelope.to_cli_json_string()).map_err(io_error)?;
    }
}

fn io_error(error: std::io::Error) -> String {
    format!("write failed: {error}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn monitor_health_transitions_do_not_change_the_cli_event_contract() {
        use araiguma_engine::FakeFeed;

        let mut output = Vec::new();
        let mut stop = std::pin::pin!(std::future::pending::<()>());
        let feed = FakeFeed::new(vec![
            // The health transition occurs inside the change debounce
            // window; the CLI must still expose only its three-event
            // contract and no partial pure-diff event.
            Signal::Changed { kind: "newLink" },
            Signal::Disconnected,
            Signal::ReconnectFailed {
                reason: "synthetic reconnect failure".to_string(),
            },
            Signal::Reconnected,
        ]);
        run_watch_with_feed(&mut output, &mut stop, feed, Engine::new(Vec::new()))
            .await
            .expect("watch completes when the feed ends");

        let lines = String::from_utf8(output).expect("UTF-8 NDJSON");
        let values: Vec<serde_json::Value> = lines
            .lines()
            .map(|line| serde_json::from_str(line).expect("valid NDJSON"))
            .collect();
        assert_eq!(values.len(), 2);
        assert_eq!(values[0]["event"], "snapshot");
        assert_eq!(values[1]["event"], "resynchronized");
        assert!(values.iter().all(|value| value.get("seq").is_none()));
        assert!(values.iter().all(|value| value["event"] != "monitorHealth"));
    }

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

    /// Write `document` to a unique temp file and project it — the same
    /// document-boundary path `araiguma topology --input` uses. The name
    /// carries a process-wide counter: tests run in parallel within one
    /// process, and a shared name would let one test truncate another's
    /// file between write and read.
    fn project_tmp(document: &str) -> Result<TopologyDocument, String> {
        static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let serial = NEXT.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let path = std::env::temp_dir().join(format!(
            "araiguma-cli-test-{}-{serial}.json",
            std::process::id()
        ));
        std::fs::write(&path, document).expect("writable temp file");
        let result = project_snapshot_to_topology(Some(path.to_str().unwrap()));
        let _ = std::fs::remove_file(&path);
        result
    }

    /// The minimal route shape shared by the version-policy tests: one
    /// interface (so the route's oif resolves), one route, no v3-only
    /// fields unless the test adds them.
    fn legacy_document(version: u64) -> String {
        format!(
            r#"{{
        "schemaVersion": {version},
        "timestampMs": 1,
        "interfaces": [{{
            "ifindex": 2, "name": "eth0", "kind": "ethernet",
            "macAddress": "52:54:00:aa:bb:cc",
            "operState": "up", "adminState": "up",
            "sources": ["netlink"], "notes": []
        }}],
        "addresses": [],
        "routes": [{{
            "family": "ipv4",
            "destination": {{"address": "10.0.0.0", "prefixLen": 24}},
            "gateway": null, "outputInterface": 2, "outputInterfaceName": null,
            "protocol": "kernel", "scope": "link", "routeType": "unicast",
            "metric": 0, "table": 254, "flags": null,
            "sources": ["netlink"], "notes": []
        }}],
        "providers": []
        }}"#
        )
    }

    /// Regression (WO-03 M3): a v2 document — written before `nextHops`
    /// and `peerIfindex` existed — must project unchanged. This is the
    /// exact document an Emne-side export produced at the Part 1 gate.
    #[test]
    fn v2_document_without_v3_fields_projects() {
        let topology = project_tmp(&legacy_document(2))
            .expect("v2 document (no nextHops/peerIfindex) parses under the v3 model");
        assert_eq!(topology.schema_version, 3);
        assert_eq!(topology.nodes.len(), 1, "the interface projects to a node");
    }

    /// Regression (WO-03 M3): an M2-era v3 document carries `peerIfindex`
    /// but no `nextHops`; the field default must accept it.
    #[test]
    fn m2_era_v3_document_without_next_hops_projects() {
        let topology =
            project_tmp(&legacy_document(3)).expect("M2-era v3 document (no nextHops) parses");
        assert_eq!(topology.schema_version, 3);
    }

    /// A current v3 multipath document round-trips through the projector
    /// (sanity: the new `nextHops` field itself is no obstacle).
    #[test]
    fn v3_multipath_document_projects() {
        let document = legacy_document(3).replace(
            r#""sources": ["netlink"], "notes": []
        }}],
        "providers""#,
            r#""nextHops": {{"multi": [
                {{"gateway": "10.0.0.1", "outputInterface": 2, "outputInterfaceName": null, "weight": 2, "flags": []}},
                {{"gateway": "10.0.0.2", "outputInterface": 2, "outputInterfaceName": null, "weight": 1, "flags": []}}
            ]}},
            "sources": ["netlink"], "notes": []
        }}],
        "providers""#,
        );
        let topology = project_tmp(&document).expect("v3 multipath document parses");
        assert_eq!(topology.schema_version, 3);
    }

    /// v1 stays rejected exactly where it is ambiguous (null MAC), and
    /// unknown versions stay rejected — the version policy itself is
    /// regression-locked alongside the acceptance tests.
    #[test]
    fn v1_ambiguous_and_unknown_versions_are_rejected() {
        let ambiguous = legacy_document(1).replace(
            r#""macAddress": "52:54:00:aa:bb:cc""#,
            r#""macAddress": null"#,
        );
        let error = project_tmp(&ambiguous).expect_err("v1 null MAC is ambiguous");
        assert!(
            error.contains("ambiguous in schema v1"),
            "rejection explains the ambiguity: {error}"
        );

        let unambiguous = legacy_document(1);
        project_tmp(&unambiguous).expect("unambiguous v1 is accepted");

        let error = project_tmp(&legacy_document(99)).expect_err("unknown version rejected");
        assert!(
            error.contains("unsupported snapshot schemaVersion 99"),
            "rejection names the version: {error}"
        );
    }

    #[test]
    fn observations_without_provenance_are_rejected_at_the_boundary() {
        // WO-03-F / AUD-06: an observation with an EMPTY source list is not a
        // valid observation — nothing actually supplied it, and no attribution
        // may be invented downstream. The projector rejects the document with
        // a path-qualified reason instead of projecting fabricated provenance.
        let document = legacy_document(3).replace(
            r#""sources": ["netlink"], "notes": []
        }],
        "providers""#,
            r#""sources": [], "notes": []
        }],
        "providers""#,
        );
        let error = project_tmp(&document).expect_err("empty provenance rejected");
        assert!(
            error.contains("$.routes[0].sources is empty"),
            "the rejection names the offending location: {error}"
        );
        assert!(
            error.contains("must name at least one provider"),
            "the rejection explains the contract: {error}"
        );
    }
}
