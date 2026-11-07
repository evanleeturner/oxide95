// SPDX-License-Identifier: MIT
//! The fronts on the page's back, on the host, its keys kept by [`Input`]. A
//! test replaying a run says so, `from` and `tid` left out. A name no run
//! reached carries the reading of its callers' code, its test's doc saying
//! so. The rows are a test's, their names the
//! `USER32!` spellings the layer logs.
//!
//! The desk leaves a key message's scan code 0 (a wall in `window`): a
//! logged `l` is compared with bits 16 to 24 cleared, the scan code and
//! the extended key.
use window::desk::Desk;
use window::front::Front as WindowFront;
use window::page::Page;
use window::table::{Calls as WindowCalls, Row as WindowRow};
use window::{Event, Rect};

use crate::front::{Front, Name};
use crate::table::{Calls, Row};
use crate::test_log::debug_lines;
use crate::{Input, Keys};

static CALLS: Calls = Calls {
    get_cursor_pos: "USER32!GetCursorPos",
    set_cursor_pos: "USER32!SetCursorPos",
    show_cursor: "USER32!ShowCursor",
    load_cursor: "USER32!LoadCursorA",
    clip_cursor: "USER32!ClipCursor",
    get_async_key_state: "USER32!GetAsyncKeyState",
    get_key_state: "USER32!GetKeyState",
    map_virtual_key: "USER32!MapVirtualKeyA",
};

static ROW: Row = Row {
    file: "test",
    calls: &CALLS,
};

static WINDOW_CALLS: WindowCalls = WindowCalls {
    register_class: "USER32!RegisterClassA",
    create_window: "USER32!CreateWindowExA",
    destroy_window: "USER32!DestroyWindow",
    find_window: "USER32!FindWindowA",
    load_icon: "USER32!LoadIconA",
    show_window: "USER32!ShowWindow",
    set_window_pos: "USER32!SetWindowPos",
    adjust_window_rect: "USER32!AdjustWindowRect",
    get_window_rect: "USER32!GetWindowRect",
    get_window_long: "USER32!GetWindowLongA",
    set_window_long: "USER32!SetWindowLongA",
    client_to_screen: "USER32!ClientToScreen",
    screen_to_client: "USER32!ScreenToClient",
    set_focus: "USER32!SetFocus",
    get_system_metrics: "USER32!GetSystemMetrics",
    def_window_proc: "USER32!DefWindowProcA",
    peek_message: "USER32!PeekMessageA",
    wait_message: "USER32!WaitMessage",
    translate_message: "USER32!TranslateMessage",
    dispatch_message: "USER32!DispatchMessageA",
    send_message: "USER32!SendMessageA",
    post_message: "USER32!PostMessageA",
    post_quit_message: "USER32!PostQuitMessage",
    get_message_time: "USER32!GetMessageTime",
};

static WINDOW_ROW: WindowRow = WindowRow {
    file: "test",
    calls: &WINDOW_CALLS,
    metrics: &[],
};

/// The logged messages' window, `2005e`: the keys' window, none made here.
const WINDOW: u32 = 0x0002_005e;
const PM_REMOVE: u32 = 1;
/// `WM_KEYFIRST` and `WM_KEYLAST`, the range the shell's pumps ask.
const WM_KEYFIRST: u32 = 0x100;
const WM_KEYLAST: u32 = 0x108;
/// Bits 16 to 24 of a key message's `l`, the scan code and the extended
/// key: 0 from the desk.
const SCAN: u32 = 0x01ff_0000;
const VK_SHIFT: u8 = 0x10;
const VK_MENU: u8 = 0x12;

/// The page's back, 640 by 480, its keys kept.
fn desk() -> Desk<Input<Page>> {
    Desk::new(Input::new(Page::new(0x280, 0x1e0)))
}

/// A key `key` down or up in the logged window.
const fn key(key: u8, down: bool) -> Event {
    Event::Key {
        window: WINDOW,
        key,
        down,
    }
}

/// Each key message waiting, as the shell's loop takes it: one a call,
/// translated before the next; its `(message, w, l)` and the translation's
/// answer. Bounded at 16 turns.
fn taken(front: &WindowFront<'_, Input<Page>>) -> (Vec<(u32, u32, u32)>, Vec<u32>) {
    let (mut messages, mut answers) = (Vec::new(), Vec::new());
    for _ in 0..16 {
        let Some(message) = front.peek_message(0, WM_KEYFIRST, WM_KEYLAST, PM_REMOVE) else {
            break;
        };
        messages.push((message.message, message.w, message.l));
        answers.push(front.translate_message(&message));
    }
    (messages, answers)
}

