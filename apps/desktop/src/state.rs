//! Desktop application state and lifecycle (document 01 §6).
//!
//! [`AppState`] owns the shared [`araiguma_engine::Engine`] behind a Tokio
//! mutex, the resolved XDG paths, monitoring status, the cooperative
//! shutdown signal, and the live driver's join handle. The engine is the
//! single owner of observation state; the shell only translates IPC to
//! engine calls and forwards envelopes verbatim (document 01 §7.3 — "the
//! shell is thin").

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex as StdMutex};
use std::time::Duration;

use araiguma_engine::Engine;
use tauri::async_runtime::JoinHandle;
use tokio::sync::{watch, Mutex};

use crate::paths::XdgPaths;

/// The state of the process-wide network change monitor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MonitoringStatus {
    /// The driver has not finished starting the monitor yet.
    Starting,
    /// The monitor is active and can deliver change signals.
    Running,
    /// The event stream is temporarily unreliable while the monitor
    /// reconnects. Observation commands remain available, but a new live
    /// subscription must wait until the stream is healthy again.
    Degraded(String),
    /// Monitoring could not start or ended unexpectedly.
    Unavailable(String),
}

/// The process-wide state managed by Tauri.
#[derive(Clone)]
pub struct AppState {
    /// The one observation engine shared by every IPC consumer.
    pub engine: Arc<Mutex<Engine>>,
    /// The XDG configuration and state locations resolved at startup.
    pub paths: XdgPaths,
    /// Cooperative shutdown signal. `true` means the application no longer
    /// accepts commands and the live driver should stop.
    shutdown: watch::Sender<bool>,
    shutting_down: Arc<AtomicBool>,
    /// Set only after the driver has been joined (or forcibly aborted after
    /// the bounded shutdown timeout). It permits the final Tauri exit.
    exit_allowed: Arc<AtomicBool>,
    /// The live monitor's startup/runtime state.
    monitoring: watch::Sender<MonitoringStatus>,
    /// Serializes the shutdown transition with monitoring-status updates.
    /// Without this lock, a startup completion could publish `Running`
    /// after shutdown had already made the application unavailable.
    lifecycle_guard: Arc<StdMutex<()>>,
    /// The named owner of the live-driver task. Keeping the handle makes
    /// shutdown completion observable and prevents an orphaned driver.
    driver_task: Arc<StdMutex<Option<JoinHandle<()>>>>,
}

impl AppState {
    pub fn new(paths: XdgPaths) -> Self {
        Self::with_engine(paths, Engine::default_providers())
    }

    /// Construct state with an explicit engine (tests and embedders).
    pub(crate) fn with_engine(paths: XdgPaths, engine: Engine) -> Self {
        let (shutdown, _receiver) = watch::channel(false);
        let (monitoring, _receiver) = watch::channel(MonitoringStatus::Starting);
        AppState {
            engine: Arc::new(Mutex::new(engine)),
            paths,
            shutdown,
            shutting_down: Arc::new(AtomicBool::new(false)),
            exit_allowed: Arc::new(AtomicBool::new(false)),
            monitoring,
            lifecycle_guard: Arc::new(StdMutex::new(())),
            driver_task: Arc::new(StdMutex::new(None)),
        }
    }

    /// A receiver for the cooperative shutdown signal.
    pub fn shutdown_receiver(&self) -> watch::Receiver<bool> {
        self.shutdown.subscribe()
    }

    /// Begin an ordered shutdown. This method never waits on the engine:
    /// it is called from Tauri's event-loop callback, where blocking is not
    /// allowed. The live driver observes the same signal and stops at its
    /// next boundary.
    pub fn request_shutdown(&self) {
        let _ = self.begin_shutdown();
    }

    /// Begin shutdown once and report whether this call initiated it.
    pub fn begin_shutdown(&self) -> bool {
        let _lifecycle = self
            .lifecycle_guard
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if !self.shutting_down.swap(true, Ordering::SeqCst) {
            self.shutdown.send_replace(true);
            self.monitoring.send_replace(MonitoringStatus::Unavailable(
                "the application is shutting down".to_string(),
            ));
            true
        } else {
            false
        }
    }

    /// Whether shutdown has begun.
    pub fn shutdown_requested(&self) -> bool {
        self.shutting_down.load(Ordering::SeqCst)
    }

    /// Allow the final Tauri exit after shutdown cleanup has completed.
    pub fn allow_exit(&self) {
        self.exit_allowed.store(true, Ordering::SeqCst);
    }

    /// Whether shutdown cleanup has completed far enough to exit.
    pub fn exit_allowed(&self) -> bool {
        self.exit_allowed.load(Ordering::SeqCst)
    }

    /// Record the live monitor's current state.
    pub fn set_monitoring_status(&self, status: MonitoringStatus) {
        let _lifecycle = self
            .lifecycle_guard
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if self.shutdown_requested()
            && matches!(
                status,
                MonitoringStatus::Running | MonitoringStatus::Degraded(_)
            )
        {
            // A driver startup/reconnect that raced application shutdown
            // must not make the session appear monitorable after shutdown
            // began.
            return;
        }
        self.monitoring.send_replace(status);
    }

    /// The live monitor's current readiness state.
    pub fn monitoring_status(&self) -> MonitoringStatus {
        self.monitoring.borrow().clone()
    }

