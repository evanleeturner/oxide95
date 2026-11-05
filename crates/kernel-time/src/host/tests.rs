// SPDX-License-Identifier: MIT
//! The host's clocks against the host's own `date`: the wall
//! clock against `date +%s`. The game clock is the host's instant, so a
//! real wait moves it.
use super::Host;
use crate::Clocks;
use crate::date::UNIX_EPOCH;
use std::process::Command;

fn date(format: &str) -> String {
    let output = Command::new("date")
        .arg(format)
        .output()
        .expect("date runs");
    String::from_utf8_lossy(&output.stdout).trim().to_string()
}

#[test]
fn the_wall_is_dates() {
    let host = Host::new(u64::MAX);
    let seconds = (host.wall() - UNIX_EPOCH) / 10_000_000;
    let theirs: u64 = date("+%s").parse().expect("seconds");
    assert!(seconds.abs_diff(theirs) <= 2, "{seconds} {theirs}");
}

#[test]
fn a_sleep_moves_the_clock() {
    let host = Host::new(u64::MAX);
    let before = host.micros();
    host.sleep(20);
    let after = host.micros();
    assert!(after - before >= 20_000, "{before} {after}");
}
