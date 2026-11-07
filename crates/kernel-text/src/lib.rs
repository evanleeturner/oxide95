// SPDX-License-Identifier: MIT
//! The Kernel's text: the KERNEL32 names a program
//! asks of its text, written once. The code pages and the locale a call
//! resolves to are the game's row, never the host's; the code pages' tables
//! are the back's, behind [`Locale`]: on the page, Linux, macOS and Windows
//! the one portable back, [`Portable`], with the tables in Rust and no OS
//! call; under Wine the original's calls, logged in the swap.
//!
//! Each file the seam serves is a [`table::Row`], kept in its game's rows
//! crate. [`Front`] answers in the file's numbers and writes one `imp.call`
//! line a call: `c` the row's name, `a` the call's numbers, `s` the text
//! it was given, `with` a second text, `v` its answer, `e` the error left.
//!
//! The crate names no game and nothing of any one target, so it builds for
//! `wasm32-unknown-unknown`; a game's facts live in its rows crate.
//! `GetUserDefaultLCID`, `IsValidCodePage` and `IsValidLocale`, imported
//! by a game no row serves yet, have no front: each comes with its first
//! measured caller.
//!
//! The walls, named: a code page with lead bytes; `WideCharToMultiByte`'s
//! best-fit answers (a unit the page lacks answers the default character,
//! where Wine answers U+0100 with `A`);
//! Windows' sort weights past the first letter, so the sort is the units'
//! order, case folded for `NORM_IGNORECASE`; `LCMapString`'s sort key;
//! `GetStringType`'s second and third classes. Each answers a logged fail.
#![cfg_attr(not(test), no_std)]

pub mod pages;
pub mod table;
#[cfg(test)]
mod tests;

use core::cmp::Ordering;
use core::fmt;
use oxide_trace::Args;
use table::{CodePage, Row};

/// `MB_PRECOMPOSED`: a single-byte page has nothing to compose.
const PRECOMPOSED: u32 = 1;
/// `NORM_IGNORECASE`.
const IGNORE_CASE: u32 = 1;
/// `LCMAP_LOWERCASE` and `LCMAP_UPPERCASE`.
const LOWERCASE: u32 = 0x100;
const UPPERCASE: u32 = 0x200;
/// `CT_CTYPE1`: the one class `GetStringType` answers.
const CLASS_ONE: u32 = 1;
/// `LOCALE_NOUSEROVERRIDE`: the row holds no user's override to skip.
const NO_OVERRIDE: u32 = 0x8000_0000;

/// Where one program's code pages live, and where a refusal's error goes:
/// the seam between the calls and a target.
pub trait Locale {
    /// The single-byte code page `number`, `None` for one this back lacks.
    fn code_page(&self, number: u32) -> Option<&CodePage>;
    /// Keeps `code` as the last error the file reads.
    fn fail(&self, code: u32);
    /// Writes one line around whatever the target must keep across it.
    fn quiet(&self, line: impl FnOnce()) {
        line();
    }
}

/// A borrowed back: each of a program's files keeps its own [`Front`] and
/// all share the one back.
impl<L: Locale> Locale for &L {
    fn code_page(&self, number: u32) -> Option<&CodePage> {
        (**self).code_page(number)
    }

    fn fail(&self, code: u32) {
        (**self).fail(code);
    }

    fn quiet(&self, line: impl FnOnce()) {
        (**self).quiet(line);
    }
}

/// The portable back: the code pages in [`pages`], a refusal's error
/// handed to the process that keeps it.
pub struct Portable {
    fail: fn(u32),
}

impl Portable {
    #[must_use]
    pub const fn new(fail: fn(u32)) -> Self {
        Self { fail }
    }
}

impl Locale for Portable {
    fn code_page(&self, number: u32) -> Option<&CodePage> {
        pages::ALL.iter().find(|page| page.number == number)
    }

