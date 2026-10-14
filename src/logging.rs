//! Logging setup for the whole app.
//!
//! `env_logger` is a dependency but was never initialised, so every
//! `log::warn!`/`error!` call in the project was a no-op. Inside the `.app`
//! bundle stderr is not visible either, so a plain stderr logger alone would
//! not help the user.
//!
//! [`init`] therefore installs a tiny [`log::Log`] wrapper with **two sinks**:
//!
//! 1. the finished `env_logger::Logger` (stderr / `RUST_LOG`),
//! 2. a log **file** `<config dir>/prime-photos.log`, alongside
//!    `config.toml`/`dedup.sqlite3`, opened in append mode.
//!
//! It honours `RUST_LOG` but defaults to `warn`, which is the level of the
//! existing calls. Neither `init` nor a log write ever panics: if the file
//! cannot be opened, only the stderr branch stays active.

use std::io::Write;
use std::path::PathBuf;
use std::sync::Mutex;

use log::{Log, Metadata, Record};

/// Writes every accepted record to the `env_logger` sink **and** to the log
/// file. The file half is a boxed `Write` so it can be swapped for a pure
/// in-memory sink in tests.
struct DualLogger {
    console: env_logger::Logger,
    file: Option<Mutex<Box<dyn Write + Send>>>,
}

impl Log for DualLogger {
    fn enabled(&self, metadata: &Metadata<'_>) -> bool {
        self.console.enabled(metadata)
    }

    fn log(&self, record: &Record<'_>) {
        // Both sinks follow the same filter: a record the console rejects is not
        // written to the file either.
        if !self.console.matches(record) {
            return;
        }
        self.console.log(record);

        if let Some(sink) = self.file.as_ref() {
            if let Ok(mut writer) = sink.lock() {
                let timestamp = chrono::Local::now().format("%Y-%m-%d %H:%M:%S%.3f");
                // A failed file write must never panic — it is dropped.
                let _ = writeln!(
                    writer,
                    "{timestamp} {:<5} {}: {}",
                    record.level(),
                    record.target(),
                    record.args()
                );
            }
        }
    }

    fn flush(&self) {
        self.console.flush();
        if let Some(sink) = self.file.as_ref() {
            if let Ok(mut writer) = sink.lock() {
                let _ = writer.flush();
            }
        }
    }
}

/// Install the global logger. Never panics; if the log file cannot be opened,
/// logging simply continues on stderr only.
pub(crate) fn init() {
    let console = env_logger::Builder::from_env(
        env_logger::Env::default().default_filter_or("warn"),
    )
    .build();
    let max_level = console.filter();

    let logger = DualLogger {
        console,
        file: open_log_file(),
    };

    // `set_boxed_logger` fails only if a logger is already installed; ignore it
    // instead of panicking.
    if log::set_boxed_logger(Box::new(logger)).is_ok() {
        log::set_max_level(max_level);
    }
}

/// `<config dir>/prime_photos/prime-photos.log`, built the same way as
/// `config.rs`/`dedup.rs` build their paths. `None` if no config directory can
/// be determined.
fn log_path() -> Option<PathBuf> {
    Some(dirs::config_dir()?.join("prime_photos").join("prime-photos.log"))
}

/// Append-mode file sink; `None` on any error (missing dir, no permission, …).
fn open_log_file() -> Option<Mutex<Box<dyn Write + Send>>> {
    let path = log_path()?;
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .ok()?;
    Some(Mutex::new(Box::new(file)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use log::LevelFilter;
    use std::io;
    use std::sync::Arc;

    /// Pure in-memory `Write` sink (no filesystem, no environment).
    #[derive(Clone)]
    struct SharedSink(Arc<Mutex<Vec<u8>>>);

    impl Write for SharedSink {
        fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
            self.0.lock().unwrap().extend_from_slice(buf);
            Ok(buf.len())
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    fn captured(buffer: &Arc<Mutex<Vec<u8>>>) -> String {
        String::from_utf8(buffer.lock().unwrap().clone()).unwrap()
    }

    /// A record that passes the filter reaches **both** sinks.
    #[test]
    fn dual_logger_writes_to_both_sinks() {
        let console_buf = Arc::new(Mutex::new(Vec::new()));
        let file_buf = Arc::new(Mutex::new(Vec::new()));

        let console = env_logger::Builder::new()
            .filter_level(LevelFilter::Warn)
            .target(env_logger::Target::Pipe(Box::new(SharedSink(
                console_buf.clone(),
            ))))
            .build();

        let logger = DualLogger {
            console,
            file: Some(Mutex::new(Box::new(SharedSink(file_buf.clone())))),
        };

        let record = Record::builder()
            .level(log::Level::Warn)
            .target("prime_photos::tests")
            .args(format_args!("hello dual"))
            .build();
        logger.log(&record);
        logger.flush();

        assert!(
            captured(&console_buf).contains("hello dual"),
            "console sink missed the record"
        );
        assert!(
            captured(&file_buf).contains("hello dual"),
            "file sink missed the record"
        );
    }

    /// A record below the filter is written to neither sink.
    #[test]
    fn dual_logger_drops_filtered_records() {
        let console_buf = Arc::new(Mutex::new(Vec::new()));
        let file_buf = Arc::new(Mutex::new(Vec::new()));

        let console = env_logger::Builder::new()
            .filter_level(LevelFilter::Warn)
            .target(env_logger::Target::Pipe(Box::new(SharedSink(
                console_buf.clone(),
            ))))
            .build();

        let logger = DualLogger {
            console,
            file: Some(Mutex::new(Box::new(SharedSink(file_buf.clone())))),
        };

        let record = Record::builder()
            .level(log::Level::Info)
            .target("prime_photos::tests")
            .args(format_args!("too quiet"))
            .build();
        logger.log(&record);
        logger.flush();

        assert!(captured(&console_buf).is_empty(), "console leaked info");
        assert!(captured(&file_buf).is_empty(), "file leaked info");
    }
}
