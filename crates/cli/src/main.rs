//! The `araiguma` binary: a thin shell around the library that sets up the
//! tokio runtime, a minimal stderr logger, and signal handling.

use std::io::Write;
use std::process::ExitCode;

use araiguma_cli::{parse_args, run_watch, Command};

/// Minimal stderr logger honoring ARAIGUMA_LOG (error|warn|info|debug|trace).
struct StderrLogger {
    level: log::LevelFilter,
}

impl log::Log for StderrLogger {
    fn enabled(&self, metadata: &log::Metadata) -> bool {
        metadata.level() <= self.level
    }

    fn log(&self, record: &log::Record) {
        if self.enabled(record.metadata()) {
            eprintln!("[araiguma:{}] {}", record.level(), record.args());
        }
    }

    fn flush(&self) {}
}

fn init_logger() {
    let level = match std::env::var("ARAIGUMA_LOG").as_deref() {
        Ok("error") => log::LevelFilter::Error,
        Ok("warn") => log::LevelFilter::Warn,
        Ok("info") => log::LevelFilter::Info,
        Ok("debug") | Ok("trace") => log::LevelFilter::Debug,
        _ => log::LevelFilter::Warn,
    };
    let _ = log::set_boxed_logger(Box::new(StderrLogger { level }));
    log::set_max_level(level);
}

/// Returns the process exit code.
async fn async_main() -> u8 {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match parse_args(&args) {
        Ok(Command::Snapshot { pretty }) => {
            let snapshot = araiguma_cli::observe_merged().await;
            println!("{}", araiguma_cli::render_snapshot(&snapshot, pretty));
            0
        }
        Ok(Command::Watch) => {
            let stdout = std::io::stdout();
            let mut out = std::io::BufWriter::new(stdout.lock());
            let mut stop = std::pin::pin!(shutdown_signal());
            match run_watch(&mut out, &mut stop).await {
                Ok(()) => {
                    let _ = out.flush();
                    0
                }
                Err(message) => {
                    eprintln!("araiguma: {message}");
                    1
                }
            }
        }
        Ok(Command::Help) => {
            println!("{}", araiguma_cli::usage());
            0
        }
        Ok(Command::Version) => {
            println!("araiguma {}", araiguma_cli::VERSION);
            0
        }
        Err(message) => {
            eprintln!("araiguma: {message}\n");
            eprintln!("{}", araiguma_cli::usage());
            2
        }
    }
}

/// Resolve when SIGINT or SIGTERM arrives (or the signal stream closes).
async fn shutdown_signal() {
    let interrupt = async {
        let _ = tokio::signal::ctrl_c().await;
    };
    #[cfg(unix)]
    let terminate = async {
        use tokio::signal::unix::{signal, SignalKind};
        match signal(SignalKind::terminate()) {
            Ok(mut stream) => {
                stream.recv().await;
            }
            Err(_) => std::future::pending::<()>().await,
        }
    };
    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();
    tokio::select! {
        _ = interrupt => {},
        _ = terminate => {},
    }
}

fn main() -> ExitCode {
    init_logger();
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("tokio runtime");
    ExitCode::from(runtime.block_on(async_main()))
}