/// The logged messages with the scan code cleared.
fn unscanned(logged: &[(u32, u32, u32)]) -> Vec<(u32, u32, u32)> {
    logged
        .iter()
        .map(|&(message, w, l)| (message, w, l & !SCAN))
        .collect()
}

/// `GetCursorPos` writes the host's pointer, unlocked: a traced run read
/// 2, 2 (108,616 lines) and 320, 240 (55,180), each `v=1`.
#[test]
fn get_cursor_pos_writes_the_hosts_pointer() {
    let desk = desk();
    let front = Front::new(&ROW, &desk);
    let back = desk.screen().back();
    let mut read = Vec::new();
    let lines = debug_lines(|| {
        back.hand(Event::Pointer {
            window: WINDOW,
            x: 2,
            y: 2,
        });
        read.push(front.get_cursor_pos());
        back.hand(Event::Pointer {
            window: WINDOW,
            x: 0x140,
            y: 0xf0,
        });
        read.push(front.get_cursor_pos());
    });
    assert_eq!(read, [(2, 2), (0x140, 0xf0)]);
    assert_eq!(
        lines,
        [
            "imp.call c=USER32!GetCursorPos o=2,2 v=1 by=window-input",
            "imp.call c=USER32!GetCursorPos o=140,f0 v=1 by=window-input",
        ]
    );
}

/// `GetAsyncKeyState` polls the 3 buttons as a traced run did, `a=1`,
/// `a=2`, `a=4` `v=0` (55,180 lines each). A button down reads `0x8000`
/// and up 0 again, a reading, not a measurement: no run pressed one.
#[test]
fn get_async_key_state_polls_the_3_buttons() {
    let desk = desk();
    let front = Front::new(&ROW, &desk);
    let button = |down| Event::Button {
        window: WINDOW,
        key: 1,
        down,
    };
    let lines = debug_lines(|| {
        for key in [1, 2, 4] {
            assert_eq!(front.get_async_key_state(key), 0);
        }
        desk.screen().back().hand(button(true));
        assert_eq!(front.get_async_key_state(1), 0x8000);
        desk.screen().back().hand(button(false));
        assert_eq!(front.get_async_key_state(1), 0);
    });
    assert_eq!(
        lines,
        [
            "imp.call c=USER32!GetAsyncKeyState a=1 v=0 by=window-input",
            "imp.call c=USER32!GetAsyncKeyState a=2 v=0 by=window-input",
            "imp.call c=USER32!GetAsyncKeyState a=4 v=0 by=window-input",
            "imp.call c=USER32!GetAsyncKeyState a=1 v=8000 by=window-input",
            "imp.call c=USER32!GetAsyncKeyState a=1 v=0 by=window-input",
        ]
    );
}

/// `MapVirtualKeyA` type 2 types nothing for Alt, a traced run's `a=12,2 v=0` (6
/// lines). `A` types its capital, and type 0 answers 0, the named gap: a
/// reading, not a measurement, no run asked either.
#[test]
fn map_virtual_key_types_from_the_map() {
    let desk = desk();
    let front = Front::new(&ROW, &desk);
    let lines = debug_lines(|| {
        assert_eq!(front.map_virtual_key(0x12, 2), 0);
        assert_eq!(front.map_virtual_key(0x41, 2), 0x41);
        assert_eq!(front.map_virtual_key(0x41, 0), 0);
    });
    assert_eq!(
        lines,
        [
            "imp.call c=USER32!MapVirtualKeyA a=12,2 v=0 by=window-input",
            "imp.call c=USER32!MapVirtualKeyA a=41,2 v=41 by=window-input",
            "imp.call c=USER32!MapVirtualKeyA a=41,0 v=0 by=window-input",
        ]
    );
}

/// `GetKeyState` keeps Alt's toggle in a traced run's order: its 9 polls,
/// 10:53:52.239 to 10:57:24.572, each Ctrl, Alt and Shift (`a=11`, `a=12`,
/// `a=10`), Ctrl and Shift `v=0`; bit 0 flips at each press, `0xff80`
/// while down. Alt's ups were never logged: the presses between the polls
/// are a reading of the run's log and the answers, the
/// test's.
#[test]
fn get_key_state_keeps_alts_toggle_in_a_runs_order() {
    // Alt's presses before each poll, `true` down, and the poll's Alt line.
    let polls: [(&[bool], &str); 9] = [
        (&[true], "a=12 v=ff81"),
        (&[false], "a=12 v=1"),
        (&[true], "a=12 v=ff80"),
        (&[false, true], "a=12 v=ff81"),
        (&[false, true], "a=12 v=ff80"),
        (&[false, true], "a=12 v=ff81"),
        (&[false], "a=12 v=1"),
        (&[], "a=12 v=1"),
        (&[true], "a=12 v=ff80"),
    ];
    let desk = desk();
    let front = Front::new(&ROW, &desk);
    let lines = debug_lines(|| {
        for (presses, _) in polls {
            for &down in presses {
                desk.screen().back().hand(key(VK_MENU, down));
            }
            desk.pump();
            for key in [0x11, 0x12, 0x10] {
                let _ = front.get_key_state(key);
            }
        }
    });
    let logged: Vec<String> = polls
        .iter()
        .flat_map(|(_, alt)| ["a=11 v=0", alt, "a=10 v=0"])
        .map(|line| format!("imp.call c=USER32!GetKeyState {line} by=window-input"))
        .collect();
    assert_eq!(lines, logged);
}

