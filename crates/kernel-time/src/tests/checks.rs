// SPDX-License-Identifier: MIT
//! The crate's clocks against the computer's own: `Instant` around each read,
//! natively `date` for the wall, under Wine Wine's own calls beside the crate.
//! An ordering needs no tolerance, so orderings judge wherever they can: a late
//! read on a busy computer widens a bracket and leaves it true. The one sum with
//! slack, the tick against Wine's, takes its slack from Windows' stated
//! resolution.
//!
//! The rows print under `--nocapture`, one line each, and none stops the run.
use crate as kernel_time;
use crate::host::Host;
use crate::table::DATE_BYTES;
use crate::{Clocks, Times};
use rows::{Book, ROW};
use std::thread;
use std::time::{Duration, Instant};

mod rows;

const BACK: &str = if cfg!(windows) { "W" } else { "N" };

/// Long enough that a clock running double leaves its bracket.
const SETTLE: Duration = Duration::from_millis(50);

/// `system_time`'s words without the weekday, so they order as dates do.
fn dated(bytes: &[u8; DATE_BYTES]) -> [u16; 7] {
    let word = |at: usize| u16::from(bytes[at]) | u16::from(bytes[at + 1]) << 8;
    [
        word(0),
        word(2),
        word(6),
        word(8),
        word(10),
        word(12),
        word(14),
    ]
}

fn counted(times: &Times<Host>) -> i64 {
    let mut counter = 0;
    times.counter(&mut counter);
    counter
}

/// Each read of the 3 names sits inside its `Instant` bracket. `Host`'s clock
/// starts between `outer` and `inner` and telescopes to one floor, so
/// `floor(before - inner) <= micros <= floor(after - outer)`, with no slack.
fn brackets(book: &mut Book<'_>) {
    let outer = Instant::now();
    let host = Host::new(u64::MAX);
    let inner = Instant::now();
    let times = Times::new(&ROW, host);
    let bounds = |before: Instant, after: Instant| {
        (
            (before - inner).as_micros(),
            (after - outer).as_micros(),
        )
    };
    for round in 1..=2 {
        thread::sleep(SETTLE);
        let before = Instant::now();
        let tick = u128::from(times.tick_count());
        let (low, high) = bounds(before, Instant::now());
        book.check(
            "ti1",
            &format!("tick.read{round}"),
            low / 1000 <= tick && tick <= high / 1000,
            &format_args!("{low} {tick} {high}"),
        );
        let before = Instant::now();
        let media = u128::from(times.time_get_time());
        let (low, high) = bounds(before, Instant::now());
        book.check(
            "ti1",
            &format!("media.read{round}"),
            low / 1000 <= media && media <= high / 1000,
            &format_args!("{low} {media} {high}"),
        );
        let before = Instant::now();
        let counter = u128::try_from(counted(&times)).expect("the clock starts at 0");
        let (low, high) = bounds(before, Instant::now());
        book.check(
            "ti1",
            &format!("counter.read{round}"),
            low <= counter && counter <= high,
            &format_args!("{low} {counter} {high}"),
        );
    }
}

/// The 3 names read one clock: tick, then media, then counter, then tick again.
fn order(book: &mut Book<'_>) {
    let times = Times::new(&ROW, Host::new(u64::MAX));
    for round in 1..=2 {
        thread::sleep(SETTLE);
        let tick = i64::from(times.tick_count());
        let media = i64::from(times.time_get_time());
        let counter = counted(&times) / 1000;
        let next = i64::from(times.tick_count());
        book.check(
            "ti1",
            &format!("order.round{round}"),
            tick <= media && media <= counter && counter <= next,
            &format_args!("{tick} {media} {counter} {next}"),
        );
    }
}

/// The counter counts at 1 MHz: its gap lies between the `Instant`s' inner and
/// outer gaps, with 1 microsecond each way, the 2 floors of 2 readings.
fn rate(book: &mut Book<'_>) {
    let times = Times::new(&ROW, Host::new(u64::MAX));
    let before_first = Instant::now();
    let first = counted(&times);
    let after_first = Instant::now();
    thread::sleep(SETTLE);
    let before_last = Instant::now();
    let last = counted(&times);
    let after_last = Instant::now();
    let moved = i128::from(last - first);
    let low = i128::try_from((before_last - after_first).as_micros()).expect("a short wait") - 1;
    let high = i128::try_from((after_last - before_first).as_micros()).expect("a short wait") + 1;
    book.check(
        "ti1",
        "counter.rate",
        low <= moved && moved <= high,
        &format_args!("{low} {moved} {high}"),
    );
}

/// A long gap comes back as one clamp on a host as on a page: by design.
fn clamp(book: &mut Book<'_>) {
    let times = Times::new(&ROW, Host::new(10_000));
    thread::sleep(SETTLE);
    let first = counted(&times);
    thread::sleep(SETTLE);
    let last = counted(&times);
    book.check(
        "ti1",
        "clamp.host",
        first == 10_000 && last == 20_000,
        &format_args!("{first} {last}"),
    );
}

