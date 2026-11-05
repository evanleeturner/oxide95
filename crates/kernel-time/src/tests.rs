// SPDX-License-Identifier: MIT
//! The fronts on the page's clocks. The dates are taken from
//! `date -u -d @N '+%s %Y %m %w %d %H %M %S'`, never worked by hand.
use super::Times;
use super::clock::Clock;
use super::date::{Date, UNIX_EPOCH};
use super::page::Page;
use super::table::{Calls, Row};

const ROW: Row = Row {
    file: "test",
    calls: Calls {
        tick_count: "KERNEL32!GetTickCount",
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