/// The pointer, no run having reached play:
/// `ClipCursor` locks it, the kept point then moved by motion alone and
/// held in the clip, the host's pointer not followed; a null clip lets
/// both go, and the point follows the host's pointer again, its motion
/// not. The game's 640 by 480 clipped on a screen 1024 by 768.
#[test]
fn a_clip_locks_the_point_and_motion_moves_it_inside() {
    let desk = Desk::new(Input::new(Page::new(0x400, 0x300)));
    let front = Front::new(&ROW, &desk);
    let back = desk.screen().back();
    let pointer = Event::Pointer {
        window: WINDOW,
        x: 0x300,
        y: 0x200,
    };
    let mut read = Vec::new();
    let lines = debug_lines(|| {
        assert_eq!(front.set_cursor_pos(0x140, 0xf0), 1);
        assert_eq!(front.clip_cursor(Some(Rect::sized(0, 0, 0x280, 0x1e0))), 1);
        back.hand(pointer);
        back.hand(Event::Motion { dx: 0x10, dy: -8 });
        read.push(front.get_cursor_pos());
        back.hand(Event::Motion {
            dx: 0x1000,
            dy: 0x1000,
        });
        read.push(front.get_cursor_pos());
        assert_eq!(front.clip_cursor(None), 1);
        back.hand(pointer);
        back.hand(Event::Motion { dx: 0x10, dy: -8 });
        read.push(front.get_cursor_pos());
    });
    assert_eq!(read, [(0x150, 0xe8), (0x27f, 0x1df), (0x300, 0x200)]);
    assert!(!desk.screen().keys().locked);
    assert_eq!(
        lines,
        [
            "imp.call c=USER32!SetCursorPos a=140,f0 v=1 by=window-input",
            "imp.call c=USER32!ClipCursor a=0,0,280,1e0 v=1 by=window-input",
            "imp.call c=USER32!GetCursorPos o=150,e8 v=1 by=window-input",
            "imp.call c=USER32!GetCursorPos o=27f,1df v=1 by=window-input",
            "imp.call c=USER32!ClipCursor v=1 by=window-input",
            "imp.call c=USER32!GetCursorPos o=300,200 v=1 by=window-input",
        ]
    );
}

/// `ShowCursor` counts from 0, hidden to -1 and shown back to 0;
/// `LoadCursorA` hands a fresh handle for the arrow, `#7f00`. A reading,
/// not a measurement: no run reached either.
#[test]
fn show_cursor_counts_and_load_cursor_hands_a_handle() {
    let desk = desk();
    let front = Front::new(&ROW, &desk);
    let lines = debug_lines(|| {
        assert_eq!(front.show_cursor(0), -1);
        assert_eq!(front.show_cursor(1), 0);
        assert_eq!(front.load_cursor(0, Name::Number(0x7f00)), 0x0001_0020);
    });
    assert_eq!(
        lines,
        [
            "imp.call c=USER32!ShowCursor a=0 v=ffffffff by=window-input",
            "imp.call c=USER32!ShowCursor a=1 v=0 by=window-input",
            "imp.call c=USER32!LoadCursorA a=0 s=#7f00 v=10020 by=window-input",
        ]
    );
}