    fn fail(&self, code: u32) {
        (self.fail)(code);
    }
}

/// `GetCPInfo`'s record.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Info {
    pub max_char_size: u32,
    pub default: [u8; 2],
    pub lead_bytes: [u8; 12],
}

/// Bytes as the line shows them: a printable byte as its character, any
/// other `\xNN`.
pub struct Bytes<'a>(pub &'a [u8]);

impl fmt::Display for Bytes<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.iter().try_for_each(|byte| match byte {
            b' '..=b'~' => fmt::Write::write_char(f, char::from(*byte)),
            _ => write!(f, "\\x{byte:02x}"),
        })
    }
}

/// UTF-16 units as the line shows them: a printable ASCII unit as its
/// character, any other `\uNNNN`.
pub struct Units<'a>(pub &'a [u16]);

impl fmt::Display for Units<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0
            .iter()
            .try_for_each(|unit| match u8::try_from(*unit) {
                Ok(byte @ b' '..=b'~') => fmt::Write::write_char(f, char::from(byte)),
                _ => write!(f, "\\u{unit:04x}"),
            })
    }
}

/// `unit` in one case, kept where the case has no single unit.
fn cased(unit: u16, upper: bool) -> u16 {
    let Some(ch) = char::from_u32(u32::from(unit)) else {
        return unit;
    };
    let one = |each: &mut dyn Iterator<Item = char>| match (each.next(), each.next()) {
        (Some(ch), None) => u16::try_from(u32::from(ch)).unwrap_or(unit),
        _ => unit,
    };
    if upper {
        one(&mut ch.to_uppercase())
    } else {
        one(&mut ch.to_lowercase())
    }
}

/// `CT_CTYPE1`'s bits for `unit`: ASCII's as Windows answers them, any
/// other unit's from its Unicode properties.
fn class(unit: u16) -> u16 {
    let Some(ch) = char::from_u32(u32::from(unit)) else {
        return 0;
    };
    let bits = [
        (ch.is_uppercase(), 0x001),
        (ch.is_lowercase(), 0x002),
        (ch.is_ascii_digit(), 0x004),
        (ch.is_whitespace(), 0x008),
        (
            !ch.is_alphanumeric() && !ch.is_whitespace() && !ch.is_control(),
            0x010,
        ),
        (ch.is_control(), 0x020),
        (matches!(ch, ' ' | '\t'), 0x040),
        (ch.is_ascii_hexdigit(), 0x080),
        (ch.is_alphabetic(), 0x100),
    ];
    bits.iter()
        .filter(|(set, _)| *set)
        .fold(0, |all, (_, bit)| all | bit)
}

/// Writes `count` items of `answer` into `out`, or only counts them when
/// no buffer was given; `None` for a buffer too short.
fn written<T>(out: Option<&mut [T]>, count: usize, answer: impl Iterator<Item = T>) -> Option<i32> {
    if let Some(out) = out {
        if out.len() < count {
            return None;
        }
        out.iter_mut()
            .zip(answer)
            .for_each(|(slot, item)| *slot = item);
    }
    Some(i32::try_from(count).unwrap_or(i32::MAX))
}

/// One file's text calls: its row, and its back.
pub struct Front<L> {
    file: &'static Row,
    locale: L,
}

impl<L: Locale> Front<L> {
    pub const fn new(file: &'static Row, locale: L) -> Self {
        Self { file, locale }
    }

    /// The code page `number` resolves to: the row's for 0, 1 and 3.
    fn page(&self, number: u32) -> Option<&CodePage> {
        let number = match number {
            0 | 3 => self.file.ansi,
            1 => self.file.oem,
            other => other,
        };
        self.locale.code_page(number)
    }

    /// Whether the row serves `locale`: its own number, 0, or a default.
    const fn serves(&self, locale: u32) -> bool {
        matches!(locale, 0 | 0x400 | 0x800) || locale == self.file.locale
    }