    /// Wait until monitoring is ready to accept a live subscription.
    ///
    /// A startup failure is returned to the caller instead of being hidden
    /// behind a successful one-shot snapshot subscription. Shutdown wakes
    /// this wait immediately so command admission cannot remain stuck on a
    /// monitor that will never become ready.
    pub async fn wait_for_monitoring(&self) -> Result<(), String> {
        let mut monitoring = self.monitoring.subscribe();
        let mut shutdown = self.shutdown_receiver();
        loop {
            if self.shutdown_requested() || *shutdown.borrow_and_update() {
                return Err(
                    "shutdownInProgress: the Araiguma desktop session is shutting down".to_string(),
                );
            }

            let status = monitoring.borrow_and_update().clone();
            match status {
                MonitoringStatus::Starting | MonitoringStatus::Degraded(_) => {
                    tokio::select! {
                        biased;
                        _ = shutdown.changed() => continue,
                        changed = monitoring.changed() => {
                            if changed.is_err() {
                                return Err(
                                    "internalError: live monitoring status channel closed".to_string()
                                );
                            }
                        }
                    }
                }
                MonitoringStatus::Running => return Ok(()),
                MonitoringStatus::Unavailable(reason) => {
                    return Err(format!(
                        "internalError: live monitoring unavailable: {reason}"
                    ));
                }
            }
        }
    }

    /// Give the live-driver task a named owner.
    pub fn set_driver_task(&self, task: JoinHandle<()>) {
        let mut slot = self
            .driver_task
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if slot.is_some() {
            task.abort();
        } else {
            *slot = Some(task);
        }
    }

    /// Signal the driver, then join it within a bounded timeout.
    pub async fn shutdown_driver(&self, timeout: Duration) -> Result<(), String> {
        self.request_shutdown();
        let task = {
            let mut slot = self
                .driver_task
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            slot.take()
        };
        let Some(mut task) = task else {
            return Ok(());
        };

        match tokio::time::timeout(timeout, &mut task).await {
            Ok(Ok(())) => Ok(()),
            Ok(Err(error)) => Err(format!("live driver failed during shutdown: {error}")),
            Err(_) => {
                task.abort();
                Err(format!(
                    "live driver did not stop within the {timeout:?} shutdown timeout"
                ))
            }
        }
    }

    /// Reject commands after shutdown has started (document 04 §9).
    pub fn ensure_running(&self) -> Result<(), String> {
        if self.shutdown_requested() {
            Err("shutdownInProgress: the Araiguma desktop session is shutting down".to_string())
        } else {
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::paths::XdgPaths;
    use std::path::PathBuf;

    fn state() -> AppState {
        AppState::new(XdgPaths {
            config_dir: PathBuf::from("/tmp/config/araiguma"),
            state_dir: PathBuf::from("/tmp/state/araiguma"),
        })
    }

    #[test]
    fn shutdown_is_visible_to_new_commands() {
        let state = state();
        assert!(state.ensure_running().is_ok());
        assert!(state.begin_shutdown());
        assert!(!state.begin_shutdown());
        assert!(state.shutdown_requested());
        let error = state
            .ensure_running()
            .expect_err("shutdown rejects commands");
        assert!(error.starts_with("shutdownInProgress:"));
    }

    #[tokio::test]
    async fn monitoring_startup_failure_is_visible_to_subscribers() {
        let state = state();
        state.set_monitoring_status(MonitoringStatus::Unavailable(
            "netlink monitor could not start".to_string(),
        ));
        let error = state
            .wait_for_monitoring()
            .await
            .expect_err("unavailable monitoring rejects a subscription");
        assert!(error.starts_with("internalError: live monitoring unavailable:"));
    }

    #[tokio::test]
    async fn running_monitoring_accepts_a_subscription() {
        let state = state();
        state.set_monitoring_status(MonitoringStatus::Running);
        assert!(state.wait_for_monitoring().await.is_ok());
    }

    #[tokio::test]
    async fn degraded_monitoring_does_not_admit_a_new_live_subscription() {
        let state = state();
        state.set_monitoring_status(MonitoringStatus::Degraded(
            "netlink event stream disconnected".to_string(),
        ));
        let waiting = state.clone();
        let task = tokio::spawn(async move { waiting.wait_for_monitoring().await });

        tokio::time::sleep(Duration::from_millis(10)).await;
        assert!(
            !task.is_finished(),
            "degraded monitoring must not admit a watch"
        );

        state.set_monitoring_status(MonitoringStatus::Running);
        tokio::time::timeout(Duration::from_secs(1), task)
            .await
            .expect("monitoring wait stopped")
            .expect("monitoring wait task completed")
            .expect("recovered monitoring admits a watch");
    }

    #[tokio::test]
    async fn shutdown_wakes_a_waiting_monitoring_readiness_check() {
        let state = state();
        let waiting = state.clone();
        let task = tokio::spawn(async move { waiting.wait_for_monitoring().await });

        tokio::time::sleep(Duration::from_millis(10)).await;
        state.begin_shutdown();

        let error = tokio::time::timeout(Duration::from_secs(1), task)
            .await
            .expect("monitoring wait stopped")
            .expect("monitoring wait task completed")
            .expect_err("shutdown rejects the waiting subscription");
        assert!(error.starts_with("shutdownInProgress:"));
    }

    #[test]
    fn monitoring_cannot_report_running_after_shutdown_begins() {
        let state = state();
        state.set_monitoring_status(MonitoringStatus::Running);
        state.begin_shutdown();
        state.set_monitoring_status(MonitoringStatus::Running);

        assert!(matches!(
            state.monitoring.borrow().clone(),
            MonitoringStatus::Unavailable(_)
        ));
    }
}
