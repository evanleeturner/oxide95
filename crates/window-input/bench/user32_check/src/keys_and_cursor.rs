// SPDX-License-Identifier: MIT
//! `wu3`: `window_input::user32`'s 8, with `xdotool` outside. The program prints `ask <what>`,
//! the driver does it and answers on stdin, and the program keeps its queue moving meanwhile, as
//! Wine only takes in a host's input while the thread pumps.
//!
//! `GetAsyncKeyState` hands out its "pressed since" bit once, to whoever reads first. So each
//! round reads both ways round: the first reader must see the bit and the second must not, which
//! shows the thin call read the same real state and took the bit itself. Shift's plain code,
//! `0x10`, carries no such bit under Wine, on either side (measured), so its worked reads say
//! that, and `0xa0`, the left Shift's own code, is read as well.
//!
//! Read from outside: the 2 key calls and the 3 cursor calls that move or read a point.
//! `ShowCursor`, `LoadCursorA` and `MapVirtualKeyA` have nothing `xdotool` can read, so they
//! stand on the hand-declared call and their worked values alone.
use std::io;
use std::process::ExitCode;
use std::sync::mpsc::{Receiver, TryRecvError, channel};
use std::thread;
use std::time::Duration;

use window_input::user32 as thin;

use crate::bench::{Bench, FROM, Plant, Same, Want, address, code, spot, take_lines, text, words};
use crate::hand::{self, Point, Rect};

const NAMES: [&str; 8] = [
    "GetCursorPos",
    "SetCursorPos",
    "ShowCursor",
    "LoadCursorA",
    "ClipCursor",
    "GetAsyncKeyState",
    "GetKeyState",
    "MapVirtualKeyA",
];
const SENTENCE: &str = "`xdotool` pressed one key and the crate read another. `window-input`'s thin call, not the program.";
/// `WS_POPUP | WS_VISIBLE`: no frame, so the client area starts at the window's corner.
const VISIBLE_POPUP: u32 = 0x9000_0000;
/// One key: its virtual code, the name `xdotool` presses it by, and its worked first read of
/// a press and of a tap.
struct Key {
    code: u32,
    name: &'static str,
    first_read: u32,
    tapped: u32,
}
const KEYS: [Key; 4] = [
    Key {
        code: 0x41,
        name: "a",
        first_read: FIRST_READ,
        tapped: TAPPED,
    },
    Key {
        code: 0x1b,
        name: "Escape",
        first_read: FIRST_READ,
        tapped: TAPPED,
    },
    Key {
        code: 0xa0,
        name: "Shift_L",
        first_read: FIRST_READ,
        tapped: TAPPED,
    },
    Key {
        code: 0x10,
        name: "Shift_L",
        first_read: HELD,
        tapped: 0,
    },
];
const HELD: u32 = 0x8000;
const FIRST_READ: u32 = 0x8001;
const TAPPED: u32 = 1;
/// How long a press gets to reach this thread; short of the screen's key repeat.
const SETTLE_TURNS: u32 = 40;
const TURN: Duration = Duration::from_millis(5);
/// `IDC_ARROW`.
const ARROW: u32 = 0x7f00;
/// A code, a kind, and the worked answer under the private screen's US map.
const MAPPED: [(u32, u32, u32); 5] = [
    (0x41, 0, 0x1e),
    (0x1e, 1, 0x41),
    (0x1b, 0, 1),
    (0x10, 0, 0x2a),
    (0x41, 2, 0x41),
];

/// Which of the pair reads first.
#[derive(Clone, Copy, PartialEq, Eq)]
enum First {
    Thin,
    Hand,
}

/// The driver on the other end of `ask`.
pub struct Asker {
    pub replies: Receiver<String>,
}

impl Asker {
    pub fn new() -> Self {
        let (send, replies) = channel();
        thread::spawn(move || {
            for line in io::stdin().lines() {
                let Ok(line) = line else { break };
                if send.send(line).is_err() {
                    break;
                }
            }
        });
        Self { replies }
    }

    /// The driver's answer; the queue keeps moving until it comes.
    pub fn ask(&self, request: &str) -> String {
        println!("ask {request}");
        loop {
            hand::pump();
            match self.replies.try_recv() {
                Ok(reply) => return reply,
                Err(TryRecvError::Empty) => thread::sleep(TURN),
                Err(TryRecvError::Disconnected) => panic!("the driver went away"),
            }
        }
    }

    /// Keeps the queue moving long enough for what was just done to arrive.
    pub fn settle() {
        for _ in 0..SETTLE_TURNS {
            hand::pump();
            thread::sleep(TURN);
        }
    }

    pub fn press(&self, request: &str) {
        self.ask(request);
        Self::settle();
    }
}

unsafe extern "system" fn procedure(window: u32, message: u32, w: u32, l: u32) -> u32 {
    hand::def_window_proc(window, message, w, l)
}

