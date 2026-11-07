// SPDX-License-Identifier: MIT
//! The loops: the queue's fronts on the page's back, and one
//! whole loop as a program's measured loops turn it (wait when idle, then
//! one message a call, translated and dispatched, ending on `WM_QUIT`).
//! The row is a test's, its names the `USER32!` spellings a game's rows
//! give; the numbers are this crate's own handles and the page's time,
//! no game's. The lines from real runs are a game's rows crate's.
use crate::desk::Desk;
use crate::front::{Create, Front};
use crate::page::{Page, Placed};
use crate::table::{Calls, Row};
use crate::test_log::debug_lines;
use crate::{Event, Message, Rect};

static CALLS: Calls = Calls {
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

static ROW: Row = Row {
    file: "test",
    calls: &CALLS,
    metrics: &[],
};

const PM_REMOVE: u32 = 1;
const WM_CLOSE: u32 = 0x10;
const WM_QUIT: u32 = 0x12;
const WM_COMMAND: u32 = 0x111;
/// A program's own message number, `WM_USER`.
const WM_USER: u32 = 0x400;
/// The test class's procedure, an address as a program gives it.
const PROCEDURE: u32 = 0x0040_1000;

/// A shown popup of the test class, 640 by 480, its handle.
fn window(front: &Front<'_, Page>) -> u32 {
    assert_eq!(front.register_class(0, PROCEDURE, 0, 0, b"Test"), 0xc000);
    front.create_window(&Create {
        ex_style: 0,
        class: b"Test",
        title: b"Test",
        style: 0x9000_0000,
        place: [0, 0, 0x280, 0x1e0],
        parent: 0,
        menu: 0,
        param: 0,
    })
}

/// One more window of the test class, as `style` and `place` say, its handle.
fn made(front: &Front<'_, Page>, style: u32, place: [i32; 4]) -> u32 {
    front.create_window(&Create {
        ex_style: 0,
        class: b"Test",
        title: b"Test",
        style,
        place,
        parent: 0,
        menu: 0,
        param: 0,
    })
}

/// `FindWindowA` with 2 windows of one class answers the newest, and the
/// older once the newest is gone.
#[test]
fn find_window_answers_the_newest_of_a_class() {
    let desk = Desk::new(Page::new(0x280, 0x1e0));
    let front = Front::new(&ROW, &desk);
    let first = window(&front);
    let second = made(&front, 0x8000_0000, [0, 0, 0x280, 0x1e0]);
    assert_ne!(first, second);
    assert_eq!(front.find_window(Some(&b"Test"[..]), None), second);
    assert_eq!(front.destroy_window(second), 1);
    assert_eq!(front.find_window(Some(&b"Test"[..]), None), first);
}

/// `PeekMessageA` with `PM_REMOVE` takes one message a call, oldest
/// first, and answers 0 with nothing written on an empty queue.
#[test]
fn peek_removes_one_a_call_and_answers_0_when_empty() {
    let desk = Desk::new(Page::new(0x280, 0x1e0));
    let front = Front::new(&ROW, &desk);
    desk.screen().frame(0x64);
    let mut taken = Vec::new();
    let lines = debug_lines(|| {
        assert_eq!(front.post_message(0, WM_USER, 1, 2), 1);
        assert_eq!(front.post_message(0, WM_USER + 1, 3, 4), 1);
        for _ in 0..3 {
            taken.push(front.peek_message(0, 0, 0, PM_REMOVE));
        }
    });
    let first = Message {
        message: WM_USER,
        w: 1,
        l: 2,
        time: 0x64,
        ..Message::default()
    };
    let second = Message {
        message: WM_USER + 1,
        w: 3,
        l: 4,
        ..first
    };
    assert_eq!(taken, [Some(first), Some(second), None]);
    assert_eq!(
        lines,
        [
            "imp.call c=USER32!PostMessageA a=0,400,1,2 v=1 by=window",
            "imp.call c=USER32!PostMessageA a=0,401,3,4 v=1 by=window",
            "imp.call c=USER32!PeekMessageA a=0,0,0,1 o=0,400,1,2,64,0,0 v=1 by=window",
            "imp.call c=USER32!PeekMessageA a=0,0,0,1 o=0,401,3,4,64,0,0 v=1 by=window",
            "imp.call c=USER32!PeekMessageA a=0,0,0,1 v=0 by=window",
        ]
    );
    assert!(desk.state().queue.is_empty());
}

/// `PeekMessageA` without `PM_REMOVE` answers the message and leaves it.
#[test]
fn peek_without_remove_leaves_the_message() {
    let desk = Desk::new(Page::new(0x280, 0x1e0));
    let front = Front::new(&ROW, &desk);
    assert_eq!(front.post_message(0, WM_USER, 0, 0), 1);
    let peeked = [
        front.peek_message(0, 0, 0, 0),
        front.peek_message(0, 0, 0, 0),
    ];
    assert_eq!(peeked[0], peeked[1]);
    assert_eq!(peeked[0].map(|message| message.message), Some(WM_USER));
    assert_eq!(desk.state().queue.len(), 1);
}

/// `WaitMessage` answers 1 at once on an empty queue: a page never waits.
#[test]
fn wait_answers_1_at_once() {
    let desk = Desk::new(Page::new(0x280, 0x1e0));
    let front = Front::new(&ROW, &desk);
    let lines = debug_lines(|| assert_eq!(front.wait_message(), 1));
    assert_eq!(lines, ["imp.call c=USER32!WaitMessage v=1 by=window"]);
    assert!(desk.state().queue.is_empty());
}

/// `PostQuitMessage` leaves `WM_QUIT` with its code, to the program, for
/// the loop to take.
#[test]
fn post_quit_leaves_wm_quit_with_its_code() {
    let desk = Desk::new(Page::new(0x280, 0x1e0));
    let front = Front::new(&ROW, &desk);
    let lines = debug_lines(|| front.post_quit_message(3));
    assert_eq!(
        lines,
        ["imp.call c=USER32!PostQuitMessage a=3 v=0 by=window"]
    );
    let quit = front.peek_message(0, 0, 0, PM_REMOVE);
    assert_eq!(
        quit.map(|quit| (quit.window, quit.message, quit.w)),
        Some((0, WM_QUIT, 3))
    );
}

/// A timer's message names no window and holds its procedure in `l`: USER32
/// calls that procedure. This front calls nothing and answers 0.
#[test]
#[ignore = "red by design: a timer's message with no window reaches no procedure here, a named difference not fixed here"]
fn a_timer_message_with_no_window_reaches_its_procedure() {
    let desk = Desk::new(Page::new(0x280, 0x1e0));
    let front = Front::new(&ROW, &desk);
    let timer = Message {
        window: 0,
        message: 0x113,
        w: 7,
        l: PROCEDURE,
        ..Message::default()
    };
    let mut called = Vec::new();
    let answer = front.dispatch_message(&timer, |procedure, _| {
        called.push(procedure);
        1
    });
    assert_eq!(called, [PROCEDURE]);
    assert_eq!(answer, 1);
}

/// `DispatchMessageA` hands the call its window's class's procedure and
/// answers what it answers; a window not here calls nothing and answers 0.
#[test]
fn dispatch_hands_the_class_procedure_and_0_for_a_window_not_here() {
    let desk = Desk::new(Page::new(0x280, 0x1e0));
    let front = Front::new(&ROW, &desk);
    let handle = window(&front);
    let mut called = Vec::new();
    let mut procedure = |procedure: u32, message: &Message| {
        called.push((procedure, message.window));
        7
    };
    let to = |window| Message {
        window,
        message: WM_USER,
        ..Message::default()
    };
    let lines = debug_lines(|| {
        assert_eq!(front.dispatch_message(&to(handle), &mut procedure), 7);
        assert_eq!(front.dispatch_message(&to(0x999), &mut procedure), 0);
    });
    assert_eq!(called, [(PROCEDURE, handle)]);
    assert_eq!(
        lines,
        [
            "imp.call c=USER32!DispatchMessageA a=10020,400,0,0,0,0,0 v=7 by=window",
            "imp.call c=USER32!DispatchMessageA a=999,400,0,0,0,0,0 v=0 by=window",
        ]
    );
}

/// A chosen item or button queues `WM_COMMAND`: its number in the low word,
/// the high word and `l` 0. It writes no line of its own and is no key
/// message, so `TranslateMessage` answers 0.
#[test]
fn a_command_queues_wm_command_and_writes_no_line_of_its_own() {
    let desk = Desk::new(Page::new(0x280, 0x1e0));
    let front = Front::new(&ROW, &desk);
    let handle = window(&front);
    desk.screen().frame(0x64);
    let mut taken = None;
    let lines = debug_lines(|| {
        desk.screen().hand(Event::Command {
            window: handle,
            id: 0xffff,
        });
        taken = front.peek_message(0, 0, 0, PM_REMOVE);
    });
    let command = Message {
        window: handle,
        message: WM_COMMAND,
        w: 0xffff,
        l: 0,
        time: 0x64,
        ..Message::default()
    };
    assert_eq!(taken, Some(command));
    assert_eq!(
        lines,
        ["imp.call c=USER32!PeekMessageA a=0,0,0,1 o=10020,111,ffff,0,64,0,0 v=1 by=window"]
    );
    assert_eq!(front.translate_message(&command), 0);
}

/// One whole loop on the page's back: the page hands in a close, the loop
/// takes `WM_CLOSE` and dispatches it, the procedure hands it to
/// `DefWindowProcA`, which destroys the window, and posts the quit; the
/// loop ends on `WM_QUIT`, the page's window gone. Bounded at 4 turns.
#[test]
fn a_whole_loop_ends_on_the_quit_a_close_brings() {
    let desk = Desk::new(Page::new(0x280, 0x1e0));
    let front = Front::new(&ROW, &desk);
    let handle = window(&front);
    let shown = Placed {
        window: handle,
        rect: Rect::sized(0, 0, 0x280, 0x1e0),
        shown: true,
    };
    assert_eq!(desk.screen().placed(handle), Some(shown));
    desk.screen().frame(0x64);
    desk.screen().hand(Event::Close { window: handle });
    let procedure = |procedure: u32, message: &Message| {
        assert_eq!(procedure, PROCEDURE);
        let answer = front.def_window_proc(message.window, message.message, message.w, message.l);
        if message.message == WM_CLOSE {
            front.post_quit_message(0);
        }
        answer
    };
    let mut ended = None;
    let lines = debug_lines(|| {
        for _ in 0..4 {
            assert_eq!(front.wait_message(), 1);
            let Some(message) = front.peek_message(0, 0, 0, PM_REMOVE) else {
                continue;
            };
            if message.message == WM_QUIT {
                ended = Some(message.w);
                break;
            }
            assert_eq!(front.translate_message(&message), 0);
            front.dispatch_message(&message, procedure);
        }
    });
    assert_eq!(ended, Some(0));
    assert_eq!(
        lines,
        [
            "imp.call c=USER32!WaitMessage v=1 by=window",
            "imp.call c=USER32!PeekMessageA a=0,0,0,1 o=10020,10,0,0,64,0,0 v=1 by=window",
            "imp.call c=USER32!TranslateMessage a=10020,10,0,0,64,0,0 v=0 by=window",
            "imp.call c=USER32!DefWindowProcA a=10020,10,0,0 v=0 by=window",
            "imp.call c=USER32!PostQuitMessage a=0 v=0 by=window",
            "imp.call c=USER32!DispatchMessageA a=10020,10,0,0,64,0,0 v=0 by=window",
            "imp.call c=USER32!WaitMessage v=1 by=window",
            "imp.call c=USER32!PeekMessageA a=0,0,0,1 o=0,12,0,0,64,0,0 v=1 by=window",
        ]
    );
    assert!(desk.state().window(handle).is_none());
    assert_eq!(desk.screen().placed(handle), None);
}
