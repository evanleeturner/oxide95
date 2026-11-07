// SPDX-License-Identifier: MIT
//! What every check shares: the logger that catches each thin call's line, the two kinds of
//! red, and the running of a thin call beside its hand-declared twin.
//!
//! The two reds are the point. `red thin` says the hand-declared call and the thin call
//! disagree, or the thin call's line is wrong. `red control` says the hand-declared call itself
//! missed its worked value, or neither saw what was pressed, so the thin call is not the one to
//! blame.
use core::fmt::Debug;
use std::collections::BTreeSet;
use std::ffi::CStr;
use std::process::ExitCode;
use std::sync::Mutex;

use log::kv::{Error, Key, Value, VisitSource};
use log::{LevelFilter, Log, Metadata, Record};

use crate::hand;

/// The `from` handed to each thin call: only shown on its line.
pub const FROM: u32 = 0x1234;
/// A window handle nothing here ever holds.
pub const NO_WINDOW: u32 = 1;
/// `ERROR_INVALID_WINDOW_HANDLE`.
pub const BAD_WINDOW: u32 = 1400;
/// The last error the logger leaves behind: a front that lost `kept` hands this to its caller.
pub const LOGGER_ERROR: u32 = 0x7777;
/// The last error each call of a pair starts from.
pub const MARKED_ERROR: u32 = 0x5a5a;
const CONTROL_SENTENCE: &str =
    "The hand-declared call missed its worked value too. Wine or this computer, not the thin call.";

/// A wrong answer put in on purpose, to show its check can go red.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Plant {
    None,
    SwapXy,
    NoLine,
    WrongBy,
    NoKept,
    KeepMessage,
    NoTimer,
    KeyOffByOne,
}

impl Plant {
    /// The plant the driver names; none for a name not here.
    pub fn named(name: &str) -> Option<Self> {
        Some(match name {
            "none" => Self::None,
            "swap-xy" => Self::SwapXy,
            "no-line" => Self::NoLine,
            "wrong-by" => Self::WrongBy,
            "no-kept" => Self::NoKept,
            "keep-message" => Self::KeepMessage,
            "no-timer" => Self::NoTimer,
            "key-off-by-one" => Self::KeyOffByOne,
            _ => return None,
        })
    }
}

/// One `imp.call` line's fields, each as the line shows it; empty when the line has none.
#[derive(Default)]
pub struct Line {
    c: String,
    a: String,
    s: String,
    with: String,
    o: String,
    v: String,
    from: String,
    by: String,
}

impl VisitSource<'_> for Line {
    fn visit_pair(&mut self, key: Key<'_>, value: Value<'_>) -> Result<(), Error> {
        let text = value.to_string();
        match key.as_str() {
            "c" => self.c = text,
            "a" => self.a = text,
            "s" => self.s = text,
            "with" => self.with = text,
            "o" => self.o = text,
            "v" => self.v = text,
            "from" => self.from = text,
            "by" => self.by = text,
            _ => {}
        }
        Ok(())
    }
}

static LINES: Mutex<Vec<Line>> = Mutex::new(Vec::new());

struct Catcher;

impl Log for Catcher {
    fn enabled(&self, _: &Metadata<'_>) -> bool {
        true
    }

    fn log(&self, record: &Record<'_>) {
        let mut line = Line::default();
        let _ = record.key_values().visit(&mut line);
        println!(
            "{} c={} a={} s={} with={} o={} v={} from={} by={}",
            record.args(),
            line.c,
            line.a,
            line.s,
            line.with,
            line.o,
            line.v,
            line.from,
            line.by
        );
        LINES.lock().expect("one thread writes").push(line);
        hand::set_last_error(LOGGER_ERROR);
    }

    fn flush(&self) {}
}

/// Catches every line from here on, at every level a thin call writes.
pub fn install_logger() {
    log::set_logger(&Catcher).expect("one logger");
    log::set_max_level(LevelFilter::Debug);
}