/// The thin call's read, its `SHORT` as the caller takes it; the plant asks the next key along.
fn thin_read(bench: &mut Bench, key: u32) -> u32 {
    let asked = if bench.plant == Plant::KeyOffByOne {
        key + 1
    } else {
        key
    };
    let mut whole = 0;
    let short = bench.alone("GetAsyncKeyState", &Want::a(&[key]), || {
        whole = thin::get_async_key_state(FROM, asked);
        whole & 0xffff
    });
    println!("row WU3 GetAsyncKeyState key={key:x} eax={whole:x}");
    short
}

/// `[thin, hand]`, read in the order asked.
fn read_pair(bench: &mut Bench, key: u32, first: First) -> [u32; 2] {
    let pair = match first {
        First::Thin => {
            let by_thin = thin_read(bench, key);
            [by_thin, hand::get_async_key_state(key)]
        }
        First::Hand => {
            let by_hand = hand::get_async_key_state(key);
            [thin_read(bench, key), by_hand]
        }
    };
    println!(
        "row WU3 pair key={key:x} thin={:x} hand={:x}",
        pair[0], pair[1]
    );
    pair
}

/// Neither side seeing a pressed key clears the thin call: nothing reached either. So does the
/// hand-declared call missing its worked value when it read first, as no thin call came before
/// it. Read second, it may rightly hold a bit a wrong thin call left behind, so it clears
/// nothing.
fn judge(bench: &mut Bench, key: u32, first: First, what: &str, got: [u32; 2], want: [u32; 2]) {
    if got == want {
        return;
    }
    let detail = format!("key {key:x} {what}: {got:x?}, worked {want:x?}");
    if got == [0, 0] || (first == First::Hand && got[1] != want[1]) {
        bench.control_red("GetAsyncKeyState", &detail);
    } else {
        bench.thin_red("GetAsyncKeyState", &detail);
    }
}

fn key_state(bench: &mut Bench, key: u32, down: bool) {
    let [_, answer] = bench.beside(
        "GetKeyState",
        Same::Answer,
        &Want::a(&[key]),
        || thin::get_key_state(FROM, key) & 0xffff,
        || hand::get_key_state(key),
    );
    if (answer & HELD != 0) != down {
        bench.control_red(
            "GetKeyState",
            &format!("key {key:x} read {answer:x}, down {down}"),
        );
    }
}

/// `[thin, hand]` when the first reader sees `seen` and the second `after`.
fn by_order(first: First, seen: u32, after: u32) -> [u32; 2] {
    match first {
        First::Thin => [seen, after],
        First::Hand => [after, seen],
    }
}

fn one_key(bench: &mut Bench, asker: &Asker, spec: &Key) {
    let Key {
        code: key,
        name,
        first_read,
        tapped,
    } = *spec;
    for first in [First::Thin, First::Hand] {
        asker.press(&format!("keydown {name}"));
        let got = read_pair(bench, key, first);
        let want = by_order(first, first_read, HELD);
        judge(bench, key, first, "first read of a press", got, want);
        let got = read_pair(bench, key, first);
        judge(bench, key, first, "a held key", got, [HELD, HELD]);
        key_state(bench, key, true);
        asker.press(&format!("keyup {name}"));
        let got = read_pair(bench, key, first);
        judge(bench, key, first, "let go", got, [0, 0]);
        key_state(bench, key, false);
    }
    for first in [First::Thin, First::Hand] {
        asker.press(&format!("key {name}"));
        let got = read_pair(bench, key, first);
        judge(
            bench,
            key,
            first,
            "a tap between reads",
            got,
            by_order(first, tapped, 0),
        );
        let got = read_pair(bench, key, first);
        judge(bench, key, first, "after a tap", got, [0, 0]);
    }
}

/// The point both ways, against a worked one if there is one; answers the hand's.
fn read_cursor(bench: &mut Bench, value: Option<Point>) -> Point {
    let (mut by_thin, mut by_hand) = ([0; 2], [0; 2]);
    let want = value.map_or_else(Want::default, |point| Want::default().o(&words(point)));
    bench.beside(
        "GetCursorPos",
        Same::Answer,
        &want,
        // SAFETY: a record of the size it writes.
        || unsafe { thin::get_cursor_pos(FROM, spot(&mut by_thin)) },
        || hand::get_cursor_pos(&mut by_hand),
    );
    bench.agree("GetCursorPos", "point", by_thin, by_hand);
    if let Some(point) = value {
        bench.worked("GetCursorPos", "point", by_hand, point);
    }
    by_hand
}

fn cursor(bench: &mut Bench, asker: &Asker, t: u32) {
    asker.press("mousemove 100 100");
    read_cursor(bench, Some([100, 100]));
    let mut client: Point = [100, 100];
    // SAFETY: a record of the size it writes.
    let _ = unsafe { window::user32::screen_to_client(FROM, t, spot(&mut client)) };
    take_lines();
    bench.agree("GetCursorPos", "the point in the window", client, [60, 60]);

    let want = Want::a(&[200, 150]);
    let answer = bench.alone("SetCursorPos", &want, || {
        thin::set_cursor_pos(FROM, 200, 150)
    });
    bench.agree("SetCursorPos", "answer", answer, 1);
    read_cursor(bench, Some([200, 150]));
    let seen = asker.ask("location");
    bench.agree(
        "SetCursorPos",
        "where X says it went",
        seen.as_str(),
        "200 150",
    );
    bench.worked("SetCursorPos", "answer", hand::set_cursor_pos(210, 160), 1);
    let seen = asker.ask("location");
    bench.worked(
        "SetCursorPos",
        "where X says it went",
        seen.as_str(),
        "210 160",
    );
}

