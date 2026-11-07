// SPDX-License-Identifier: MIT
//! The native back's keys and its grab through `Desk` and `Input`, one check a run: each
//! event the host raised, then what the front, the queue and the keys answer, one JSON line
//! each. It judges nothing. A `need` line asks the script outside to act by `xdotool`: the
//! program never presses a key of its own. A bounded run, and never a test: Linux turns the
//! loop on the main thread alone.
//!
//! `DISPLAY=<a private screen> timeout 60 native_keys <WD1|WD3|WD7> <plant>`
//!
//! It exits 0 once the host made both windows, 1 otherwise, 2 on a word it does not know.
use std::cell::RefCell;
use std::process::ExitCode;
use std::thread;
use std::time::Duration;

use window::desk::Desk;
use window::native::{Hold, Native};
use window::{Event, Rect, Screen};
use window_input::Input;
use window_input::front::Front;
use window_input::table::{Calls, Row};

const SCREEN: (i32, i32) = (640, 480);
const FIRST: u32 = 1;
const SECOND: u32 = 2;
const FIRST_PLACE: Rect = Rect::sized(40, 40, 240, 180);
const SECOND_PLACE: Rect = Rect::sized(360, 40, 240, 180);
const PUMP_GAP: Duration = Duration::from_millis(20);
const MADE_TURNS: u32 = 25;
/// 5 s of pumps: what an outside act is given to arrive.
const AWAIT_TURNS: u32 = 250;
const KEY_A: u8 = 0x41;
const KEY_B: u8 = 0x42;
const ASYNC_DOWN: u32 = 0x8000;
const LETTER_A: &str = "a";

static CALLS: Calls = Calls {
    get_cursor_pos: "GetCursorPos",
    set_cursor_pos: "SetCursorPos",
    show_cursor: "ShowCursor",
    load_cursor: "LoadCursorA",
    clip_cursor: "ClipCursor",
    get_async_key_state: "GetAsyncKeyState",
    get_key_state: "GetKeyState",
    map_virtual_key: "MapVirtualKeyA",
};
static ROW: Row = Row {
    file: "native_keys",
    calls: &CALLS,
};

type Rig = Desk<Input<Seen<Native>>>;

/// A fault put in on purpose at run time, each in this program, none in the crates.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Plant {
    None,
    /// Each key's number 1 too high, under the keys.
    IndexOff,
    /// The lost focus never reaches the keys.
    ActiveUnwired,
    /// A grab that asks `Locked` and has no fallback.
    LockedAlone,
}

impl Plant {
    fn named(word: &str) -> Option<Self> {
        match word {
            "none" => Some(Self::None),
            "index-off" => Some(Self::IndexOff),
            "active-unwired" => Some(Self::ActiveUnwired),
            "locked-alone" => Some(Self::LockedAlone),
            _ => None,
        }
    }
}

/// A back as `Input` takes it: each event written as the host raised it, so a wrong answer
/// past here is never the host's, then handed on, or its planted wrong form.
struct Seen<S> {
    back: S,
    plant: Plant,
    events: RefCell<Vec<Event>>,
}

impl<S: Screen> Screen for Seen<S> {
    fn size(&self) -> (i32, i32) {
        self.back.size()
    }

    fn place(&self, window: u32, rect: Rect, shown: bool) {
        self.back.place(window, rect, shown);
    }

    fn gone(&self, window: u32) {
        self.back.gone(window);
    }

    fn pump(&self, into: &mut dyn FnMut(Event)) {
        self.back.pump(&mut |event| {
            println!(
                r#"{{"row":"event","ms":{},"event":{:?}}}"#,
                self.back.now(),
                format!("{event:?}")
            );
            self.events.borrow_mut().push(event);
            match (self.plant, event) {
                (Plant::IndexOff, Event::Key { window, key, down }) => into(Event::Key {
                    window,
                    key: key.wrapping_add(1),
                    down,
                }),
                (Plant::ActiveUnwired, Event::Active { active: false, .. }) => {}
                _ => into(event),
            }
        });
    }