    /// `value` as answered: 0 leaves `error`, kept and returned for the
    /// line; another value leaves none.
    fn refused(&self, value: i32, error: u32) -> u32 {
        if value != 0 {
            return 0;
        }
        self.locale.fail(error);
        error
    }

    /// `GetACP`: the row's code page.
    #[must_use]
    pub fn ansi_code_page(&self) -> u32 {
        let value = self.file.ansi;
        self.locale.quiet(|| {
            log::debug!(c = self.file.calls.ansi_code_page, v:% = oxide_trace::__shown!(value),
                by = "kernel-text"; "imp.call");
        });
        value
    }

    /// `GetOEMCP`: the row's code page.
    #[must_use]
    pub fn oem_code_page(&self) -> u32 {
        let value = self.file.oem;
        self.locale.quiet(|| {
            log::debug!(c = self.file.calls.oem_code_page, v:% = oxide_trace::__shown!(value),
                by = "kernel-text"; "imp.call");
        });
        value
    }

    /// `GetCPInfo`: one byte a character, the page's default and no lead
    /// bytes; `None` and the row's error for a page the back lacks.
    pub fn code_page_info(&self, code_page: u32) -> Option<Info> {
        let info = self.page(code_page).map(|page| Info {
            max_char_size: 1,
            default: [page.default, 0],
            lead_bytes: [0; 12],
        });
        let value = i32::from(info.is_some());
        let error = self.refused(value, self.file.errors.invalid_parameter);
        let arguments = [oxide_trace::__shown!(code_page)];
        self.locale.quiet(|| {
            log::debug!(
                c = self.file.calls.code_page_info, a:% = Args(arguments),
                v:% = oxide_trace::__shown!(value), e:% = oxide_trace::__shown!(error),
                by = "kernel-text"; "imp.call"
            );
        });
        info
    }

    /// `MultiByteToWideChar`: each byte's unit, NULs included, into `out`,
    /// or the count alone for no buffer. Flags 0 and `MB_PRECOMPOSED`.
    pub fn to_wide(
        &self,
        code_page: u32,
        flags: u32,
        bytes: &[u8],
        out: Option<&mut [u16]>,
    ) -> i32 {
        let size = out.as_ref().map_or(0, |out| out.len());
        let (value, missing) = match self.page(code_page) {
            _ if flags & !PRECOMPOSED != 0 => (0, self.file.errors.invalid_flags),
            None => (0, self.file.errors.invalid_parameter),
            Some(page) => {
                let answer = bytes.iter().map(|byte| page.unit(*byte));
                let value = written(out, bytes.len(), answer);
                (value.unwrap_or(0), self.file.errors.insufficient_buffer)
            }
        };
        let error = self.refused(value, missing);
        let size = u32::try_from(size).unwrap_or(u32::MAX);
        let arguments = [
            oxide_trace::__shown!(code_page),
            oxide_trace::__shown!(flags),
            oxide_trace::__shown!(size),
        ];
        self.locale.quiet(|| {
            log::debug!(
                c = self.file.calls.to_wide, a:% = Args(arguments), s:% = Bytes(bytes),
                v:% = oxide_trace::__shown!(value), e:% = oxide_trace::__shown!(error),
                by = "kernel-text"; "imp.call"
            );
        });
        value
    }

