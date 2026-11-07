// SPDX-License-Identifier: MIT
//! A program whose only job is to write log lines through this crate's sink, so that
//! `bench/logline_check.py` can judge the file from outside the process. It reads nothing
//! back and decides nothing about what it wrote.
//!
//! Call, in the folder the file goes in: `logline_writer <mode> <file> [second file]`
//!
//! - `grammar <file> <second>`: one line at each level, the text and typed values the
//!   check's table holds, then a second `install` on `<second>`, which must be refused.
//! - `million <file>`: 8 threads of 125,000 fixed-width lines.
//! - `newline <file>`: a value holding a line feed, then one holding a carriage return.
//! - `root <file>`: a job root holding a space and the word `base`.
//!
//! Exits 0, or 2 on a bad call, or 3 when the second `install` was not refused.

use std::env;
use std::fs::File;
use std::path::Path;
use std::process::ExitCode;
use std::thread;

use log::{LevelFilter, SetLoggerError};
use oxide_logline::{Roots, install};

const PROGRAM: &str = "logline_writer";
const JOB: &str = "run-job";
const BASE: &str = "run-base";
/// The level with `RUST_LOG` unset: the check reads `T` only when it asks for it.
const DEFAULT_LEVEL: LevelFilter = LevelFilter::Debug;
const THREADS: u32 = 8;
const LINES_EACH: u32 = 125_000;
const EXIT_BAD_CALL: u8 = 2;
const EXIT_INSTALLED_TWICE: u8 = 3;

/// Each text value, in the order the check's table reads them back.
const TEXTS: [&str; 9] = [
    "plain",
    "two words",
    "a\"quote",
    "a=b",
    "",
    "back\\slash",
    "all \"of=it\" \\ here",
    "tab\there",
    "caf\u{e9} \u{2603}",
];

/// Creates `file` and installs the sink on it, the header going first.
fn install_on(file: &str, job: &str) -> Result<(), SetLoggerError> {
    let out = File::create(file).expect("invariant: the run's folder is writable");
    let cwd = env::current_dir().expect("invariant: the working folder exists");
    let roots = Roots {
        job: Path::new(job),
        base: Path::new(BASE),
        cwd: &cwd,
    };
    install(PROGRAM, &roots, out, DEFAULT_LEVEL)
}

fn write_grammar(file: &str, second: &str) -> ExitCode {
    install_on(file, JOB).expect("invariant: the first install takes the facade");
    log::error!("logline.check.error");
    log::warn!("logline.check.warn");
    log::info!("logline.check.info");
    log::debug!("logline.check.debug");
    log::trace!("logline.check.trace");
    for text in TEXTS {
        log::info!(v = text; "logline.check.text");
    }
    log::info!(
        count = 42u64, below = -7i64, ratio = 1.5f64, ok = true, letter = 'x';
        "logline.check.typed"
    );
    log::logger().flush();
    if install_on(second, JOB).is_ok() {
        return ExitCode::from(EXIT_INSTALLED_TWICE);
    }
    ExitCode::SUCCESS
}

fn write_million(file: &str) -> ExitCode {
    install_on(file, JOB).expect("invariant: the first install takes the facade");
    let writers: Vec<_> = (0..THREADS)
        .map(|t| {
            thread::spawn(move || {
                for n in 0..LINES_EACH {
                    let number = format!("{n:06}");
                    log::info!(t = t, n = number.as_str(); "logline.check.million");
                }
            })
        })
        .collect();
    for writer in writers {
        writer.join().expect("invariant: a writer thread only logs");
    }
    log::logger().flush();
    ExitCode::SUCCESS
}

fn write_newline(file: &str) -> ExitCode {
    install_on(file, JOB).expect("invariant: the first install takes the facade");
    log::info!(v = "one\ntwo"; "logline.check.text");
    log::info!(v = "one\rtwo"; "logline.check.text");
    log::logger().flush();
    ExitCode::SUCCESS
}

fn write_root(file: &str) -> ExitCode {
    install_on(file, "my base job").expect("invariant: the first install takes the facade");
    log::info!("logline.check.info");
    log::logger().flush();
    ExitCode::SUCCESS
}

fn main() -> ExitCode {
    let args: Vec<String> = env::args().skip(1).collect();
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    match args.as_slice() {
        ["grammar", file, second] => write_grammar(file, second),
        ["million", file] => write_million(file),
        ["newline", file] => write_newline(file),
        ["root", file] => write_root(file),
        _ => ExitCode::from(EXIT_BAD_CALL),
    }
}
