// SPDX-License-Identifier: MIT
//! The same three calls with real pads behind them. [`PadFront`] answers from a
//! [`Pads`], which is the only part that knows where a pad comes from: the page's
//! script, `winmm` or `gilrs`.
//!
//! A pad only adds. With none there, the count is 0, asking its capabilities is
//! refused and reading it says it is unplugged, nothing is written and nothing stops,
//! so a program with no pad carries on with its keys.
//!
//! Both records hold whole 32-bit words and no pointer, so a [`Record`] is enough
//! whatever a pointer is on the target. `JOYCAPSA`'s names sit at the front of it as
//! bytes, and stay 0: no [`Pads`] hands one over.
//!
//! The numbers are mingw-w64's `joystickapi.h`.

use core::fmt;

use crate::table::Row;
use crate::{Back, PARMS};

/// `JOYERR_NOERROR`.
const GOOD: u32 = 0;
/// `JOYERR_UNPLUGGED`: `JOYERR_BASE`, 160, plus 7.
const UNPLUGGED: u32 = 167;

/// `sizeof(JOYCAPSA)`, in bytes and in words.
const CAPS_SIZE: u32 = 404;
const CAPS_WORDS: usize = 101;
/// Where each axis's span starts in `JOYCAPSA`, in words, in the order X, Y, Z, R, U, V.
const CAPS_SPANS: [usize; 6] = [9, 11, 13, 18, 20, 22];
const CAPS_BUTTONS: usize = 15;
const CAPS_CAPS: usize = 24;
const CAPS_MAX_AXES: usize = 25;
const CAPS_AXES: usize = 26;
const CAPS_MAX_BUTTONS: usize = 27;
/// The words of `JOYCAPSA` a line shows: the first span through `wMaxButtons`.
const CAPS_SHOWN: core::ops::Range<usize> = 9..28;

/// `sizeof(JOYINFOEX)`, in bytes.
const POSITION_SIZE: u32 = 0x34;
const POSITION_SIZE_AT: usize = 0;
const POSITION_FLAGS_AT: usize = 1;
/// `dwXpos`; the five other axes follow it, in the order [`Reading::axes`] holds them.
const POSITION_AXES_AT: usize = 2;
const POSITION_BUTTONS_AT: usize = 8;
const POSITION_PRESSED_AT: usize = 9;
const POSITION_POV_AT: usize = 10;

/// `JOY_RETURN*`, one for each axis, in the order [`Reading::axes`] holds them.
const RETURN_AXES: [u32; 6] = [0x1, 0x2, 0x4, 0x8, 0x10, 0x20];
const RETURN_POV: u32 = 0x40;
const RETURN_BUTTONS: u32 = 0x80;
const RETURN_CENTERED: u32 = 0x400;
/// What a read must ask for: the first two axes, the buttons, and a centred stick.
const READ_ALWAYS: u32 = RETURN_AXES[0] | RETURN_AXES[1] | RETURN_BUTTONS | RETURN_CENTERED;
/// What a read may add, by what the capabilities said is there.
const READ_MAY_ADD: u32 =
    RETURN_AXES[2] | RETURN_AXES[3] | RETURN_AXES[4] | RETURN_AXES[5] | RETURN_POV;

/// `JOYCAPS_*`, the bits of [`Shape::caps`].
pub mod has {
    pub const Z: u32 = 0x1;
    pub const R: u32 = 0x2;
    pub const U: u32 = 0x4;
    pub const V: u32 = 0x8;
    pub const POV: u32 = 0x10;
    pub const POV_FOUR_WAYS: u32 = 0x20;
}

/// `dwPOV` for each way the hat can point, and for none.
pub mod pov {
    pub const FORWARD: u32 = 0;
    pub const RIGHT: u32 = 9000;
    pub const BACK: u32 = 18000;
    pub const LEFT: u32 = 27000;
    pub const CENTRED: u32 = 0xffff;
}

/// How many axes `JOYCAPSA` has room for, and how many buttons `dwButtons` has bits for.
const MAX_AXES: u32 = 6;
const MAX_BUTTONS: u32 = 32;
/// The middle of [`Span::FULL`], where [`stick`] puts a stick at rest.
pub const CENTRE: u32 = 32768;

/// One axis's lowest and highest number.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Span {
    pub min: u32,
    pub max: u32,
}