    /// `WideCharToMultiByte`: each unit's byte, NULs included, into `out`,
    /// or the count alone for no buffer; a unit the page lacks answers
    /// `default`, else the page's, and the second answer says one did.
    /// Flags 0 alone.
    pub fn to_bytes(
        &self,
        code_page: u32,
        flags: u32,
        units: &[u16],
        out: Option<&mut [u8]>,
        default: Option<u8>,
    ) -> (i32, bool) {
        let size = out.as_ref().map_or(0, |out| out.len());
        let mut defaulted = false;
        let (value, missing) = match self.page(code_page) {
            _ if flags != 0 => (0, self.file.errors.invalid_flags),
            None => (0, self.file.errors.invalid_parameter),
            Some(page) => {
                let fallback = default.unwrap_or(page.default);
                defaulted = units.iter().any(|unit| page.byte(*unit).is_none());
                let answer = units
                    .iter()
                    .map(|unit| page.byte(*unit).unwrap_or(fallback));
                let value = written(out, units.len(), answer);
                (value.unwrap_or(0), self.file.errors.insufficient_buffer)
            }
        };
        let error = self.refused(value, missing);
        let size = u32::try_from(size).unwrap_or(u32::MAX);
        let arguments = [
            oxide_trace::__shown!(code_page),
            oxide_trace::__shown!(flags),
            oxide_trace::__shown!(size),
        ];
        self.locale.quiet(|| {
            log::debug!(
                c = self.file.calls.to_bytes, a:% = Args(arguments), s:% = Units(units),
                v:% = oxide_trace::__shown!(value), e:% = oxide_trace::__shown!(error),
                by = "kernel-text"; "imp.call"
            );
        });
        (value, defaulted)
    }

    /// `GetStringTypeA`: `CT_CTYPE1`'s bits for each byte's unit in the
    /// row's code page, into `out`, one a byte.
    pub fn string_type(&self, locale: u32, kind: u32, bytes: &[u8], out: &mut [u16]) -> i32 {
        let page = self.page(0);
        let units = bytes
            .iter()
            .map(|byte| page.map_or(u16::from(*byte), |page| page.unit(*byte)));
        let value = self.classes(locale, kind, bytes.len(), units, out);
        self.typed(
            self.file.calls.string_type,
            [locale, kind],
            value,
            Bytes(bytes),
        );
        value.0
    }

    /// `GetStringTypeW`: `CT_CTYPE1`'s bits for each unit, into `out`.
    pub fn string_type_w(&self, kind: u32, units: &[u16], out: &mut [u16]) -> i32 {
        let value = self.classes(0, kind, units.len(), units.iter().copied(), out);
        self.typed(
            self.file.calls.string_type_w,
            [0, kind],
            value,
            Units(units),
        );
        value.0
    }

    /// The classes for `GetStringType`: the answer and the error left.
    fn classes(
        &self,
        locale: u32,
        kind: u32,
        count: usize,
        units: impl Iterator<Item = u16>,
        out: &mut [u16],
    ) -> (i32, u32) {
        let (value, missing) = if kind != CLASS_ONE {
            (0, self.file.errors.invalid_flags)
        } else if !self.serves(locale) {
            (0, self.file.errors.invalid_parameter)
        } else {
            let value = written(Some(out), count, units.map(class));
            (
                i32::from(value.is_some()),
                self.file.errors.insufficient_buffer,
            )
        };
        (value, self.refused(value, missing))
    }

    /// The `GetStringType` line.
    fn typed(
        &self,
        call: &str,
        numbers: [u32; 2],
        (value, error): (i32, u32),
        text: impl fmt::Display,
    ) {
        let arguments = [
            oxide_trace::__shown!(numbers[0]),
            oxide_trace::__shown!(numbers[1]),
        ];
        self.locale.quiet(|| {
            log::debug!(
                c = call, a:% = Args(arguments), s:% = text,
                v:% = oxide_trace::__shown!(value), e:% = oxide_trace::__shown!(error),
                by = "kernel-text"; "imp.call"
            );
        });
    }

    /// The case `LCMapString`'s flags ask, `None` for any other flag.
    const fn case(flags: u32) -> Option<bool> {
        match flags {
            LOWERCASE => Some(false),
            UPPERCASE => Some(true),
            _ => None,
        }
    }

