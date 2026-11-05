// SPDX-License-Identifier: MIT
//! A host's clocks, Linux's and macOS's, through `std` alone: the game
//! clock moved a frame at each read from the host's own instant, the wall
//! clock `SystemTime`, `Sleep` a real wait. No zone is read: the
//! fronts answer UTC whatever zone the host keeps.

use crate::clock::Clock;
use crate::{Clocks, date};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Mutex, PoisonError};
use std::thread;
use std::time::{Duration, Instant, SystemTime};

#[cfg(test)]
mod tests;

/// File-time ticks of `span`.
fn ticks(span: Duration) -> u64 {
    u64::try_from(span.as_nanos() / 100).unwrap_or(u64::MAX)
}

/// The clocks of the host the program runs on.
pub struct Host {
    start: Instant,
    clock: Mutex<Clock>,
    error: AtomicU32,
}

impl Host {
    /// A game clock at 0 now, whose frames move it at most `clamp`
    /// microseconds each.
    #[must_use]
    pub fn new(clamp: u64) -> Self {
        let mut clock = Clock::new(clamp, 0);
        clock.frame(0);
        Self {
            start: Instant::now(),
            clock: Mutex::new(clock),
            error: AtomicU32::new(0),
        }
    }

    /// The last error kept.
    #[must_use]
    pub fn last_error(&self) -> u32 {
        self.error.load(Ordering::Relaxed)
    }
}

impl Clocks for Host {
    /// The game clock, a frame moved at the host's instant first.
    fn micros(&self) -> u64 {
        let host = u64::try_from(self.start.elapsed().as_micros()).unwrap_or(u64::MAX);
        let mut clock = self.clock.lock().unwrap_or_else(PoisonError::into_inner);
        clock.frame(host);
        clock.micros()
    }

    fn wall(&self) -> u64 {
        match SystemTime::now().duration_since(SystemTime::UNIX_EPOCH) {
            Ok(after) => date::UNIX_EPOCH.saturating_add(ticks(after)),
            Err(before) => date::UNIX_EPOCH.saturating_sub(ticks(before.duration())),
        }
    }

    fn sleep(&self, millis: u32) {
        thread::sleep(Duration::from_millis(u64::from(millis)));
    }

    fn fail(&self, code: u32) {
        self.error.store(code, Ordering::Relaxed);
    }
}