    fn now(&self) -> u32 {
        self.back.now()
    }

    fn quiet(&self, line: impl FnOnce()) {
        self.back.quiet(line);
    }

    fn character(&self, key: u32) -> u32 {
        self.back.character(key)
    }
}

/// Pumps up to `turns` times, 20 ms apart, until `until` holds for an event the host raised;
/// whether it did. An event an answer's own pump took counts at the next turn.
fn turn(rig: &Rig, turns: u32, until: impl Fn(Event) -> bool) -> bool {
    for _ in 0..turns {
        rig.pump();
        let seen = rig.screen().back().events.take();
        if seen.into_iter().any(&until) {
            return true;
        }
        thread::sleep(PUMP_GAP);
    }
    false
}

/// Asks the script outside for `act` on `word`, waits for the event it should raise, and
/// writes whether it came.
fn act(rig: &Rig, act: &str, word: &str, until: impl Fn(Event) -> bool) {
    println!(r#"{{"need":{act:?},"key":{word:?}}}"#);
    let met = turn(rig, AWAIT_TURNS, until);
    println!(r#"{{"row":"met","act":{act:?},"met":{met}}}"#);
}

/// Asks the script outside to move the focus to the window at `place`; `until` the event
/// that says it moved.
fn focus(rig: &Rig, window: u32, place: Rect, until: impl Fn(Event) -> bool) {
    println!(
        r#"{{"need":"focus","window":{window},"x":{},"y":{}}}"#,
        place.left, place.top
    );
    let met = turn(rig, AWAIT_TURNS, until);
    println!(r#"{{"row":"met","act":"focus","window":{window},"met":{met}}}"#);
}

/// One read at each link: the 2 keys by the front first, as its own pump may take a later
/// event; then the keys and the queue as they stand, no pump between.
fn read(stage: &str, rig: &Rig, front: &Front<'_, Seen<Native>>) {
    let a = front.get_async_key_state(u32::from(KEY_A));
    let b = front.get_async_key_state(u32::from(KEY_B));
    let a_state = front.get_key_state(u32::from(KEY_A));
    let keys = rig.screen().keys();
    let numbers = |set: &[bool; 256]| (0..256).filter(|at| set[*at]).collect::<Vec<usize>>();
    let (down, toggled) = (numbers(&keys.down), numbers(&keys.toggled));
    drop(keys);
    let queue: Vec<[u32; 2]> = rig
        .state()
        .queue
        .iter()
        .map(|message| [message.message, message.w])
        .collect();
    println!(
        r#"{{"row":"read","stage":{stage:?},"a":{},"b":{},"a_answer":{a},"b_answer":{b},"a_state":{a_state},"down":{down:?},"toggled":{toggled:?},"queue":{queue:?}}}"#,
        a & ASYNC_DOWN != 0,
        b & ASYNC_DOWN != 0
    );
}

/// A key to the event, the pump, the keys and the front's answer; and its up.
fn keys_check(rig: &Rig, front: &Front<'_, Seen<Native>>) {
    focus(rig, FIRST, FIRST_PLACE, |event| {
        matches!(
            event,
            Event::Active {
                window: FIRST,
                active: true
            }
        )
    });
    act(rig, "keydown", LETTER_A, |event| {
        matches!(
            event,
            Event::Key {
                key: KEY_A,
                down: true,
                ..
            }
        )
    });
    read("down", rig, front);
    act(rig, "keyup", LETTER_A, |event| {
        matches!(
            event,
            Event::Key {
                key: KEY_A,
                down: false,
                ..
            }
        )
    });
    read("up", rig, front);
}

/// A key held while the focus goes to the program's other window; its toggle stays.
fn blur_check(rig: &Rig, front: &Front<'_, Seen<Native>>) {
    focus(rig, FIRST, FIRST_PLACE, |event| {
        matches!(
            event,
            Event::Active {
                window: FIRST,
                active: true
            }
        )
    });
    act(rig, "keydown", LETTER_A, |event| {
        matches!(
            event,
            Event::Key {
                key: KEY_A,
                down: true,
                ..
            }
        )
    });
    read("held", rig, front);
    focus(rig, SECOND, SECOND_PLACE, |event| {
        matches!(
            event,
            Event::Active {
                window: FIRST,
                active: false
            }
        )
    });
    read("blurred", rig, front);
    act(rig, "keyup", LETTER_A, |event| {
        matches!(
            event,
            Event::Key {
                key: KEY_A,
                down: false,
                ..
            }
        )
    });
    read("let-go", rig, front);
}

/// Each hold asked, then the program's lock as the back takes it, the cursor hidden, the
/// lock let go.
fn grab_check(rig: &Rig, plant: Plant) {
    focus(rig, FIRST, FIRST_PLACE, |event| {
        matches!(
            event,
            Event::Active {
                window: FIRST,
                active: true
            }
        )
    });
    let back = &rig.screen().back().back;
    for hold in [Hold::Locked, Hold::Confined, Hold::Free] {
        let answer = back.hold(FIRST, hold);
        println!(
            r#"{{"row":"hold","asked":"{hold:?}","answer":{:?}}}"#,
            format!("{answer:?}")
        );
    }
    let grabbed = if plant == Plant::LockedAlone {
        back.hold(FIRST, Hold::Locked).map(|()| Hold::Locked)
    } else {
        back.grab(FIRST, true)
    };
    println!(
        r#"{{"row":"grab","locked":true,"answer":{:?}}}"#,
        format!("{grabbed:?}")
    );
    let hidden = back.cursor_shown(FIRST, false);
    println!(
        r#"{{"row":"cursor","shown":false,"answer":{:?}}}"#,
        format!("{hidden:?}")
    );
    let free = back.grab(FIRST, false);
    println!(
        r#"{{"row":"grab","locked":false,"answer":{:?}}}"#,
        format!("{free:?}")
    );
    let _ = back.cursor_shown(FIRST, true);
}

fn main() -> ExitCode {
    let words: Vec<String> = std::env::args().skip(1).collect();
    let [check, plant] = words.as_slice() else {
        println!("call: native_keys <WD1|WD3|WD7> <plant>");
        return ExitCode::from(2);
    };
    let Some(plant) = Plant::named(plant) else {
        println!("no plant {plant}");
        return ExitCode::from(2);
    };
    let back = match Native::new(SCREEN.0, SCREEN.1) {
        Ok(back) => back,
        Err(refusal) => {
            println!(r#"{{"row":"refused","said":{:?}}}"#, refusal.to_string());
            return ExitCode::FAILURE;
        }
    };
    let rig = Desk::new(Input::new(Seen {
        back,
        plant,
        events: RefCell::new(Vec::new()),
    }));
    let front = Front::new(&ROW, &rig);
    rig.screen().place(FIRST, FIRST_PLACE, true);
    rig.screen().place(SECOND, SECOND_PLACE, true);
    turn(&rig, MADE_TURNS, |_| false);
    for window in [FIRST, SECOND] {
        let made = rig.screen().back().back.made(window);
        println!(
            r#"{{"row":"made","window":{window},"answer":{:?}}}"#,
            format!("{made:?}")
        );
        if made != Ok(true) {
            return ExitCode::FAILURE;
        }
    }
    match check.as_str() {
        "WD1" => keys_check(&rig, &front),
        "WD3" => blur_check(&rig, &front),
        "WD7" => grab_check(&rig, plant),
        _ => {
            println!("no check {check}");
            return ExitCode::from(2);
        }
    }
    rig.screen().gone(FIRST);
    rig.screen().gone(SECOND);
    turn(&rig, 5, |_| false);
    println!(r#"{{"row":"done"}}"#);
    ExitCode::SUCCESS
}