    /// `LCMapStringA`: each byte in one case, through the row's code page;
    /// a byte whose other case the page lacks kept.
    pub fn map_string(&self, locale: u32, flags: u32, bytes: &[u8], out: Option<&mut [u8]>) -> i32 {
        let size = out.as_ref().map_or(0, |out| out.len());
        let (value, missing) = match (Self::case(flags), self.page(0)) {
            (None, _) => (0, self.file.errors.invalid_flags),
            (_, None) => (0, self.file.errors.invalid_parameter),
            _ if !self.serves(locale) => (0, self.file.errors.invalid_parameter),
            (Some(upper), Some(page)) => {
                let one = |byte: &u8| page.byte(cased(page.unit(*byte), upper)).unwrap_or(*byte);
                let value = written(out, bytes.len(), bytes.iter().map(one));
                (value.unwrap_or(0), self.file.errors.insufficient_buffer)
            }
        };
        let error = self.refused(value, missing);
        let size = u32::try_from(size).unwrap_or(u32::MAX);
        let arguments = [
            oxide_trace::__shown!(locale),
            oxide_trace::__shown!(flags),
            oxide_trace::__shown!(size),
        ];
        self.locale.quiet(|| {
            log::debug!(
                c = self.file.calls.map_string, a:% = Args(arguments), s:% = Bytes(bytes),
                v:% = oxide_trace::__shown!(value), e:% = oxide_trace::__shown!(error),
                by = "kernel-text"; "imp.call"
            );
        });
        value
    }

    /// `LCMapStringW`: each unit in one case; a unit whose other case is
    /// not one unit kept.
    pub fn map_string_w(
        &self,
        locale: u32,
        flags: u32,
        units: &[u16],
        out: Option<&mut [u16]>,
    ) -> i32 {
        let size = out.as_ref().map_or(0, |out| out.len());
        let (value, missing) = match Self::case(flags) {
            None => (0, self.file.errors.invalid_flags),
            _ if !self.serves(locale) => (0, self.file.errors.invalid_parameter),
            Some(upper) => {
                let value = written(
                    out,
                    units.len(),
                    units.iter().map(|unit| cased(*unit, upper)),
                );
                (value.unwrap_or(0), self.file.errors.insufficient_buffer)
            }
        };
        let error = self.refused(value, missing);
        let size = u32::try_from(size).unwrap_or(u32::MAX);
        let arguments = [
            oxide_trace::__shown!(locale),
            oxide_trace::__shown!(flags),
            oxide_trace::__shown!(size),
        ];
        self.locale.quiet(|| {
            log::debug!(
                c = self.file.calls.map_string_w, a:% = Args(arguments), s:% = Units(units),
                v:% = oxide_trace::__shown!(value), e:% = oxide_trace::__shown!(error),
                by = "kernel-text"; "imp.call"
            );
        });
        value
    }

    /// The sort's answer, 1 less, 2 equal, 3 greater, over two runs of
    /// units; 0 and the error for a flag other than `NORM_IGNORECASE` or a
    /// locale the row does not serve.
    fn order(
        &self,
        locale: u32,
        flags: u32,
        first: impl Iterator<Item = u16>,
        second: impl Iterator<Item = u16>,
    ) -> (i32, u32) {
        let key = |unit| {
            if flags == IGNORE_CASE {
                cased(unit, false)
            } else {
                unit
            }
        };
        let (value, missing) = if flags & !IGNORE_CASE != 0 {
            (0, self.file.errors.invalid_flags)
        } else if !self.serves(locale) {
            (0, self.file.errors.invalid_parameter)
        } else {
            let value = match first.map(key).cmp(second.map(key)) {
                Ordering::Less => 1,
                Ordering::Equal => 2,
                Ordering::Greater => 3,
            };
            (value, 0)
        };
        (value, self.refused(value, missing))
    }

