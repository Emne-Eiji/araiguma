//! Araiguma desktop shell entry point (document 01 §7.3 — the desktop shell
//! crate is the only crate that depends on Tauri, OD-01).
//!
//! Startup order: resolve and prepare the XDG locations; manage
//! [`AppState`]; start and retain the live driver inside the Tauri runtime;
//! bind the four IPC commands; run until the window closes.
//!
//! Read-only: the shell only exposes IPC and window management. It never
//! mutates network state (document 06 — the invariant is structural and
//! inherited by the observation engine).

mod driver;
mod ipc;
mod paths;
mod state;

use std::time::Duration;

use tauri::{AppHandle, Manager};

use paths::XdgPaths;
use state::AppState;

/// The maximum time the final exit waits for the live driver to stop.
const DRIVER_SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(5);

/// Start the one ordered shutdown path. The caller is responsible for
/// preventing the event that requested the close until this task has
/// joined the live driver.
fn start_shutdown(app_handle: &AppHandle, requested_exit_code: i32) {
    let state = app_handle.state::<AppState>().inner().clone();
    if !state.begin_shutdown() {
        return;
    }

    eprintln!("araiguma: shutdown requested");
    let app_handle = app_handle.clone();
    tauri::async_runtime::spawn(async move {
        let exit_code = match state.shutdown_driver(DRIVER_SHUTDOWN_TIMEOUT).await {
            Ok(()) => requested_exit_code,
            Err(error) => {
                eprintln!("araiguma: {error}");
                1
            }
        };
        eprintln!("araiguma: live driver stopped");
        state.allow_exit();
        app_handle.exit(exit_code);
    });
}

/// Install cooperative SIGTERM/SIGINT handling for the desktop process.
///
/// Tauri's default Linux behavior terminates immediately on SIGTERM, which
/// would bypass the ordered driver shutdown and report signal status instead
/// of a clean exit. The signal task enters the same shutdown path as a window
/// close; `RunEvent::ExitRequested` then lets the final exit through after
/// the driver has been joined.
#[cfg(unix)]
fn install_shutdown_signals(app_handle: &AppHandle) {
    use tokio::signal::unix::{signal, SignalKind};

    let handle = app_handle.clone();
    tauri::async_runtime::spawn(async move {
        let mut terminate = match signal(SignalKind::terminate()) {
            Ok(signal) => signal,
            Err(error) => {
                eprintln!("araiguma: cannot install SIGTERM handler: {error}");
                return;
            }
        };
        let mut interrupt = match signal(SignalKind::interrupt()) {
            Ok(signal) => signal,
            Err(error) => {
                eprintln!("araiguma: cannot install SIGINT handler: {error}");
                return;
            }
        };

        let name = tokio::select! {
            _ = terminate.recv() => "SIGTERM",
            _ = interrupt.recv() => "SIGINT",
        };
        eprintln!("araiguma: {name} received");
        start_shutdown(&handle, 0);
    });
}

fn main() {
    let paths = XdgPaths::resolve();
    if let Err(error) = paths.ensure_state_dir() {
        eprintln!("araiguma: cannot initialize application state: {error}");
        std::process::exit(1);
    }

    eprintln!(
        "araiguma: configuration path: {}",
        paths.config_file().display()
    );

    let app = tauri::Builder::default()
        .manage(AppState::new(paths))
        .setup(|app| {
            let state = app.state::<AppState>().inner().clone();
            eprintln!(
                "araiguma: state directory: {}",
                state.paths.state_dir.display()
            );
            let driver_state = state.clone();
            let task = tauri::async_runtime::spawn(driver::drive(driver_state));
            state.set_driver_task(task);
            #[cfg(unix)]
            install_shutdown_signals(app.handle());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            ipc::get_snapshot,
            ipc::get_topology,
            ipc::subscribe_watch,
            ipc::unsubscribe_watch,
        ])
        .build(tauri::generate_context!())
        .expect("araiguma desktop build");

    app.run(|app_handle, event| match event {
        tauri::RunEvent::ExitRequested { api, code, .. } => {
            let state = app_handle.state::<AppState>().inner().clone();

            // The final explicit exit is allowed through after cleanup.
            if state.exit_allowed() {
                return;
            }

            // Prevent the first exit request while the live driver stops.
            // The cleanup task calls `exit` again after joining it.
            api.prevent_exit();
            start_shutdown(app_handle, code.unwrap_or(0));
        }
        tauri::RunEvent::WindowEvent {
            event: tauri::WindowEvent::CloseRequested { api, .. },
            ..
        } => {
            // On Linux, closing the last window is not necessarily an
            // application exit request. Keep the window alive while the
            // named driver owner is joined, then perform the explicit
            // application exit from the cleanup task.
            api.prevent_close();
            start_shutdown(app_handle, 0);
        }
        tauri::RunEvent::WindowEvent {
            event: tauri::WindowEvent::Destroyed,
            ..
        } => {
            // Fallback for a close path that could not be prevented.
            app_handle.state::<AppState>().request_shutdown();
        }
        tauri::RunEvent::Exit => {
            // Fallback for an exit path that could not be prevented. The
            // normal path above has already joined the driver.
            app_handle.state::<AppState>().request_shutdown();
        }
        _ => {}
    });
}