/// The lines written since the last take, oldest first.
pub fn take_lines() -> Vec<Line> {
    std::mem::take(&mut *LINES.lock().expect("one thread writes"))
}

/// Numbers as a line shows them: hex, with commas.
pub fn hex(numbers: &[u32]) -> String {
    numbers
        .iter()
        .map(|number| format!("{number:x}"))
        .collect::<Vec<_>>()
        .join(",")
}

/// A point's or a rectangle's fields, as a thin call's `u32` line shows them.
pub fn words<const N: usize>(fields: [i32; N]) -> [u32; N] {
    fields.map(i32::cast_unsigned)
}

/// What a value lends a thin call to read.
pub fn address<T>(value: &T) -> u32 {
    let at = core::ptr::from_ref(value).expose_provenance();
    u32::try_from(at).expect("a 32-bit program")
}

/// What a value lends a thin call to write.
pub fn spot<T>(value: &mut T) -> u32 {
    let at = core::ptr::from_mut(value).expose_provenance();
    u32::try_from(at).expect("a 32-bit program")
}

/// A text's address.
pub fn text(value: &CStr) -> u32 {
    u32::try_from(value.as_ptr().expose_provenance()).expect("a 32-bit program")
}

/// A procedure's address, as a class record or a timer takes it.
pub fn code(procedure: *const ()) -> u32 {
    u32::try_from(procedure.expose_provenance()).expect("a 32-bit program")
}

/// A line's fields a check knows before the call. `v`, if not named, must show the answer.
#[derive(Default)]
pub struct Want {
    a: Option<String>,
    s: Option<String>,
    with: Option<String>,
    o: Option<String>,
    v: Option<String>,
}

impl Want {
    /// The line's `a`.
    pub fn a(numbers: &[u32]) -> Self {
        Self {
            a: Some(hex(numbers)),
            ..Self::default()
        }
    }

    /// The line's `s`, as text.
    pub fn s(mut self, shown: &str) -> Self {
        self.s = Some(shown.to_owned());
        self
    }

    /// The line's `with`, as text.
    pub fn with(mut self, shown: &str) -> Self {
        self.with = Some(shown.to_owned());
        self
    }

    /// The line's `o`.
    pub fn o(mut self, numbers: &[u32]) -> Self {
        self.o = Some(hex(numbers));
        self
    }

    /// A line with no `v`, for a call with no answer.
    pub fn no_v(mut self) -> Self {
        self.v = Some(String::new());
        self
    }
}

/// How a pair's answers must compare.
#[derive(Clone, Copy)]
pub enum Same {
    /// The same number.
    Answer,
    /// Each its own handle or atom, as each asked of its own twin.
    BothNonzero,
}

/// One check's running tally.
pub struct Bench {
    check: &'static str,
    by: &'static str,
    sentence: &'static str,
    pub plant: Plant,
    names: &'static [&'static str],
    seen: BTreeSet<&'static str>,
    thin_reds: u32,
    control_reds: u32,
    /// The last errors the last call or pair left: the thin call's, then the hand-declared's.
    pub errors: [u32; 2],
}

