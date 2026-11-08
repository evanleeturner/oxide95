// SPDX-License-Identifier: MIT
//! A made disc through the door and a real back, one arm a run, for the bench to score:
//! `native` over cpal with the `native` feature, `wine` over winmm on a Windows build. A
//! build with neither gets an empty `main`, so the default build needs no sound library.
//!
//! Prints one line of counts on stdout, and exits 1 on any red. A planted arm is meant
//! to read red; the bench judges that.
//!
//! Hazard: the stream opens at load and stays silent until stdin carries the line `go`,
//! so a stream that lands on the wrong output has nothing to play. With no line it waits
//! for ever; the caller owes a timeout. A closed stdin plays nothing and reads red.
//!
//! Hazard: a `seek-*` arm opens no stream, and its hung worker cannot be joined, so it
//! ends the process. `took` and the stream are this bench's own reading of the back, from
//! outside: they do not show that a person heard anything.

#[cfg(all(feature = "door", feature = "ogg", any(windows, feature = "native")))]
#[path = "../bench/made_track.rs"]
mod made_track;

#[cfg(not(all(feature = "door", feature = "ogg", any(windows, feature = "native"))))]
fn main() {}

#[cfg(all(feature = "door", feature = "ogg", any(windows, feature = "native")))]
fn main() -> std::process::ExitCode {
    route::run()
}

#[cfg(all(feature = "door", feature = "ogg", any(windows, feature = "native")))]
mod route {
    use std::io::BufRead as _;
    use std::process::ExitCode;
    use std::rc::Rc;
    use std::sync::atomic::{AtomicU64, Ordering};

    use log::{Level, LevelFilter, Log, Metadata, Record};

    use crate::made_track::{LineCounts, Real, Time, arm_of, run_arm, seek_arm, seek_of};

    #[cfg(not(windows))]
    type Back = sound_mix::native::Native;
    #[cfg(windows)]
    type Back = sound_mix::waveout::WaveOutBack;

    #[cfg(not(windows))]
    const ROUTE: &str = "native";
    #[cfg(windows)]
    const ROUTE: &str = "wine";

    static DROPPED: AtomicU64 = AtomicU64::new(0);
    static SKIP_LINES: AtomicU64 = AtomicU64::new(0);
    static UNREAD: AtomicU64 = AtomicU64::new(0);
    static LINES: Lines = Lines;

    /// Prints each line, and counts the three the scorer reads.
    struct Lines;

    impl Log for Lines {
        fn enabled(&self, metadata: &Metadata<'_>) -> bool {
            metadata.level() <= Level::Debug
        }

        fn log(&self, record: &Record<'_>) {
            if !self.enabled(record.metadata()) {
                return;
            }
            let event = record.args().to_string();
            if event == "sound.frames.dropped" {
                let frames = record
                    .key_values()
                    .get("frames".into())
                    .and_then(|value| parsed_count(&value.to_string()));
                DROPPED.fetch_add(frames.unwrap_or(0), Ordering::Relaxed);
            }
            if event == "cd.track.unread" {
                UNREAD.fetch_add(1, Ordering::Relaxed);
            }
            let by = record
                .key_values()
                .get("by".into())
                .map(|value| value.to_string());
            if by.as_deref() == Some("cd-audio") && event != "imp.call" {
                SKIP_LINES.fetch_add(1, Ordering::Relaxed);
            }
            eprintln!("{}", oxide_logline::body(record));
        }

        fn flush(&self) {}
    }

    /// `oxide_trace::__shown!` writes a count in hex, with no prefix.
    fn parsed_count(text: &str) -> Option<u64> {
        u64::from_str_radix(text, 16).ok()
    }

    fn counted() -> LineCounts {
        LineCounts {
            dropped: DROPPED.load(Ordering::Relaxed),
            skip_lines: SKIP_LINES.load(Ordering::Relaxed),
            unread: UNREAD.load(Ordering::Relaxed),
        }
    }

    fn gate_opened() -> bool {
        let mut line = String::new();
        let read = std::io::stdin().lock().read_line(&mut line);
        matches!(read, Ok(count) if count > 0) && line.trim() == "go"
    }

    pub fn run() -> ExitCode {
        let word = std::env::args().nth(1).unwrap_or_default();
        log::set_logger(&LINES).expect("nothing else sets a logger here");
        log::set_max_level(LevelFilter::Debug);

        if let Some(lie) = seek_of(&word) {
            let read = seek_arm(lie);
            println!("{}", read.line(ROUTE, &word));
            // A hung worker cannot be joined, so the process ends here rather than at `main`.
            std::process::exit(i32::from(read.red().is_some()));
        }
        let Some(arm) = arm_of(&word) else {
            eprintln!(
                "usage: made_route <clean|wrong-track|wrong-place|short-table|no-pump|pace:N|\
                 ogg-clean:PATH|ogg-lie:PATH|ogg-short|ogg-short-lie|\
                 seek-truth|seek-lie|seek-stays>"
            );
            return ExitCode::from(2);
        };
        let time: Rc<dyn Time> = Rc::new(Real::new());
        let outcome = run_arm(&arm, Back::new(), &time, gate_opened, &counted);
        println!("{}", outcome.line(ROUTE, &word));
        if outcome.reds.is_empty() {
            ExitCode::SUCCESS
        } else {
            ExitCode::FAILURE
        }
    }
}