    /// `CompareStringA`: the two texts as given, a count of -1 ending each
    /// before its NUL, through the row's code page.
    pub fn compare(&self, locale: u32, flags: u32, first: &[u8], second: &[u8]) -> i32 {
        let unit = |byte: &u8| {
            self.page(0)
                .map_or(u16::from(*byte), |page| page.unit(*byte))
        };
        let (value, error) = self.order(
            locale,
            flags,
            first.iter().map(unit),
            second.iter().map(unit),
        );
        let arguments = [oxide_trace::__shown!(locale), oxide_trace::__shown!(flags)];
        self.locale.quiet(|| {
            log::debug!(
                c = self.file.calls.compare, a:% = Args(arguments), s:% = Bytes(first), with:% = Bytes(second),
                v:% = oxide_trace::__shown!(value), e:% = oxide_trace::__shown!(error),
                by = "kernel-text"; "imp.call"
            );
        });
        value
    }

    /// `CompareStringW`: the two texts as given.
    pub fn compare_w(&self, locale: u32, flags: u32, first: &[u16], second: &[u16]) -> i32 {
        let (value, error) =
            self.order(locale, flags, first.iter().copied(), second.iter().copied());
        let arguments = [oxide_trace::__shown!(locale), oxide_trace::__shown!(flags)];
        self.locale.quiet(|| {
            log::debug!(
                c = self.file.calls.compare_w, a:% = Args(arguments), s:% = Units(first), with:% = Units(second),
                v:% = oxide_trace::__shown!(value), e:% = oxide_trace::__shown!(error),
                by = "kernel-text"; "imp.call"
            );
        });
        value
    }

    /// The row's text for `kind`, `LOCALE_NOUSEROVERRIDE` set aside.
    fn info(&self, locale: u32, kind: u32) -> Option<&'static str> {
        let kind = kind & !NO_OVERRIDE;
        let found = self.file.locale_info.iter().find(|(each, _)| *each == kind);
        found.filter(|_| self.serves(locale)).map(|(_, text)| *text)
    }

    /// `GetLocaleInfoA`: the row's text and its NUL, into `out`, or the
    /// count alone for no buffer.
    pub fn locale_info(&self, locale: u32, kind: u32, out: Option<&mut [u8]>) -> i32 {
        let text = self.info(locale, kind);
        let value = text.map_or(Some(0), |text| {
            written(out, text.len() + 1, text.bytes().chain([0]))
        });
        let error = self.refused(
            value.unwrap_or(0),
            if text.is_none() {
                self.file.errors.invalid_parameter
            } else {
                self.file.errors.insufficient_buffer
            },
        );
        let value = value.unwrap_or(0);
        let arguments = [oxide_trace::__shown!(locale), oxide_trace::__shown!(kind)];
        self.locale.quiet(|| {
            log::debug!(
                c = self.file.calls.locale_info, a:% = Args(arguments),
                v:% = oxide_trace::__shown!(value), e:% = oxide_trace::__shown!(error),
                by = "kernel-text"; "imp.call"
            );
        });
        value
    }

    /// `GetLocaleInfoW`: the row's text and its NUL, as units.
    pub fn locale_info_w(&self, locale: u32, kind: u32, out: Option<&mut [u16]>) -> i32 {
        let text = self.info(locale, kind);
        let value = text.map_or(Some(0), |text| {
            written(
                out,
                text.encode_utf16().count() + 1,
                text.encode_utf16().chain([0]),
            )
        });
        let error = self.refused(
            value.unwrap_or(0),
            if text.is_none() {
                self.file.errors.invalid_parameter
            } else {
                self.file.errors.insufficient_buffer
            },
        );
        let value = value.unwrap_or(0);
        let arguments = [oxide_trace::__shown!(locale), oxide_trace::__shown!(kind)];
        self.locale.quiet(|| {
            log::debug!(
                c = self.file.calls.locale_info_w, a:% = Args(arguments),
                v:% = oxide_trace::__shown!(value), e:% = oxide_trace::__shown!(error),
                by = "kernel-text"; "imp.call"
            );
        });
        value
    }
}