impl Bench {
    pub fn new(
        check: &'static str,
        by: &'static str,
        sentence: &'static str,
        plant: Plant,
        names: &'static [&'static str],
    ) -> Self {
        Self {
            check,
            by,
            sentence,
            plant,
            names,
            seen: BTreeSet::new(),
            thin_reds: 0,
            control_reds: 0,
            errors: [0; 2],
        }
    }

    /// The thin call is the one to blame.
    pub fn thin_red(&mut self, name: &str, detail: &str) {
        self.thin_reds += 1;
        println!("red thin {} {name} {detail}", self.check);
    }

    /// Wine or the computer is the one to blame.
    pub fn control_red(&mut self, name: &str, detail: &str) {
        self.control_reds += 1;
        println!("red control {} {name} {detail}", self.check);
    }

    /// The thin call's answer against the hand-declared one's.
    pub fn agree<T: PartialEq + Debug + Copy>(&mut self, name: &str, what: &str, thin: T, hand: T) {
        if thin != hand {
            self.thin_red(name, &format!("{what}: thin {thin:x?}, hand {hand:x?}"));
        }
    }

    /// A hand-declared answer against its value worked before the run.
    pub fn worked<T: PartialEq + Debug + Copy>(
        &mut self,
        name: &str,
        what: &str,
        hand: T,
        value: T,
    ) {
        if hand != value {
            self.control_red(name, &format!("{what}: hand {hand:x?}, worked {value:x?}"));
        }
    }

    /// A thin call alone: the last error it leaves, and its one line against what is wanted.
    pub fn alone(&mut self, name: &'static str, want: &Want, thin: impl FnOnce() -> u32) -> u32 {
        take_lines();
        hand::set_last_error(MARKED_ERROR);
        let answer = thin();
        self.errors[0] = hand::last_error();
        let lines = take_lines();
        self.read_line(name, &lines, want, answer);
        answer
    }

    /// A thin call and its hand-declared twin: answers, last errors and the thin call's line.
    /// Answers `[thin, hand]`.
    pub fn beside(
        &mut self,
        name: &'static str,
        same: Same,
        want: &Want,
        thin: impl FnOnce() -> u32,
        hand: impl FnOnce() -> u32,
    ) -> [u32; 2] {
        let thin_answer = self.alone(name, want, thin);
        hand::set_last_error(MARKED_ERROR);
        let hand_answer = hand();
        self.errors[1] = hand::last_error();
        println!(
            "row {} {name} thin={thin_answer:x} hand={hand_answer:x} errors={:x},{:x}",
            self.check, self.errors[0], self.errors[1]
        );
        match same {
            Same::Answer => self.agree(name, "answer", thin_answer, hand_answer),
            Same::BothNonzero => {
                if (thin_answer == 0) != (hand_answer == 0) {
                    self.thin_red(name, &format!("thin {thin_answer:x}, hand {hand_answer:x}"));
                }
            }
        }
        self.agree(name, "last error", self.errors[0], self.errors[1]);
        [thin_answer, hand_answer]
    }

    /// A call's one line, against what is wanted of it.
    fn read_line(&mut self, name: &'static str, lines: &[Line], want: &Want, answer: u32) {
        let [line] = lines else {
            self.thin_red(name, &format!("{} lines for one call", lines.len()));
            return;
        };
        self.seen.insert(name);
        let by = if self.plant == Plant::WrongBy && name == "FindWindowA" {
            "draw"
        } else {
            line.by.as_str()
        };
        let shown = format!("{answer:x}");
        let fields = [
            ("c", line.c.as_str(), Some(format!("USER32!{name}"))),
            ("by", by, Some(self.by.to_owned())),
            ("from", line.from.as_str(), Some(format!("{FROM:x}"))),
            ("v", line.v.as_str(), Some(want.v.clone().unwrap_or(shown))),
            ("a", line.a.as_str(), want.a.clone()),
            ("s", line.s.as_str(), want.s.clone()),
            ("with", line.with.as_str(), want.with.clone()),
            ("o", line.o.as_str(), want.o.clone()),
        ];
        for (field, got, wanted) in fields {
            if let Some(wanted) = wanted
                && got != wanted
            {
                self.thin_red(name, &format!("line's {field}: {got}, worked {wanted}"));
            }
        }
    }

    /// Says every name was read, and the check's verdict.
    pub fn finish(mut self) -> ExitCode {
        let names = self.names;
        for name in names {
            if !self.seen.contains(name) {
                self.thin_red(name, "no line read");
            }
        }
        println!(
            "{} names {} of {}",
            self.check,
            self.seen.len(),
            names.len()
        );
        if self.thin_reds > 0 {
            println!("{} red {}", self.check, self.sentence);
        }
        if self.control_reds > 0 {
            println!("{} red {CONTROL_SENTENCE}", self.check);
        }
        if self.thin_reds + self.control_reds == 0 {
            println!("{} pass", self.check);
            ExitCode::SUCCESS
        } else {
            ExitCode::FAILURE
        }
    }
}