impl Span {
    /// The span of a back that scales its own axes.
    pub const FULL: Self = Self { min: 0, max: 65535 };

    /// A program may divide by a span's size, so one with no size is never handed on,
    /// whatever a back said: it reads as [`Span::FULL`].
    #[must_use]
    const fn with_size(self) -> Self {
        if self.max > self.min {
            self
        } else {
            Self::FULL
        }
    }
}

/// What a pad has, whatever it is reading now.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Shape {
    /// X, Y, Z, R, U, V, an absent axis's included.
    pub spans: [Span; 6],
    /// `wCaps`, by [`has`].
    pub caps: u32,
    /// How many axes it has, and how many buttons.
    pub axes: u32,
    pub buttons: u32,
}

/// What a pad reads now.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Reading {
    /// X, Y, Z, R, U, V.
    pub axes: [u32; 6],
    /// One bit a button.
    pub buttons: u32,
    /// `dwButtonNumber`.
    pub pressed: u32,
    /// `dwPOV`, by [`pov`].
    pub pov: u32,
}

/// Where a target's pads come from.
pub trait Pads {
    /// `joyGetNumDevs`: the slots a program should try, some perhaps empty.
    fn devices(&self) -> u32;
    /// What is in a slot; none if nothing is.
    fn shape(&self, device: u32) -> Option<Shape>;
    /// What a slot reads now; none if nothing is there.
    fn reading(&self, device: u32) -> Option<Reading>;
}

/// The record a call was handed, as its 32-bit words, whatever a pointer is on this
/// target.
pub trait Record {
    /// The word at `index`.
    fn word(&self, index: usize) -> u32;
    /// Writes the word at `index`.
    fn set_word(&mut self, index: usize, value: u32);
}

/// A stick from -1 to 1 as a number in [`Span::FULL`]: the same sum as `stick` in
/// `page/gamepads.mjs`, which rounds a half up, so a stick reads the same through
/// either. A number that is no number reads as a stick at rest.
#[must_use]
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
pub fn stick(value: f32) -> u32 {
    if !value.is_finite() {
        return CENTRE;
    }
    let in_range = f64::from(value.clamp(-1.0, 1.0));
    ((in_range + 1.0) * 32767.5 + 0.5) as u32
}

/// Four directions as a hat, each of which may be down. A diagonal has no four-way
/// answer, so it reads as the first of up, right, down, left, as in `page/gamepads.mjs`.
#[must_use]
#[allow(clippy::fn_params_excessive_bools)]
pub fn hat(up: bool, right: bool, down: bool, left: bool) -> u32 {
    if up {
        pov::FORWARD
    } else if right {
        pov::RIGHT
    } else if down {
        pov::BACK
    } else if left {
        pov::LEFT
    } else {
        pov::CENTRED
    }
}

/// A record's numbers on a line: comma-joined, each as any other number is shown.
struct Words<'a>(&'a [u32]);

impl fmt::Display for Words<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (at, word) in self.0.iter().enumerate() {
            if at > 0 {
                f.write_str(",")?;
            }
            fmt::Display::fmt(&oxide_trace::__shown!(*word), f)?;
        }
        Ok(())
    }
}

/// One file's joystick calls with real pads: its row, its target, and its pads.
pub struct PadFront<B, P> {
    file: &'static Row,
    back: B,
    pads: P,
}

impl<B: Back, P: Pads> PadFront<B, P> {
    pub const fn new(file: &'static Row, back: B, pads: P) -> Self {
        Self { file, back, pads }
    }

    /// The pads, for whoever feeds them.
    pub const fn pads(&self) -> &P {
        &self.pads
    }

    /// `joyGetNumDevs`.
    #[must_use]
    pub fn devices(&self) -> u32 {
        let count = self.pads.devices();
        self.back.quiet(|| {
            log::debug!(
                c = self.file.calls.devices,
                v:% = oxide_trace::__shown!(count),
                by = "joystick"; "imp.call"
            );
        });
        count
    }

