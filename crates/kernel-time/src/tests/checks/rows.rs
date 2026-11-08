// SPDX-License-Identifier: MIT
//! The rows that read alike on every back, each on a page's fixed clock, so
//! each answer is exact. Two callers hold this file: the crate's test,
//! natively and under Wine, and a probe in a browser, so a page's green reads
//! these same rows. The caller names the crate `kernel_time`, as the two reach
//! it by different roads, and hands in where a line goes.
//!
//! A row never stops the run: every row prints, in a red run too, and the
//! caller reads the count of fails at its end.
use super::kernel_time::Times;
use super::kernel_time::clock::Clock;
use super::kernel_time::date::{Date, UNIX_EPOCH};
use super::kernel_time::page::Page;
use super::kernel_time::table::{Calls, DATE_BYTES, Row, ZONE_BYTES};
use core::fmt::Display;

/// A row of this check's own: names of its own, the id Windows gives no zone
/// in force, and Windows' code for a bad argument.
pub(super) const ROW: Row = Row {
    file: "own",
    calls: Calls {
        tick_count: "tick",
        time_get_time: "media",
        sleep: "sleep",
        counter: "counter",
        system_time: "system",
        local_time: "local",
        zone: "zone",
        to_local_file_time: "to_local",
        to_system_time: "to_system",
        time: "seconds",
    },
    zone_unknown: 0,
    bad_argument: 87,
};

/// File times and their dates, as a `SYSTEMTIME`'s 8 words: year, month,
/// weekday, day, hour, minute, second, millisecond. Each row was taken from a
/// run of Wine's `FileTimeToSystemTime` and, where it reaches, Python's
/// `datetime`, then frozen here. The last is the largest file time Windows
/// takes, far past `datetime`'s year 9999.
pub(super) const TABLE: [(&str, u64, [u16; 8]); 9] = [
    ("zero", 0, [1601, 1, 1, 1, 0, 0, 0, 0]),
    ("unix", 116_444_736_000_000_000, [1970, 1, 4, 1, 0, 0, 0, 0]),
    ("y2k", 125_911_584_000_000_000, [2000, 1, 6, 1, 0, 0, 0, 0]),
    (
        "leap",
        125_962_560_000_000_000,
        [2000, 2, 2, 29, 0, 0, 0, 0],
    ),
    ("y1900", 94_405_824_000_000_000, [1900, 3, 4, 1, 0, 0, 0, 0]),
    (
        "y2100",
        157_520_160_000_000_000,
        [2100, 3, 1, 1, 0, 0, 0, 0],
    ),
    (
        "end",
        125_911_583_999_990_000,
        [1999, 12, 5, 31, 23, 59, 59, 999],
    ),
    (
        "floored",
        116_444_736_000_009_999,
        [1970, 1, 4, 1, 0, 0, 0, 0],
    ),
    (
        "max_signed",
        9_223_372_036_854_775_807,
        [30828, 9, 4, 14, 2, 48, 5, 477],
    ),
];

/// File times at or past 2^63: Windows refuses them.
pub(super) const REFUSED: [(&str, u64); 2] =
    [("sign", 9_223_372_036_854_775_808), ("max", u64::MAX)];

/// A byte no answer writes, to read that a refusal wrote nothing.
const SENTINEL: u8 = 0xEE;

/// A wall clock moment, whole seconds since 1970 on a date of no note.
const NOW_SECONDS: u64 = 1_793_752_674;

/// File-time ticks a second.
const TICKS: u64 = 10_000_000;

/// The last microsecond before the milliseconds pass 32 bits.
const BEFORE_WRAP: u64 = (1 << 32) * 1000 - 1;

/// Where the rows go, and how many failed.
pub(super) struct Book<'a> {
    back: &'a str,
    fails: u32,
    out: &'a mut dyn FnMut(&str),
}

impl<'a> Book<'a> {
    pub(super) fn new(back: &'a str, out: &'a mut dyn FnMut(&str)) -> Self {
        Self {
            back,
            fails: 0,
            out,
        }
    }