/// The blur: the host's window losing the user's attention lets
/// every key and button go, the toggles kept, and the desk's held keys
/// with them, so the next press is a fresh down. A fresh `Keys` is left as
/// it was.
#[test]
fn a_blur_lets_every_key_go_and_keeps_the_toggles() {
    let fresh = Keys::new((0x280, 0x1e0));
    let mut blurred = fresh.clone();
    blurred.apply(Event::Active {
        window: WINDOW,
        active: false,
    });
    assert_eq!(blurred, fresh);

    let desk = desk();
    let front = Front::new(&ROW, &desk);
    let windows = WindowFront::new(&WINDOW_ROW, &desk);
    let back = desk.screen().back();
    back.hand(key(b'A', true));
    back.hand(key(VK_MENU, true));
    back.hand(Event::Button {
        window: WINDOW,
        key: 1,
        down: true,
    });
    back.hand(Event::Active {
        window: WINDOW,
        active: false,
    });
    desk.pump();
    assert!(desk.state().held.is_empty());
    let lines = debug_lines(|| {
        assert_eq!(front.get_key_state(0x41), 1);
        assert_eq!(front.get_key_state(0x12), 1);
        assert_eq!(front.get_async_key_state(1), 0);
    });
    assert_eq!(
        lines,
        [
            "imp.call c=USER32!GetKeyState a=41 v=1 by=window-input",
            "imp.call c=USER32!GetKeyState a=12 v=1 by=window-input",
            "imp.call c=USER32!GetAsyncKeyState a=1 v=0 by=window-input",
        ]
    );
    desk.state().queue.clear();
    back.hand(key(b'A', true));
    let (messages, _) = taken(&windows);
    assert_eq!(messages, [(0x100, 0x41, 1), (0x102, 0x61, 1)]);
}

/// The desk's key messages and `TranslateMessage`'s characters, a traced run's
/// loop at 10:57:24.571 to .572 replayed: Escape, Return, then Alt+M, each
/// taken by `PeekMessageA` (`a=0,100,108,1`) and translated before the
/// next, a character taken straight after its key; Alt types none. Each
/// row is the line's `o`, its 2nd to 4th fields. `TranslateMessage`'s
/// answers, 1 for a key message and 0 for a character, are Windows'
/// documented ones: the layer logs no `TranslateMessage`.
#[test]
fn keys_queue_and_translate_as_a_runs_loop_took_them() {
    let logged = [
        (0x100, 0x1b, 0x0001_0001),
        (0x102, 0x1b, 0x0001_0001),
        (0x101, 0x1b, 0xc001_0001),
        (0x100, 0x0d, 0x001c_0001),
        (0x102, 0x0d, 0x001c_0001),
        (0x101, 0x0d, 0xc01c_0001),
        (0x104, 0x12, 0x2038_0001),
        (0x104, 0x4d, 0x2032_0001),
        (0x106, 0x6d, 0x2032_0001),
    ];
    let desk = desk();
    let windows = WindowFront::new(&WINDOW_ROW, &desk);
    for event in [
        key(0x1b, true),
        key(0x1b, false),
        key(0x0d, true),
        key(0x0d, false),
        key(VK_MENU, true),
        key(b'M', true),
    ] {
        desk.screen().back().hand(event);
    }
    let (messages, answers) = taken(&windows);
    assert_eq!(messages, unscanned(&logged));
    assert_eq!(answers, [1, 0, 1, 1, 0, 1, 1, 1, 0]);
}

/// Alt+O as a traced run's loop took it, 10:55:45.114 to .115: Alt's
/// `WM_SYSKEYDOWN`, O's under it, then its `WM_SYSCHAR` `o`.
#[test]
fn alt_o_posts_its_syschar_as_a_runs_loop_took_it() {
    let logged = [
        (0x104, 0x12, 0x2038_0001),
        (0x104, 0x4f, 0x2018_0001),
        (0x106, 0x6f, 0x2018_0001),
    ];
    let desk = desk();
    let windows = WindowFront::new(&WINDOW_ROW, &desk);
    desk.screen().back().hand(key(VK_MENU, true));
    desk.screen().back().hand(key(b'O', true));
    let (messages, _) = taken(&windows);
    assert_eq!(messages, unscanned(&logged));
}

/// A held key's repeat and Shift's capital, a reading, not a measurement: no
/// run held a key or typed a letter without Alt. `A` down twice, then up: `l` 1,
/// then bit 30 the key down before, then bits 30 and 31 an up; its
/// character `a`. Shift comes in a pump of its own, a character reading
/// the keys as the last pump left them (a wall); with it down, `A`.
#[test]
fn a_repeat_sets_bit_30_and_shift_types_the_capital() {
    let desk = desk();
    let windows = WindowFront::new(&WINDOW_ROW, &desk);
    let back = desk.screen().back();
    for down in [true, true, false] {
        back.hand(key(b'A', down));
    }
    let (repeated, _) = taken(&windows);
    assert_eq!(
        repeated,
        [
            (0x100, 0x41, 1),
            (0x102, 0x61, 1),
            (0x100, 0x41, 0x4000_0001),
            (0x102, 0x61, 0x4000_0001),
            (0x101, 0x41, 0xc000_0001),
        ]
    );
    back.hand(key(VK_SHIFT, true));
    back.hand(key(b'A', true));
    let (shifted, _) = taken(&windows);
    assert_eq!(
        shifted,
        [(0x100, 0x10, 1), (0x100, 0x41, 1), (0x102, 0x41, 1)]
    );
}
