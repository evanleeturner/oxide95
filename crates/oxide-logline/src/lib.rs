// SPDX-License-Identifier: MIT
//! The logline adapter: a sink for the `log` facade that writes the
//! workspace's line grammar, the two-line header first, then one line per
//! event:
//!
//! ```text
//! = <program> run <run-id> fmt 1 date <YYYY-MM-DD> tz utc
//! = job <job-root> base <workspace-root> cwd <working-dir>
//! HH:MM:SS.mmm L event key=value key=value
//! ```
//!
//! A line carries the event's id and its values only; the sentence for
//! each event lives in the workspace's `logbook.json`. The adapter renders,
//! it never evaluates: the values arrive already computed through the
//! record's key-values. Libraries log to the facade and never install this;
//! a test or a binary does, once, at its root.
//!
//! Call: [`install`] `(program, roots, writer, default level)`, or [`body`]
//! `(record) -> "event k=v"` for a test that reads its own lines.

use std::fmt::Write as _;
use std::io::Write;
use std::path::Path;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use log::kv::{Error, Key, Value, VisitSource};
use log::{Level, LevelFilter, Log, Metadata, Record, SetLoggerError};

#[cfg(feature = "fence")]
pub mod fence;

/// The grammar's version, stated in the header's `fmt` field.
pub const GRAMMAR: u32 = 1;

/// The header's path roots: the job's records, the workspace, and the
/// working directory the run started in.
pub struct Roots<'a> {
    pub job: &'a Path,
    pub base: &'a Path,
    pub cwd: &'a Path,
}

/// Writes each enabled record as one grammar line, whole, under a lock, so
/// two threads never shear a line.
struct Sink<W: Write + Send> {
    out: Mutex<W>,
    level: LevelFilter,
}

impl<W: Write + Send> Log for Sink<W> {
    fn enabled(&self, metadata: &Metadata<'_>) -> bool {
        metadata.level() <= self.level
    }

    fn log(&self, record: &Record<'_>) {
        if !self.enabled(record.metadata()) {
            return;
        }
        let line = format!(
            "{} {} {}\n",
            clock(SystemTime::now()),
            letter(record.level()),
            body(record)
        );
        if let Ok(mut out) = self.out.lock() {
            // A line that cannot be written is dropped: the program's
            // results never depend on its sink.
            let _ = out.write_all(line.as_bytes());
        }
    }

    fn flush(&self) {
        if let Ok(mut out) = self.out.lock() {
            let _ = out.flush();
        }
    }
}

/// Installs the sink for the whole process: claims the facade, writes the
/// header to `out`, then routes every record at or above the level to it.
/// The level is `RUST_LOG` when it names one (`debug`, `info`, …), else
/// `default`. Fails, writing nothing, when a logger is already installed.
pub fn install<W: Write + Send + 'static>(
    program: &str,
    roots: &Roots<'_>,
    out: W,
    default: LevelFilter,
) -> Result<(), SetLoggerError> {
    let level = std::env::var("RUST_LOG")
        .ok()
        .and_then(|name| name.trim().parse().ok())
        .unwrap_or(default);
    let sink: &'static Sink<W> = Box::leak(Box::new(Sink {
        out: Mutex::new(out),
        level,
    }));
    log::set_logger(sink)?;
    // The facade's level is still off: no line can come before the header.
    if let Ok(mut out) = sink.out.lock() {
        let _ = out.write_all(header(program, roots, SystemTime::now()).as_bytes());
    }
    log::set_max_level(level);
    Ok(())
}

/// The two header lines, each run-constant stated once.
fn header(program: &str, roots: &Roots<'_>, now: SystemTime) -> String {
    let since = now.duration_since(UNIX_EPOCH).unwrap_or_default();
    let (year, month, day) = civil(since.as_secs() / 86_400);
    format!(
        "= {program} run {:x}-{:x} fmt {GRAMMAR} date {year:04}-{month:02}-{day:02} tz utc\n\
         = job {} base {} cwd {}\n",
        since.as_secs(),
        std::process::id(),
        roots.job.display(),
        roots.base.display(),
        roots.cwd.display(),
    )
}

/// The line's text after its time and level: the event id, then each field
/// as `key=value`, a value with a space, a quote or `=` double-quoted.
#[must_use]
pub fn body(record: &Record<'_>) -> String {
    struct Fields(String);
    impl<'kvs> VisitSource<'kvs> for Fields {
        fn visit_pair(&mut self, key: Key<'kvs>, value: Value<'kvs>) -> Result<(), Error> {
            let text = value.to_string();
            if text.is_empty() || text.contains([' ', '"', '=']) {
                let quoted = text.replace('\\', "\\\\").replace('"', "\\\"");
                let _ = write!(self.0, " {key}=\"{quoted}\"");
            } else {
                let _ = write!(self.0, " {key}={text}");
            }
            Ok(())
        }
    }
    let mut fields = Fields(record.args().to_string());
    let _ = record.key_values().visit(&mut fields);
    fields.0
}

/// The level as the grammar's one letter.
fn letter(level: Level) -> char {
    match level {
        Level::Error => 'E',
        Level::Warn => 'W',
        Level::Info => 'I',
        Level::Debug => 'D',
        Level::Trace => 'T',
    }
}

/// The time of day in UTC, to the millisecond: `HH:MM:SS.mmm`.
fn clock(now: SystemTime) -> String {
    let since = now.duration_since(UNIX_EPOCH).unwrap_or_default();
    let seconds = since.as_secs() % 86_400;
    format!(
        "{:02}:{:02}:{:02}.{:03}",
        seconds / 3_600,
        seconds / 60 % 60,
        seconds % 60,
        since.subsec_millis()
    )
}

/// The proleptic Gregorian date of a day counted from 1970-01-01 (Howard
/// Hinnant's `civil_from_days`, for days on or after the epoch).
fn civil(days: u64) -> (u64, u64, u64) {
    let z = days + 719_468;
    let era = z / 146_097;
    let doe = z % 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + u64::from(month <= 2);
    (year, month, day)
}

#[cfg(test)]
mod tests {
    use super::civil;

    #[test]
    fn civil_dates_known_days() {
        assert_eq!(civil(0), (1970, 1, 1));
        assert_eq!(civil(11_016), (2000, 2, 29));
        assert_eq!(civil(20_759), (2026, 11, 2));
    }
}