    /// One row, `told` being what it read, right or wrong.
    pub(super) fn check(&mut self, check: &str, name: &str, pass: bool, told: &dyn Display) {
        let verdict = if pass { "PASS" } else { "FAIL" };
        if !pass {
            self.fails += 1;
        }
        (self.out)(&format!(
            "CHECK {check} {} {name} = {verdict} {told}",
            self.back
        ));
    }

    /// A reading with no judge in this run.
    pub(super) fn measure(&mut self, check: &str, name: &str, told: &dyn Display) {
        (self.out)(&format!("MEASURE {check} {} {name} = {told}", self.back));
    }

    pub(super) fn fails(&self) -> u32 {
        self.fails
    }
}

/// Every row here, in the order they print.
pub(super) fn all(book: &mut Book<'_>) {
    floors(book);
    wraps(book);
    gaps(book);
    table(book);
    refusals(book);
    bytes(book);
    seconds(book);
    zones(book);
}

fn fixed(clamp: u64, start: u64, wall: u64) -> Times<Page> {
    Times::new(&ROW, Page::new(Clock::new(clamp, start), wall))
}

/// The millisecond names floor, and the counter keeps every microsecond.
fn floors(book: &mut Book<'_>) {
    let times = fixed(u64::MAX, 1_999, 0);
    let tick = times.tick_count();
    book.check("ti1", "tick.floor", tick == 1, &tick);
    let media = times.time_get_time();
    book.check("ti1", "media.floor", media == 1, &media);
    let mut counter = 0;
    let ok = times.counter(&mut counter);
    book.check(
        "ti1",
        "counter.floor",
        ok == 1 && counter == 1_999,
        &format_args!("ok {ok} {counter}"),
    );
}

/// The millisecond names pass 32 bits by wrapping; the counter does not wrap.
fn wraps(book: &mut Book<'_>) {
    let times = fixed(u64::MAX, BEFORE_WRAP, 0);
    let (tick_last, media_last) = (times.tick_count(), times.time_get_time());
    times.clocks().frame(0);
    times.clocks().frame(1);
    let (tick_zero, media_zero) = (times.tick_count(), times.time_get_time());
    times.clocks().frame(1_001);
    let (tick_one, media_one) = (times.tick_count(), times.time_get_time());
    book.check(
        "ti1",
        "tick.wrap",
        (tick_last, tick_zero, tick_one) == (u32::MAX, 0, 1),
        &format_args!("{tick_last} {tick_zero} {tick_one}"),
    );
    book.check(
        "ti1",
        "media.wrap",
        (media_last, media_zero, media_one) == (u32::MAX, 0, 1),
        &format_args!("{media_last} {media_zero} {media_one}"),
    );
    let mut counter = 0;
    times.counter(&mut counter);
    book.check(
        "ti1",
        "counter.wrap",
        counter == 4_294_967_297_000,
        &counter,
    );
}

/// A frame moves the clock by its gap, and a gap past the clamp by one clamp.
/// Windows' tick has no clamp: this is the crate's by design.
fn gaps(book: &mut Book<'_>) {
    let times = fixed(u64::MAX, 0, 0);
    times.clocks().frame(1_000);
    times.clocks().frame(3_500);
    let mut counter = 0;
    times.counter(&mut counter);
    book.check("ti1", "gap.exact", counter == 2_500, &counter);
    let times = fixed(1_000, 0, 0);
    times.clocks().frame(0);
    times.clocks().frame(50_000);
    let mut counter = 0;
    times.counter(&mut counter);
    book.check("ti1", "clamp.fixed", counter == 1_000, &counter);
}

fn words_of(bytes: &[u8; DATE_BYTES]) -> [u16; 8] {
    let mut words = [0; 8];
    for (word, pair) in words.iter_mut().zip(bytes.chunks_exact(2)) {
        *word = u16::from(pair[0]) | u16::from(pair[1]) << 8;
    }
    words
}

