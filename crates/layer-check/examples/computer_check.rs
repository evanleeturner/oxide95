// SPDX-License-Identifier: MIT
//! The computer's own checks, by `cpal` and the system's sound tools, with no crate of
//! this workspace in the way, so a red here clears the project's code.
//!
//! - `c7`: the default output opens. Nothing plays: the stream is built and dropped.
//! - `tone <hz> <samples> <amplitude>`: writes a tone on stdout, 16-bit mono at
//!   [`RATE`], for `pacat` to hand the mixer.
//! - `hears <file> <hz> <amplitude>`: says whether a `parec` recording holds that tone.
//!
//! `c7` and `hears` print one result line on stdout and log on stderr, more at
//! `RUST_LOG=debug`. Exits 0 on a pass, 1 on red, 2 on a call it cannot read.
use std::fmt::Write as _;
use std::io::Write as _;
use std::process::ExitCode;

use cpal::traits::{DeviceTrait, HostTrait};
use layer_check::{Heard, RATE, judge, peak, share_at, tone};

const C7_PASS: &str = "A sound device opens for programs. Nothing was played.";
const C7_RED: &str =
    "This computer offers no sound device to programs. Check that one is plugged in and turned on.";
const C9_PASS: &str = "The computer's sound mixer heard the tone that was played.";
const C9_SILENCE: &str = "The computer's sound mixer heard silence while a tone played.";
const C9_CHANGED: &str = "The computer's sound mixer changed the tone it was given.";

/// Writes the log's lines to stderr, the data channel staying stdout's.
struct Sink;

impl log::Log for Sink {
    fn enabled(&self, _: &log::Metadata<'_>) -> bool {
        true
    }

    fn log(&self, record: &log::Record<'_>) {
        let mut line = format!("{} {}", record.level(), record.args());
        // A value that cannot print leaves the rest of the line as it stands.
        let _ = record.key_values().visit(&mut Pairs(&mut line));
        eprintln!("{line}");
    }

    fn flush(&self) {}
}

/// Adds a log call's named values to its line.
struct Pairs<'a>(&'a mut String);

impl<'kvs> log::kv::VisitSource<'kvs> for Pairs<'_> {
    fn visit_pair(
        &mut self,
        key: log::kv::Key<'kvs>,
        value: log::kv::Value<'kvs>,
    ) -> Result<(), log::kv::Error> {
        write!(self.0, " {key}={value}").map_err(|_| log::kv::Error::msg("the line refused"))
    }
}

/// Prints the check's one result line and the log's summary, and makes the exit code.
fn report(check: &str, passed: bool, words: &str) -> ExitCode {
    let state = if passed { "pass" } else { "red" };
    println!("{check} {state} {words}");
    log::info!(check = check, state = state; "check.computer.result");
    log::info!(results = 1, reds = u8::from(!passed); "check.computer.done");
    if passed {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}

/// Opens the default output and closes it again without starting it, so nothing plays.
fn open_default() -> Result<(), String> {
    let device = cpal::default_host()
        .default_output_device()
        .ok_or("the system names no default output")?;
    let config = device
        .default_output_config()
        .map_err(|why| why.to_string())?;
    log::debug!(
        rate:? = config.sample_rate(), channels = config.channels(),
        format:% = config.sample_format();
        "check.computer.device"
    );
    let stream = device
        .build_output_stream_raw(
            config.config(),
            config.sample_format(),
            |data: &mut cpal::Data, _: &cpal::OutputCallbackInfo| data.bytes_mut().fill(0),
            |_| {},
            None,
        )
        .map_err(|why| why.to_string())?;
    drop(stream);
    Ok(())
}

fn c7() -> ExitCode {
    match open_default() {
        Ok(()) => report("C7", true, C7_PASS),
        Err(why) => {
            log::debug!(why = why.as_str(); "check.computer.refused");
            report("C7", false, C7_RED)
        }
    }
}

/// Writes a tone on stdout; only the writing can fail.
fn play(hz: f64, count: u32, amplitude: i16) -> ExitCode {
    let bytes: Vec<u8> = tone(hz, count, amplitude)
        .iter()
        .flat_map(|sample| sample.to_le_bytes())
        .collect();
    match std::io::stdout().lock().write_all(&bytes) {
        Ok(()) => ExitCode::SUCCESS,
        Err(why) => {
            log::debug!(why:% = why; "check.computer.refused");
            ExitCode::FAILURE
        }
    }
}

fn hears(path: &str, hz: f64, amplitude: i16) -> ExitCode {
    let bytes = match std::fs::read(path) {
        Ok(bytes) => bytes,
        Err(why) => {
            log::debug!(why:% = why; "check.computer.refused");
            return ExitCode::from(2);
        }
    };
    let caught: Vec<i16> = bytes
        .chunks_exact(2)
        .map(|pair| i16::from_le_bytes([pair[0], pair[1]]))
        .collect();
    log::debug!(
        samples = caught.len(), peak = peak(&caught), share:% = share_at(&caught, hz);
        "check.computer.heard"
    );
    match judge(&caught, hz, amplitude) {
        Heard::Tone => report("C9", true, C9_PASS),
        Heard::Silence => report("C9", false, C9_SILENCE),
        Heard::Changed => report("C9", false, C9_CHANGED),
    }
}

/// Reads the call, or says the call cannot be read.
fn run(args: &[&str]) -> Option<ExitCode> {
    Some(match *args {
        ["c7"] => c7(),
        ["tone", hz, count, amplitude] => play(
            hz.parse().ok()?,
            count.parse().ok()?,
            amplitude.parse().ok()?,
        ),
        ["hears", path, hz, amplitude] => hears(path, hz.parse().ok()?, amplitude.parse().ok()?),
        _ => return None,
    })
}

fn main() -> ExitCode {
    log::set_logger(&Sink).expect("invariant: only main installs the log's sink, once");
    log::set_max_level(
        std::env::var("RUST_LOG")
            .ok()
            .and_then(|level| level.parse().ok())
            .unwrap_or(log::LevelFilter::Info),
    );
    let args: Vec<String> = std::env::args().skip(1).collect();
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    run(&args).unwrap_or_else(|| {
        eprintln!("usage: computer_check c7 | tone <hz> <samples> <amplitude> | hears <file> <hz> <amplitude>; samples are at {RATE} a second");
        ExitCode::from(2)
    })
}