    /// `joyGetDevCapsA`. No pad is `JOYERR_PARMS` for any size, as a refusal never reads
    /// the record. A pad and a size not `JOYCAPSA`'s is a stop: there is no record to
    /// fill. Otherwise every word is written, the unnamed ones 0.
    #[must_use]
    pub fn caps(&self, device: u32, size: u32, record: &mut impl Record) -> u32 {
        let Some(shape) = self.pads.shape(device) else {
            self.back.quiet(|| {
                log::debug!(
                    c = self.file.calls.caps,
                    a:% = Words(&[device, size]),
                    v:% = oxide_trace::__shown!(PARMS),
                    by = "joystick"; "imp.call"
                );
            });
            return PARMS;
        };
        if size != CAPS_SIZE {
            self.stop(self.file.calls.caps, &[device, size]);
        }
        let mut words = [0; CAPS_WORDS];
        for (at, span) in CAPS_SPANS.into_iter().zip(shape.spans) {
            let span = span.with_size();
            words[at] = span.min;
            words[at + 1] = span.max;
        }
        words[CAPS_BUTTONS] = shape.buttons;
        words[CAPS_CAPS] = shape.caps;
        words[CAPS_MAX_AXES] = MAX_AXES;
        words[CAPS_AXES] = shape.axes;
        words[CAPS_MAX_BUTTONS] = MAX_BUTTONS;
        for (at, word) in words.into_iter().enumerate() {
            record.set_word(at, word);
        }
        self.back.quiet(|| {
            log::debug!(
                c = self.file.calls.caps,
                a:% = Words(&[device, size]),
                v:% = oxide_trace::__shown!(GOOD),
                o:% = Words(&words[CAPS_SHOWN]),
                by = "joystick"; "imp.call"
            );
        });
        GOOD
    }

    /// `joyGetPosEx`. Flags 0 asks only whether a pad is there, and writes nothing.
    /// [`READ_ALWAYS`] plus any of [`READ_MAY_ADD`] writes what was asked and no more.
    /// A size not `JOYINFOEX`'s, or any other flags, is a stop: an answer would be a guess.
    #[must_use]
    pub fn position(&self, device: u32, record: &mut impl Record) -> u32 {
        let size = record.word(POSITION_SIZE_AT);
        let flags = record.word(POSITION_FLAGS_AT);
        let is_read = flags & !READ_MAY_ADD == READ_ALWAYS;
        if size != POSITION_SIZE || !(flags == 0 || is_read) {
            self.stop(self.file.calls.position, &[device, size, flags]);
        }
        let mut written = [0; 9];
        let mut count = 0;
        let answer = if is_read {
            self.read(device, flags, record, &mut written, &mut count)
        } else if self.pads.reading(device).is_some() {
            GOOD
        } else {
            UNPLUGGED
        };
        self.back.quiet(|| {
            log::debug!(
                c = self.file.calls.position,
                a:% = Words(&[device, flags]),
                v:% = oxide_trace::__shown!(answer),
                o:% = Words(&written[..count]),
                by = "joystick"; "imp.call"
            );
        });
        answer
    }

    /// The fields `flags` asks for, each axis held to its span. A pad pulled between the
    /// two questions is simply unplugged. `written` keeps what went in, for the line.
    fn read(
        &self,
        device: u32,
        flags: u32,
        record: &mut impl Record,
        written: &mut [u32; 9],
        count: &mut usize,
    ) -> u32 {
        let Some((shape, reading)) = self
            .pads
            .shape(device)
            .and_then(|shape| Some((shape, self.pads.reading(device)?)))
        else {
            return UNPLUGGED;
        };
        let mut put = |at: usize, value: u32| {
            record.set_word(at, value);
            written[*count] = value;
            *count += 1;
        };
        for (axis, asked) in RETURN_AXES.into_iter().enumerate() {
            if flags & asked != 0 {
                let span = shape.spans[axis].with_size();
                put(
                    POSITION_AXES_AT + axis,
                    reading.axes[axis].clamp(span.min, span.max),
                );
            }
        }
        put(POSITION_BUTTONS_AT, reading.buttons);
        put(POSITION_PRESSED_AT, reading.pressed);
        if flags & RETURN_POV != 0 {
            put(POSITION_POV_AT, reading.pov);
        }
        GOOD
    }

    /// One `joy.stop` line with the call's numbers, then the end of the program.
    fn stop(&self, name: &'static str, numbers: &[u32]) -> ! {
        log::error!(c = name, a:% = Words(numbers), by = "joystick"; "joy.stop");
        self.back.stop(name)
    }
}