/// The hand's clip first, so the thin call must move it to its own for its read to pass.
fn clip(bench: &mut Bench, asker: &Asker) {
    let mut read = [0; 4];
    let control: Rect = [10, 10, 20, 20];
    hand::clip_cursor(&control);
    hand::get_clip_cursor(&mut read);
    bench.worked("ClipCursor", "the hand's clip", read, control);

    let rectangle: Rect = [50, 50, 300, 200];
    let want = Want::a(&words(rectangle));
    // SAFETY: a whole record, read during the call.
    let answer = bench.alone("ClipCursor", &want, || unsafe {
        thin::clip_cursor(FROM, address(&rectangle))
    });
    bench.agree("ClipCursor", "answer", answer, 1);
    hand::get_clip_cursor(&mut read);
    bench.agree("ClipCursor", "the clip it set", read, rectangle);

    asker.press("mousemove 500 400");
    let held = read_cursor(bench, None);
    if !(50..300).contains(&held[0]) || !(50..200).contains(&held[1]) {
        bench.control_red(
            "ClipCursor",
            &format!("the cursor stands outside it, at {held:?}"),
        );
    }
    println!(
        "row WU3 clip held {held:?}, X says {}",
        asker.ask("location")
    );

    // SAFETY: null lets the clip go.
    let answer = bench.alone("ClipCursor", &Want::a(&[]), || unsafe {
        thin::clip_cursor(FROM, 0)
    });
    bench.agree("ClipCursor", "answer", answer, 1);
    hand::get_clip_cursor(&mut read);
    bench.agree("ClipCursor", "the clip let go", read, [0, 0, 1024, 768]);
    asker.press("mousemove 500 400");
    read_cursor(bench, Some([500, 400]));
}

/// A chain: thin and hand move one count in turn, each answering where it left it.
fn show(bench: &mut Bench) {
    let hide = 0_u32;
    let shown = 1_u32;
    let first = bench.alone("ShowCursor", &Want::a(&[hide]), || {
        thin::show_cursor(FROM, hide)
    });
    let second = hand::show_cursor(1);
    let third = bench.alone("ShowCursor", &Want::a(&[shown]), || {
        thin::show_cursor(FROM, shown)
    });
    let fourth = hand::show_cursor(0);
    bench.agree(
        "ShowCursor",
        "the count",
        [first, third],
        [second.wrapping_sub(1), second.wrapping_add(1)],
    );
    bench.worked("ShowCursor", "the count at rest", [second, fourth], [0, 0]);
}

fn load(bench: &mut Bench, instance: u32) {
    let [_, arrow] = bench.beside(
        "LoadCursorA",
        Same::Answer,
        &Want::a(&[0]).s("#7f00"),
        // SAFETY: a resource id.
        || unsafe { thin::load_cursor(FROM, 0, ARROW) },
        || hand::load_cursor(0, ARROW),
    );
    bench.worked("LoadCursorA", "a cursor", arrow != 0, true);
    let [_, none] = bench.beside(
        "LoadCursorA",
        Same::Answer,
        &Want::a(&[instance]).s("no such"),
        // SAFETY: a text that lives on.
        || unsafe { thin::load_cursor(FROM, instance, text(c"no such")) },
        || hand::load_cursor(instance, text(c"no such")),
    );
    bench.worked("LoadCursorA", "a name not here", none, 0);
}

fn map(bench: &mut Bench) {
    for (key, kind, value) in MAPPED {
        let [_, answer] = bench.beside(
            "MapVirtualKeyA",
            Same::Answer,
            &Want::a(&[key, kind]),
            || thin::map_virtual_key(FROM, key, kind),
            || hand::map_virtual_key(key, kind),
        );
        bench.worked("MapVirtualKeyA", "answer", answer, value);
    }
}

/// Runs the check on one visible window at 40,40, once the driver has given it focus.
pub fn run(plant: Plant) -> ExitCode {
    let mut bench = Bench::new("WU3", "window-input", SENTENCE, plant, &NAMES);
    let asker = Asker::new();
    let instance = hand::module();
    let class = [
        0,
        code(procedure as *const ()),
        0,
        0,
        instance,
        0,
        0,
        0,
        0,
        text(c"uc_keys"),
    ];
    hand::register_class(&class);
    let t = hand::create_window(
        c"uc_keys",
        c"uc keys",
        VISIBLE_POPUP,
        [40, 40, 320, 200],
        instance,
    );
    asker.press("focus");
    hand::set_focus(t);
    Asker::settle();
    for key in &KEYS {
        one_key(&mut bench, &asker, key);
    }
    cursor(&mut bench, &asker, t);
    clip(&mut bench, &asker);
    show(&mut bench);
    load(&mut bench, instance);
    map(&mut bench);
    hand::destroy_window(t);
    bench.finish()
}
