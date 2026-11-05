// SPDX-License-Identifier: MIT
//! The wall clock's dates: 100 ns ticks since 1601-01-01 UTC, Windows'
//! file time, to the 8 words of a `SYSTEMTIME`. The civil date is counted
//! by 400-year eras, each 146,097 days, the year starting in March so a
//! leap day is the era year's last.

use crate::table::DATE_BYTES;

/// 1970-01-01 in file-time ticks.
pub const UNIX_EPOCH: u64 = 116_444_736_000_000_000;

const TICKS_PER_DAY: u64 = 864_000_000_000;

/// The days from 0000-03-01 to 1601-01-01.
const DAYS_TO_1601: i64 = 584_694;

/// A `SYSTEMTIME`: year, month, weekday (0 Sunday), day, hour, minute,
/// second, millisecond.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Date(pub [u16; 8]);

impl Date {
    /// The date at file-time `ticks`.
    #[must_use]
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_possible_wrap,
        clippy::cast_sign_loss,
        reason = "every part is in range of its word; days below 2^63 / 864e9"
    )]
    pub const fn of(ticks: u64) -> Self {
        let days = (ticks / TICKS_PER_DAY) as i64;
        let left = ticks % TICKS_PER_DAY / 10_000;
        let z = days + DAYS_TO_1601;
        let era = z / 146_097;
        let of_era = z - era * 146_097;
        let year_of_era = (of_era - of_era / 1460 + of_era / 36_524 - of_era / 146_096) / 365;
        let day_of_year = of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
        let march_month = (5 * day_of_year + 2) / 153;
        let day = day_of_year - (153 * march_month + 2) / 5 + 1;
        let month = if march_month < 10 {
            march_month + 3
        } else {
            march_month - 9
        };
        let year = year_of_era + era * 400 + if month <= 2 { 1 } else { 0 };
        // 1601-01-01 is a Monday.
        let weekday = (days + 1) % 7;
        Self([
            year as u16,
            month as u16,
            weekday as u16,
            day as u16,
            (left / 3_600_000) as u16,
            (left / 60_000 % 60) as u16,
            (left / 1000 % 60) as u16,
            (left % 1000) as u16,
        ])
    }

    /// The 8 words, little-endian, as Windows lays them.
    #[must_use]
    pub fn to_bytes(&self) -> [u8; DATE_BYTES] {
        let mut bytes = [0; DATE_BYTES];
        for (word, into) in self.0.iter().zip(bytes.chunks_exact_mut(2)) {
            into.copy_from_slice(&word.to_le_bytes());
        }
        bytes
    }
}
