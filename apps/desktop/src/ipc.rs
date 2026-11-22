//! Desktop IPC surface (document 04 §9, §10).
//!
//! | Command | Request | Response |
//! |---|---|---|
//! | `get_snapshot` | — | `NetworkSnapshot` |
//! | `get_topology` | `{ "source": "current" }` or `{ "snapshot": SnapshotDocument }` | `TopologyDocument` |
//! | `subscribe_watch` | — | `{ "subscriptionId": string }` |
//! | `unsubscribe_watch` | `{ "subscriptionId": string }` | acknowledgement |
//!
//! `get_topology` with a caller-supplied snapshot validates the document at
//! the shared Rust boundary first (document 04 §5) and rejects invalid input
//! with a path-qualified reason. `unsubscribe_watch` with an unknown id
//! returns `invalidRequest` naming the id (document 04 lifecycle).
//!
//! Stage 1 deviation: command errors are plain strings, not the typed
//! `{ code, message, detail? }` object in document 04 §9. The bundled
//! frontend consumes only the message; typed errors remain deferred.

use araiguma_core::{build_topology, parse_snapshot_document, NetworkSnapshot, TopologyDocument};
use tauri::{Manager, State, WebviewWindow};

use crate::driver;
use crate::state::{AppState, MonitoringStatus};

#[tauri::command]
pub async fn get_snapshot(state: State<'_, AppState>) -> Result<NetworkSnapshot, String> {
    state.ensure_running()?;
    state.engine.lock().await.snapshot_document().await
}

#[tauri::command]
pub async fn get_topology(
    state: State<'_, AppState>,
    source: Option<String>,
    snapshot: Option<serde_json::Value>,
) -> Result<TopologyDocument, String> {
    state.ensure_running()?;
    validate_source(source.as_deref())?;
    let document = match snapshot {
        Some(value) => {
            parse_snapshot_document(value).map_err(|error| format!("invalidRequest: {error}"))?
        }
        None => state.engine.lock().await.snapshot_document().await?,
    };
    Ok(build_topology(&document))
}

fn validate_source(source: Option<&str>) -> Result<(), String> {
    match source {
        None | Some("current") => Ok(()),
        Some(other) => Err(format!(
            "invalidRequest: source must be \"current\" when no snapshot is supplied, got {other:?}"
        )),
    }
}

fn monitoring_readiness_error(status: MonitoringStatus) -> Option<String> {
    match status {
        MonitoringStatus::Running => None,
        MonitoringStatus::Starting => {
            Some("internalError: live monitoring is still starting".to_string())
        }
        MonitoringStatus::Degraded(reason) => {
            Some(format!("internalError: live monitoring degraded: {reason}"))
        }
        MonitoringStatus::Unavailable(reason) => Some(format!(
            "internalError: live monitoring unavailable: {reason}"
        )),
    }
}

#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SubscribeWatchResponse {
    pub subscription_id: String,
}

#[tauri::command]
pub async fn subscribe_watch(window: WebviewWindow) -> Result<SubscribeWatchResponse, String> {
    let state = window.app_handle().state::<AppState>().inner().clone();
    state.ensure_running()?;
    // Do not present a one-shot snapshot as a live subscription when the
    // network monitor failed to start. The frontend receives this rejection
    // and can show its truthful unavailable/stale state.
    state.wait_for_monitoring().await?;
    // Monitoring readiness may have taken long enough for shutdown to
    // begin. Recheck before registering a subscription, and check again
    // after registration so a shutdown race cannot return a live watch.
    state.ensure_running()?;
    let (id, rx) = state.engine.lock().await.subscribe_and_snapshot().await?;
    // Start forwarding immediately. If monitoring degrades between the
    // first readiness check and registration, the health fact must reach
    // the webview rather than waiting behind the command response.
    driver::pump(window, state.engine.clone(), id.clone(), rx);
    // Recheck monitoring immediately after registration. If the event stream
    // degraded between the first readiness check and registration, do not
    // hold the command (and therefore the subscription id) pending until an
    // eventual recovery: the frontend must be able to cancel a watch during
    // an outage. The subscription is removed now and the command reports the
    // truthful degraded/unavailable state.
    let readiness_error = monitoring_readiness_error(state.monitoring_status());
    if let Some(error) = readiness_error {
        let mut engine = state.engine.lock().await;
        let _ = engine.unsubscribe(&id);
        return Err(error);
    }
    if let Err(error) = state.ensure_running() {
        let mut engine = state.engine.lock().await;
        let _ = engine.unsubscribe(&id);
        return Err(error);
    }
    Ok(SubscribeWatchResponse {
        subscription_id: id,
    })
}

#[tauri::command]
pub async fn unsubscribe_watch(
    state: State<'_, AppState>,
    subscription_id: String,
) -> Result<(), String> {
    state.ensure_running()?;
    state.engine.lock().await.unsubscribe(&subscription_id)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn only_current_is_a_valid_source_without_a_snapshot() {
        assert!(validate_source(None).is_ok());
        assert!(validate_source(Some("current")).is_ok());
        let error = validate_source(Some("file")).expect_err("unknown source rejected");
        assert!(error.starts_with("invalidRequest:"));
    }

    #[test]
    fn a_degraded_monitor_is_not_admitted_after_registration() {
        assert!(monitoring_readiness_error(MonitoringStatus::Running).is_none());
        let error = monitoring_readiness_error(MonitoringStatus::Degraded(
            "the event stream disconnected".to_string(),
        ))
        .expect("degraded monitoring rejects the registration");
        assert!(error.starts_with("internalError: live monitoring degraded:"));
    }

    #[test]
    fn subscribe_response_uses_the_contract_camel_case_field() {
        let response = SubscribeWatchResponse {
            subscription_id: "subscription-1".to_string(),
        };
        let value = serde_json::to_value(response).expect("serialize response");
        assert_eq!(value["subscriptionId"], "subscription-1");
        assert!(value.get("subscription_id").is_none());
    }

    #[test]
    fn caller_supplied_snapshots_use_the_shared_document_boundary() {
        let invalid = json!({
            "schemaVersion": 3,
            "timestampMs": 1,
            "interfaces": [{
                "ifindex": 2,
                "name": "eth0",
                "kind": "ethernet",
                "macAddress": "52:54:00:aa:bb:cc",
                "sources": [],
                "notes": []
            }],
            "addresses": [],
            "routes": [],
            "providers": []
        });
        let error = parse_snapshot_document(invalid).expect_err("empty provenance rejected");
        assert!(error.contains("$.interfaces[0].sources is empty"));

        let valid = json!({
            "schemaVersion": 3,
            "timestampMs": 1,
            "interfaces": [],
            "addresses": [],
            "routes": [],
            "providers": []
        });
        let snapshot = parse_snapshot_document(valid).expect("valid snapshot");
        let topology = build_topology(&snapshot);
        assert!(topology.nodes.is_empty());
    }
}