/// `Sleep` waits at least what it is asked; a `Sleep` of 0 returns.
fn sleeps(book: &mut Book<'_>) {
    let times = Times::new(&ROW, Host::new(u64::MAX));
    let start = Instant::now();
    times.sleep(20);
    let took = start.elapsed();
    book.check(
        "ti1",
        "sleep.twenty",
        took >= Duration::from_millis(20),
        &took.as_micros(),
    );
    let start = Instant::now();
    times.sleep(0);
    book.check("ti1", "sleep.zero", true, &start.elapsed().as_micros());
}

/// Local time on a host is system time: it falls between 2 of them.
fn local(book: &mut Book<'_>) {
    let times = Times::new(&ROW, Host::new(u64::MAX));
    let (mut first, mut local, mut last) = ([0; DATE_BYTES], [0; DATE_BYTES], [0; DATE_BYTES]);
    times.system_time(&mut first);
    times.local_time(&mut local);
    times.system_time(&mut last);
    let (first, local, last) = (dated(&first), dated(&local), dated(&last));
    book.check(
        "ti3",
        "local.host",
        first <= local && local <= last,
        &format_args!("{first:?} {local:?} {last:?}"),
    );
}

/// The computer's own `date` around the crate's wall. Under Wine `date` cannot
/// start, so this judge is native alone.
#[cfg(not(windows))]
mod native {
    use super::{Book, Clocks, Host};
    use crate::date::UNIX_EPOCH;
    use std::process::Command;

    fn date(format: &str) -> String {
        let output = Command::new("date")
            .arg(format)
            .output()
            .expect("date runs");
        String::from_utf8_lossy(&output.stdout).trim().to_string()
    }

    fn seconds() -> u64 {
        date("+%s").parse().expect("seconds")
    }

    /// Whole seconds on each side, and no slack: the 2 readings floor alike.
    /// `date` reads the same host clock, so this catches a wrong sum and no
    /// wrong clock.
    pub(super) fn rows(book: &mut Book<'_>) {
        let host = Host::new(u64::MAX);
        let before = seconds();
        let wall = (host.wall() - UNIX_EPOCH) / 10_000_000;
        let after = seconds();
        book.check(
            "ti1",
            "wall.between",
            before <= wall && wall <= after,
            &format_args!("{before} {wall} {after}"),
        );
        book.measure("ti3", "control.zone", &date("+%z"));
    }
}

