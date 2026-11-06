// SPDX-License-Identifier: MIT
//! The fronts on the page's clocks. The dates are taken from
//! `date -u -d @N '+%s %Y %m %w %d %H %M %S'`, never worked by hand.
use super::Times;
use super::clock::Clock;
use super::date::{Date, UNIX_EPOCH};
use super::page::Page;
use super::table::{Calls, Row};
use core::sync::atomic::{AtomicU64, Ordering};

const ROW: Row = Row {
    file: "test",
    calls: Calls {
        tick_count: "KERNEL32!GetTickCount",
        time_get_time: "WINMM!timeGetTime",
        sleep: "KERNEL32!Sleep",
        counter: "KERNEL32!QueryPerformanceCounter",
        system_time: "KERNEL32!GetSystemTime",
        local_time: "KERNEL32!GetLocalTime",
        zone: "KERNEL32!GetTimeZoneInformation",
        to_local_file_time: "KERNEL32!FileTimeToLocalFileTime",
        to_system_time: "KERNEL32!FileTimeToSystemTime",
        time: "_time",
    },
    zone_unknown: 0,
    bad_argument: 0x57,
};

/// `date`'s lines: seconds since 1970, then year, month, weekday, day,
/// hour, minute, second.
const HARVESTED: [(u64, [u16; 7]); 4] = [
    (0, [1970, 1, 4, 1, 0, 0, 0]),
    (951_825_599, [2000, 2, 2, 29, 11, 59, 59]),
    (4_107_542_399, [2100, 2, 0, 28, 23, 59, 59]),
    (1_793_752_674, [2026, 11, 3, 4, 0, 37, 54]),
];

const NOW: u64 = UNIX_EPOCH + 1_793_752_674 * 10_000_000;

fn times(wall: u64) -> Times<Page> {
    Times::new(&ROW, Page::new(Clock::new(250_000, 0), wall))
}

fn words(date: [u16; 7]) -> [u16; 8] {
    [
        date[0], date[1], date[2], date[3], date[4], date[5], date[6], 0,
    ]
}

#[test]
fn system_time_is_dates_harvested() {
    for (seconds, date) in HARVESTED {
        let mut into = [0; 16];
        times(UNIX_EPOCH + seconds * 10_000_000).system_time(&mut into);
        assert_eq!(into, Date(words(date)).to_bytes(), "@{seconds}");
    }
}

#[test]
fn a_file_time_past_two_to_the_63_is_refused() {
    let times = times(NOW);
    let mut into = [0xaa; 16];
    assert_eq!(times.to_system_time(1 << 63, &mut into), 0);
    assert_eq!(into, [0xaa; 16]);
    assert_eq!(times.clocks().last_error(), 0x57);
    assert_eq!(times.to_system_time(NOW, &mut into), 1);
}

/// `Sleep` on the page never blocks, and moves no clock.
#[test]
fn sleep_on_the_page_returns_at_once() {
    let times = times(NOW);
    times.clocks().frame(0);
    times.clocks().frame(10_000);
    times.sleep(u32::MAX);
    assert_eq!(times.clocks().clock().micros(), 10_000);
    assert_eq!(times.tick_count(), 10);
}

/// Through the front: 3,600 frames of 16,667 us count 15,000
/// ticks of 4 ms off `GetTickCount`, as the clock's own test counts.
#[test]
fn measured_frames_count_through_the_front() {
    let times = times(NOW);
    let (mut last, mut carry, mut ticks) = (times.tick_count(), 0u32, 0u32);
    for frame in 0..=3_600u64 {
        times.clocks().frame(frame * 16_667);
        let now = times.tick_count();
        carry += now.wrapping_sub(last);
        last = now;
        ticks += carry / 4;
        carry %= 4;
    }
    assert_eq!(ticks, 15_000);
    let mut counter = 0;
    assert_eq!(times.counter(&mut counter), 1);
    assert_eq!(counter, 3_600 * 16_667);
}

/// One clock under both names: one instant, one millisecond.
#[test]
fn both_names_read_one_clock() {
    let times = times(NOW);
    times.clocks().frame(0);
    times.clocks().frame(123_456);
    assert_eq!(times.tick_count(), 123);
    assert_eq!(times.time_get_time(), 123);
}

/// The count wraps at 2^32 ms, and a reader's unsigned subtraction across
/// the wrap still reads the 3 ms that passed.
#[test]
fn time_get_time_wraps_at_two_to_the_32_milliseconds() {
    const WRAP: u64 = (1 << 32) * 1000;
    let times = Times::new(&ROW, Page::new(Clock::new(u64::MAX, WRAP - 2_000), NOW));
    times.clocks().frame(0);
    let before = times.time_get_time();
    assert_eq!(before, u32::MAX - 1);
    times.clocks().frame(3_000);
    let after = times.time_get_time();
    assert_eq!(after, 1);
    assert_eq!(after.wrapping_sub(before), 3);
}

/// A wait spun on the front ends once each read moves the clock, and only
/// then: a page nothing frames answers the same millisecond for good.
#[test]
fn a_spin_on_the_front_ends_only_with_a_moving_read() {
    static HOST: AtomicU64 = AtomicU64::new(0);
    fn read() -> u64 {
        HOST.fetch_add(500, Ordering::Relaxed)
    }
    let moving = Times::new(&ROW, Page::reading(Clock::new(u64::MAX, 0), NOW, read));
    let start = moving.time_get_time();
    assert!((0..100).any(|_| moving.time_get_time().wrapping_sub(start) >= 10));
    let fixed = times(NOW);
    let start = fixed.time_get_time();
    assert!(!(0..100).any(|_| fixed.time_get_time().wrapping_sub(start) >= 10));
}

/// The C library's `time`: the wall's whole seconds, into the timer when
/// there is one; a wall before 1970 answers 0.
#[test]
fn seconds_are_the_walls_and_fill_the_timer() {
    let mut timer = 0;
    assert_eq!(
        times(NOW + 9_999_999).seconds(Some(&mut timer)),
        1_793_752_674
    );
    assert_eq!(timer, 1_793_752_674);
    assert_eq!(times(UNIX_EPOCH - 1).seconds(None), 0);
}
