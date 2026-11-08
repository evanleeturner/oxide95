// SPDX-License-Identifier: MIT
//! The reference script through a real back, one arm a run, for the bench to score:
//! `native` over cpal with the `native` feature, `wine` over winmm on a Windows build. A
//! build with neither gets an empty `main`, so the default build needs no sound library.
//!
//! Prints one line of counts on stdout, and exits 1 on any red. A planted arm is meant
//! to read red; the bench judges that.
//!
//! Hazard: the stream opens at load and stays silent until stdin carries the line `go`,
//! so a stream that lands on the wrong output has nothing to play. With no line it waits
//! for ever; the caller owes a timeout. A closed stdin plays nothing and reads red.
//!
//! Hazard: `took` and the wait are this bench's own reading of the back, from outside.
//! They do not show that a person heard anything, or that a real sound card played it.

#[cfg(any(windows, all(feature = "native", not(target_arch = "wasm32"))))]
#[path = "../bench/reference.rs"]
mod reference;

#[cfg(not(any(windows, all(feature = "native", not(target_arch = "wasm32")))))]
fn main() {}

#[cfg(any(windows, all(feature = "native", not(target_arch = "wasm32"))))]
fn main() -> std::process::ExitCode {
    route::run()
}

#[cfg(any(windows, all(feature = "native", not(target_arch = "wasm32"))))]
mod route {
    use std::cell::Cell;
    use std::io::BufRead as _;
    use std::process::ExitCode;
    use std::rc::Rc;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::thread::sleep;
    use std::time::{Duration, Instant};

    use log::{Level, LevelFilter, Log, Metadata, Record};
    use sound_mix::output::Output;

    use crate::reference::{BLOCK_FRAMES, END_US, FRAMES, Outcome, PINNED_HASH, Plant, Script};

    #[cfg(not(windows))]
    type Back = sound_mix::native::Native;
    #[cfg(windows)]
    type Back = sound_mix::waveout::WaveOutBack;

    #[cfg(not(windows))]
    const ROUTE: &str = "native";
    #[cfg(windows)]
    const ROUTE: &str = "wine";

    /// S4 on the native route, S5 on Wine's: the back refused what the check's own call took.
    #[cfg(not(windows))]
    const REFUSED: &str =
        "The project's sound output could not open a device that the computer check opened.";
    #[cfg(windows)]
    const REFUSED: &str =
        "The project's `waveOut` output failed where the bench's own call worked.";

    const MIX_HZ: u32 = 44_100;
    /// The crate starts no thread, and `Rc` is not `Send`, so this loop pumps.
    const PUMP: Duration = Duration::from_millis(10);
    /// How long the back may take to drain with the clock at its end.
    const TAIL: Duration = Duration::from_secs(2);
    /// Frame 44,100, where the stall arm stops pumping.
    const STALL_AT_US: u64 = 1_000_000;
    const STALL: Duration = Duration::from_secs(1);
    /// The planted `room()` waits over the red line below.
    const SLOW_ROOM: Duration = Duration::from_millis(15);
    /// One pump: a call on the back at or over this could starve the loop that pumps it.
    const WAIT_RED_US: u64 = 10_000;
    const WAIT_NOTED_US: u64 = 1_000;

    static LINES: Lines = Lines;
    static DROPPED: AtomicU64 = AtomicU64::new(0);

    /// One run's wrong input or wrong back, or none.
    #[derive(Clone, Copy)]
    enum Arm {
        Script(Plant),
        Stall,
        SlowRoom,
        OpenAt(u32),
    }

    fn arm_of(word: &str) -> Option<Arm> {
        Some(match word {
            "clean" => Arm::Script(Plant::Clean),
            "silence" => Arm::Script(Plant::Silence),
            "flip" => Arm::Script(Plant::Flip),
            "swap" => Arm::Script(Plant::Swap),
            "rate" => Arm::Script(Plant::Rate),
            "stall" => Arm::Stall,
            "slow-room" => Arm::SlowRoom,
            _ => Arm::OpenAt(word.strip_prefix("open:")?.parse().ok()?),
        })
    }

    /// Prints each line, and sums what the mix says it dropped.
    struct Lines;

    impl Log for Lines {
        fn enabled(&self, metadata: &Metadata<'_>) -> bool {
            metadata.level() <= Level::Debug
        }

        fn log(&self, record: &Record<'_>) {
            if !self.enabled(record.metadata()) {
                return;
            }
            if record.args().to_string() == "sound.frames.dropped" {
                let frames = record
                    .key_values()
                    .get("frames".into())
                    .and_then(|value| parsed_count(&value.to_string()));
                DROPPED.fetch_add(frames.unwrap_or(0), Ordering::Relaxed);
            }
            eprintln!("{}", oxide_logline::body(record));
        }

        fn flush(&self) {}
    }

    /// `oxide_trace::__shown!` writes a count in hex, with no prefix.
    fn parsed_count(text: &str) -> Option<u64> {
        u64::from_str_radix(text, 16).ok()
    }