/// Wine's own calls, beside the crate's.
#[cfg(windows)]
#[allow(unsafe_code, reason = "a check's only way to Wine's own answers")]
mod wine {
    use super::{Book, Clocks, Host, ROW, Times, counted, rows};
    use crate::date::Date;
    use std::thread;
    use std::time::{Duration, Instant};

    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetTickCount() -> u32;
        fn GetSystemTime(out: *mut [u16; 8]);
        fn QueryPerformanceCounter(out: *mut i64) -> i32;
        fn QueryPerformanceFrequency(out: *mut i64) -> i32;
        fn FileTimeToSystemTime(file: *const u64, out: *mut [u16; 8]) -> i32;
        fn GetTimeZoneInformation(out: *mut [u8; 172]) -> u32;
        fn GetLastError() -> u32;
        fn SetLastError(code: u32);
    }

    /// Windows gives `GetTickCount` a resolution of 10 to 16 ms, so 2 reads of
    /// it err by less than one step of 16; the crate's own 2 floors add 1.
    const TICK_SLACK_MS: u32 = 16 + 1;

    fn tick() -> u32 {
        // SAFETY: takes nothing and only answers.
        unsafe { GetTickCount() }
    }

    fn system() -> [u16; 7] {
        let mut out = [0; 8];
        // SAFETY: `out` is the 8 words the call writes.
        unsafe { GetSystemTime(&raw mut out) };
        [
            out[0], out[1], out[3], out[4], out[5], out[6], out[7],
        ]
    }

    fn performance() -> i64 {
        let mut out = 0;
        // SAFETY: `out` is the one number the call writes.
        unsafe { QueryPerformanceCounter(&raw mut out) };
        out
    }

    /// The file time through Wine, the error cleared first: its answer, words and error.
    fn converted(ticks: u64, out: &mut [u16; 8]) -> (i32, u32) {
        // SAFETY: `ticks` lives across the call, `out` is the 8 words it writes;
        // the error calls take and answer numbers.
        unsafe {
            SetLastError(0);
            let ok = FileTimeToSystemTime(&raw const ticks, out);
            (ok, GetLastError())
        }
    }

    /// `GetSystemTime` after 2 of its steps, or after a second if it never steps.
    fn stepped_twice() -> [u16; 7] {
        let start = Instant::now();
        let mut now = system();
        for _ in 0..2 {
            let held = now;
            while now == held && start.elapsed() < Duration::from_secs(1) {
                now = system();
            }
        }
        now
    }

    /// `Host`'s wall between 2 of Wine's readings, whole dates without the
    /// weekday. `GetSystemTime` steps coarser than the wall `std` reads and lags
    /// it, so a reading straight after the wall can show an earlier millisecond.
    /// The later reading therefore waits for 2 steps: the first may have been
    /// taken before the wall and shown after it, the second cannot. An ordering
    /// still, with no slack. They lean on the one host clock, so this catches a
    /// wrong sum and no wrong clock.
    fn wall(book: &mut Book<'_>) {
        let host = Host::new(u64::MAX);
        let before = system();
        let words = Date::of(host.wall()).0;
        let after = stepped_twice();
        let wall = [
            words[0], words[1], words[3], words[4], words[5], words[6], words[7],
        ];
        book.check(
            "ti1",
            "wall.between",
            before <= wall && wall <= after,
            &format_args!("{before:?} {wall:?} {after:?}"),
        );
    }

    /// The crate's tick over a wait against Wine's: the one sum with slack.
    fn tick_slack(book: &mut Book<'_>) {
        let times = Times::new(&ROW, Host::new(u64::MAX));
        let mut worst = 0;
        for _ in 0..3 {
            let (theirs, ours) = (tick(), times.tick_count());
            thread::sleep(Duration::from_millis(250));
            let moved = times.tick_count() - ours;
            let moved_there = tick() - theirs;
            worst = worst.max(moved.abs_diff(moved_there));
        }
        book.check(
            "ti1",
            "tick.slack",
            worst <= TICK_SLACK_MS,
            &format_args!("worst {worst} slack {TICK_SLACK_MS}"),
        );
    }

    /// The counter's 1 MHz against Wine's counter over Wine's own rate, in the
    /// order W0 C0 W1 wait W2 C1 W3, with 1 microsecond each way for 2 floors.
    /// The two rates differ, and no file imports the rate call.
    fn counter_rate(book: &mut Book<'_>) {
        let mut rate = 0;
        // SAFETY: `rate` is the one number the call writes.
        unsafe { QueryPerformanceFrequency(&raw mut rate) };
        book.measure("ti1", "rate.wine", &rate);
        let times = Times::new(&ROW, Host::new(u64::MAX));
        let before_first = performance();
        let first = counted(&times);
        let after_first = performance();
        thread::sleep(super::SETTLE);
        let before_last = performance();
        let last = counted(&times);
        let after_last = performance();
        let micros = |span: i64| i128::from(span) * 1_000_000 / i128::from(rate);
        let moved = i128::from(last - first);
        let low = micros(before_last - after_first) - 1;
        let high = micros(after_last - before_first) + 1;
        book.check(
            "ti1",
            "counter.wine",
            low <= moved && moved <= high,
            &format_args!("{low} {moved} {high}"),
        );
    }

    /// The table through Wine itself, row by row, against the same frozen words.
    fn table(book: &mut Book<'_>) {
        for (name, ticks, want) in rows::TABLE {
            let mut words = [0xBEEF; 8];
            let (ok, error) = converted(ticks, &mut words);
            book.check(
                "ti2",
                &format!("wine.table.{name}"),
                ok == 1 && words == want,
                &format_args!("ok {ok} {words:?} error {error}"),
            );
        }
        for (name, ticks) in rows::REFUSED {
            let mut words = [0xBEEF; 8];
            let (ok, error) = converted(ticks, &mut words);
            book.check(
                "ti2",
                &format!("wine.refuse.{name}"),
                ok == 0 && error == ROW.bad_argument && words == [0xBEEF; 8],
                &format_args!("ok {ok} error {error} {words:?}"),
            );
        }
    }

    /// Wine's own zone, which the crate's answers leave out on purpose: the
    /// control that a `TZ` took.
    fn zone(book: &mut Book<'_>) {
        let mut zone = [0; 172];
        // SAFETY: `zone` is the record the call writes, 172 bytes.
        let id = unsafe { GetTimeZoneInformation(&raw mut zone) };
        let bias = i32::from_le_bytes([zone[0], zone[1], zone[2], zone[3]]);
        book.measure("ti3", "control.zone", &format_args!("id {id} bias {bias}"));
    }

    pub(super) fn rows(book: &mut Book<'_>) {
        wall(book);
        tick_slack(book);
        counter_rate(book);
        table(book);
        zone(book);
    }
}

#[test]
fn every_row_holds() {
    // The harness's own line runs on to the first thing printed.
    println!();
    let mut out = |line: &str| println!("{line}");
    let mut book = Book::new(BACK, &mut out);
    rows::all(&mut book);
    brackets(&mut book);
    order(&mut book);
    rate(&mut book);
    clamp(&mut book);
    sleeps(&mut book);
    local(&mut book);
    #[cfg(not(windows))]
    native::rows(&mut book);
    #[cfg(windows)]
    wine::rows(&mut book);
    assert_eq!(book.fails(), 0, "the failed rows are printed above");
}