/// Every file time to its date: a sentinel under it, and a success keeps no error.
fn table(book: &mut Book<'_>) {
    for (name, ticks, want) in TABLE {
        let times = fixed(1, 0, 0);
        // The slot outlives a store: a fresh 0 is set, not assumed.
        kernel_last_error::set(0);
        let mut bytes = [SENTINEL; DATE_BYTES];
        let ok = times.to_system_time(ticks, &mut bytes);
        let words = words_of(&bytes);
        let error = times.clocks().last_error();
        book.check(
            "ti2",
            &format!("table.{name}"),
            ok == 1 && words == want && error == 0,
            &format_args!("ok {ok} {words:?} error {error}"),
        );
    }
}

/// A time at or past 2^63 is refused: nothing written, the error kept; and a
/// success after it leaves the error alone.
fn refusals(book: &mut Book<'_>) {
    for (name, ticks) in REFUSED {
        let times = fixed(1, 0, 0);
        let mut bytes = [SENTINEL; DATE_BYTES];
        let ok = times.to_system_time(ticks, &mut bytes);
        let stayed = bytes == [SENTINEL; DATE_BYTES];
        let error = times.clocks().last_error();
        book.check(
            "ti2",
            &format!("refuse.{name}"),
            ok == 0 && stayed && error == ROW.bad_argument,
            &format_args!("ok {ok} stayed {stayed} error {error}"),
        );
        let ok = times.to_system_time(UNIX_EPOCH, &mut bytes);
        let kept = times.clocks().last_error();
        book.check(
            "ti2",
            &format!("refuse.{name}.kept"),
            ok == 1 && kept == ROW.bad_argument,
            &format_args!("ok {ok} error {kept}"),
        );
    }
}

/// The 8 words, low byte first.
fn bytes(book: &mut Book<'_>) {
    let bytes = Date::of(125_911_583_999_990_000).to_bytes();
    let want = [
        0xCF, 0x07, 12, 0, 5, 0, 31, 0, 23, 0, 59, 0, 59, 0, 0xE7, 0x03,
    ];
    book.check(
        "ti2",
        "bytes.end",
        bytes == want,
        &format_args!("{bytes:?}"),
    );
}

/// The C library's `time`, a stand-in's own edges: no document names them.
fn seconds(book: &mut Book<'_>) {
    let now = UNIX_EPOCH + NOW_SECONDS * TICKS;
    let want = i32::try_from(NOW_SECONDS).expect("fits");
    let before = fixed(1, 0, UNIX_EPOCH - 1).seconds(None);
    book.check("ti2", "seconds.before", before == 0, &before);
    let exact = fixed(1, 0, now).seconds(None);
    book.check("ti2", "seconds.exact", exact == want, &exact);
    let past = fixed(1, 0, UNIX_EPOCH + (1 << 31) * TICKS).seconds(None);
    book.check("ti2", "seconds.past", past == i32::MAX, &past);
    let mut timer = -1;
    let answer = fixed(1, 0, now).seconds(Some(&mut timer));
    book.check(
        "ti2",
        "seconds.timer",
        answer == want && timer == want,
        &format_args!("{answer} {timer}"),
    );
}

/// No front asks for a zone: the answers are UTC, whatever zone the host keeps.
fn zones(book: &mut Book<'_>) {
    let wall = UNIX_EPOCH + NOW_SECONDS * TICKS;
    let times = fixed(1, 0, wall);
    let mut record = [SENTINEL; ZONE_BYTES];
    let id = times.zone(&mut record);
    let bare = record.iter().filter(|byte| **byte != 0).count();
    book.check(
        "ti3",
        "zone.record",
        id == ROW.zone_unknown && bare == 0,
        &format_args!("id {id} nonzero {bare}"),
    );
    let mut moved = 0;
    let ok = times.to_local_file_time(wall, &mut moved);
    book.check(
        "ti3",
        "local.file",
        ok == 1 && moved == wall,
        &format_args!("ok {ok} {moved}"),
    );
    let (mut system, mut local) = ([0; DATE_BYTES], [0; DATE_BYTES]);
    times.system_time(&mut system);
    times.local_time(&mut local);
    book.check(
        "ti3",
        "local.fixed",
        local == system,
        &format_args!("{:?}", words_of(&local)),
    );
}