    #[allow(
        clippy::cast_possible_truncation,
        reason = "584,000 years of microseconds"
    )]
    fn micros(span: Duration) -> u64 {
        span.as_micros() as u64
    }

    /// What the back answered and how long it took, read from outside it.
    #[derive(Default)]
    struct Reading {
        took: Cell<u32>,
        worst_us: Cell<u64>,
        over_1ms: Cell<u64>,
    }

    impl Reading {
        fn note(&self, from: Instant) {
            let waited = micros(from.elapsed());
            self.worst_us.set(self.worst_us.get().max(waited));
            if waited > WAIT_NOTED_US {
                self.over_1ms.set(self.over_1ms.get() + 1);
            }
        }
    }

    /// The real back, timed, with a planted rate or a planted wait when the arm asks.
    struct Timed {
        back: Back,
        reading: Rc<Reading>,
        open_at: Option<u32>,
        room_sleeps: bool,
    }

    impl Output for Timed {
        fn open(&mut self, rate_hz: u32) -> u32 {
            let took = self.back.open(self.open_at.unwrap_or(rate_hz));
            self.reading.took.set(took);
            took
        }

        fn room(&self) -> usize {
            let from = Instant::now();
            if self.room_sleeps {
                sleep(SLOW_ROOM);
            }
            let room = self.back.room();
            self.reading.note(from);
            room
        }

        fn write(&mut self, frames: &[i16]) {
            let from = Instant::now();
            self.back.write(frames);
            self.reading.note(from);
        }

        fn close(&mut self) {
            self.back.close();
        }
    }

    fn gate_opened() -> bool {
        let mut line = String::new();
        let read = std::io::stdin().lock().read_line(&mut line);
        matches!(read, Ok(count) if count > 0) && line.trim() == "go"
    }

    /// Runs the clock by real time to its end, then lets the back drain.
    fn play(script: &Script<Timed>, room_at_open: usize, stalls: bool) {
        script.start();
        let from = Instant::now();
        let mut stall_owed = stalls;
        loop {
            let now_us = micros(from.elapsed());
            if stall_owed && now_us >= STALL_AT_US {
                stall_owed = false;
                sleep(STALL);
                continue;
            }
            script.run_to(now_us);
            if now_us >= END_US {
                break;
            }
            sleep(PUMP);
        }
        let tail = Instant::now();
        while script.room() < room_at_open && tail.elapsed() < TAIL {
            script.run_to(END_US);
            sleep(PUMP);
        }
    }

    /// The red words, each a rule the line breaks, and the sentence a person reads for
    /// the first of them. Dropped frames come before wrong ones: frames are compared by
    /// their place, so one drop makes everything after it read wrong.
    fn judged(
        took: u32,
        gated: bool,
        outcome: &Outcome,
        given: u64,
        fill: u64,
        dropped: u64,
        worst_us: u64,
    ) -> (Vec<&'static str>, Option<String>) {
        let all = u64::from(FRAMES);
        let mut reds = Vec::new();
        let mut sentence = None;
        if took != MIX_HZ {
            reds.push("open");
            sentence = Some(REFUSED.to_owned());
        } else if !gated {
            reds.push("gate");
        } else {
            let counts_hold = dropped == 0
                && fill == 0
                && outcome.kept == outcome.frames
                && given == outcome.frames
                && outcome.frames + dropped == all;
            if !counts_hold {
                reds.push("counts");
                let blocks = all.saturating_sub(given).div_ceil(BLOCK_FRAMES);
                sentence = Some(format!(
                    "{blocks} blocks of sound went missing between the mixer and the output. \
                     Your computer is cleared."
                ));
            }
            if outcome.first_wrong.is_some() {
                reds.push("mix");
                sentence = sentence.or_else(|| outcome.sentence());
            }
            if outcome.hash != PINNED_HASH {
                reds.push("hash");
            }
            if worst_us >= WAIT_RED_US {
                reds.push("wait");
            }
        }
        (reds, sentence)
    }

    pub fn run() -> ExitCode {
        let word = std::env::args().nth(1).unwrap_or_default();
        let Some(arm) = arm_of(&word) else {
            eprintln!(
                "usage: reference_route <clean|silence|flip|swap|rate|stall|slow-room|open:HZ>"
            );
            return ExitCode::from(2);
        };
        log::set_logger(&LINES).expect("nothing else sets a logger here");
        log::set_max_level(LevelFilter::Debug);

        let reading = Rc::new(Reading::default());
        let script = Script::load(
            Timed {
                back: Back::new(),
                reading: Rc::clone(&reading),
                open_at: if let Arm::OpenAt(rate_hz) = arm {
                    Some(rate_hz)
                } else {
                    None
                },
                room_sleeps: matches!(arm, Arm::SlowRoom),
            },
            if let Arm::Script(plant) = arm {
                plant
            } else {
                Plant::Clean
            },
        );
        let took = reading.took.get();
        let room_at_open = script.room();
        let mut gated = false;
        if took == MIX_HZ {
            gated = gate_opened();
            if gated {
                play(&script, room_at_open, matches!(arm, Arm::Stall));
            }
        }
        let room_after = script.room();
        let outcome = script.outcome();
        script.close();

        let dropped = DROPPED.load(Ordering::Relaxed);
        let fill = room_at_open.saturating_sub(room_after) as u64;
        let given = outcome.kept.saturating_sub(fill);
        let worst_us = reading.worst_us.get();
        let (reds, sentence) = judged(took, gated, &outcome, given, fill, dropped, worst_us);
        println!(
            "route={ROUTE} arm={word} took={took} witness={witness} hash={hash:016x} \
             first_wrong={first_wrong} wrong={wrong} kept={kept} given={given} fill={fill} \
             dropped={dropped} wait_max_us={worst_us} wait_over_1ms={over_1ms} \
             room_at_open={room_at_open} reds={red_words} sentence=\"{said}\"",
            witness = outcome.frames,
            hash = outcome.hash,
            first_wrong = outcome
                .first_wrong
                .map_or_else(|| "none".to_owned(), |frame| frame.to_string()),
            wrong = outcome.wrong,
            kept = outcome.kept,
            over_1ms = reading.over_1ms.get(),
            red_words = if reds.is_empty() {
                "none".to_owned()
            } else {
                reds.join(",")
            },
            said = sentence.as_deref().unwrap_or("none"),
        );
        if reds.is_empty() {
            ExitCode::SUCCESS
        } else {
            ExitCode::FAILURE
        }
    }
}
