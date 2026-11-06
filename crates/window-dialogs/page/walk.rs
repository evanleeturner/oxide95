// SPDX-License-Identifier: MIT
//! The thin wasm a page's walk runs: one planted program, its owner window, its
//! bar and its box, built in code so no template's bytes lie here and the walk
//! needs no player's disc. Every export is one call on `window_dialogs`; what a
//! program would do with an answer is `Program::procedure`, a stand-in closing the box.
//!
//! Text comes back by [`text_ptr`] and the length a `*_text` export answers: the
//! bytes are UTF-8, as `view`'s `lines` write them, and good until the next call.
#![expect(
    unsafe_code,
    reason = "`#[unsafe(no_mangle)]` is how a wasm export keeps its name"
)]

use core::cell::{Cell, RefCell};

use kernel_process::{plant, resource};
use window::desk::{Desk, Window};
use window::page::Page;
use window::{Event, Message, Rect};
use window_dialogs::front::Front;
use window_dialogs::table::{Calls, Row};
use window_dialogs::tables::Tables;
use window_dialogs::{Resources, plant as templates, view};

const INSTANCE: u32 = 0x7000;
const MENU: u32 = 7;
const BOX: u32 = 9;
const OWNER: u32 = 0x77;
const WM_COMMAND: u32 = 0x111;
const IDOK: u32 = 1;
const IDCANCEL: u32 = 2;
/// The answer of [`ended`] and [`picked`] with nothing yet to tell.
const NONE: u32 = u32::MAX;

static ROW: Row = Row {
    file: "planted",
    calls: &Calls {
        check_dlg_button: "CheckDlgButton",
        check_menu_item: "CheckMenuItem",
        dialog_box_param: "DialogBoxParamA",
        draw_menu_bar: "DrawMenuBar",
        enable_menu_item: "EnableMenuItem",
        end_dialog: "EndDialog",
        get_dlg_item: "GetDlgItem",
        is_dlg_button_checked: "IsDlgButtonChecked",
        load_menu: "LoadMenuA",
        set_menu: "SetMenu",
    },
};

struct Disc(Vec<u8>);

impl Resources for Disc {
    fn resource(&self, _: u32, kind: u32, id: u32) -> Option<&[u8]> {
        resource::find(&self.0, kind, id)
    }
}

fn disc() -> Disc {
    let menu = templates::menu(&[
        (0x10, 0, "&File"),
        (0x00, 0x10, "&Open"),
        (0x01, 0x11, "&Save"),
        (0x00, 0, ""),
        (0x88, 0x12, "&Keep"),
        (0x90, 0, "&Help"),
        (0x80, 0x13, "&About"),
    ]);
    let controls = [
        (1, 0x80, 0x5001_0001, "&OK"),
        (2, 0x80, 0x5001_0000, "Cancel"),
        (0xffff, 0x82, 0x5002_0001, "A planted label"),
        (0x30, 0x80, 0x5001_0003, "First choice"),
        (0x31, 0x80, 0x5001_0003, "Second choice"),
    ];
    let boxed = templates::dialog(0x90c8_00c0, "A planted box", &controls);
    Disc(plant::image(&[
        (resource::MENU, MENU, &menu),
        (resource::DIALOG, BOX, &boxed),
    ]))
}

struct Program {
    desk: Desk<Page>,
    tables: Tables,
    disc: Disc,
    picked: Cell<u32>,
    text: RefCell<String>,
}

impl Program {
    fn new() -> Self {
        let program = Self {
            desk: Desk::new(Page::new(640, 480)),
            tables: Tables::new(),
            disc: disc(),
            picked: Cell::new(NONE),
            text: RefCell::new(String::new()),
        };
        program.desk.state().windows.push(Window {
            handle: OWNER,
            class: Vec::new(),
            procedure: 0,
            rect: Rect::default(),
            style: 0,
            ex_style: 0,
            user: 0,
            longs: Vec::new(),
            title: Vec::new(),
            parent: 0,
            shown: true,
        });
        let front = program.front();
        let menu = front.load_menu(INSTANCE, MENU);
        assert_ne!(front.set_menu(OWNER, menu), 0, "invariant: planted");
        assert_ne!(front.draw_menu_bar(OWNER), 0, "invariant: planted");
        program
    }

    fn front(&self) -> Front<'_, Page, Disc> {
        Front::new(&ROW, &self.desk, &self.tables, &self.disc)
    }

    fn keep_text(&self, text: Option<String>) -> u32 {
        let text = text.unwrap_or_default();
        let length = u32::try_from(text.len()).expect("invariant: a planted view is small");
        *self.text.borrow_mut() = text;
        length
    }

    /// What a program's procedure would do: the bar's pick is kept, a box's OK or
    /// Cancel ends it with its id, and every other message is dropped.
    fn procedure(&self, message: &Message) {
        if message.message != WM_COMMAND {
            return;
        }
        let id = message.w & 0xffff;
        if message.window == OWNER {
            self.picked.set(id);
        } else if id == IDOK || id == IDCANCEL {
            assert_ne!(self.front().end_dialog(message.window, id), 0, "open");
        }
    }
}

thread_local! {
    static PROGRAM: &'static Program = Box::leak(Box::new(Program::new()));
}

fn with<T>(call: impl FnOnce(&Program) -> T) -> T {
    PROGRAM.with(|program| call(program))
}

/// Opens the box: its handle, the focus on its first control; 0 had it no template.
#[unsafe(no_mangle)]
pub extern "C" fn open_box() -> u32 {
    with(|program| {
        let opening = [INSTANCE, BOX, OWNER, 0, 0];
        if program.front().dialog_box_param(opening, |_, _| 1) != 0 {
            return 0;
        }
        let tables = program.tables.state();
        tables.boxes.last().map_or(0, |open| open.handle)
    })
}

/// The box `handle` as `BoxView::lines`' text: its length, 0 for a box not open.
#[unsafe(no_mangle)]
pub extern "C" fn box_text(handle: u32) -> u32 {
    with(|program| {
        let open = view::open_box(&program.desk, &program.tables, handle);
        program.keep_text(open.map(|found| found.lines()))
    })
}

/// The owner's bar as `BarView::lines`' text: its length.
#[unsafe(no_mangle)]
pub extern "C" fn bar_text() -> u32 {
    with(|program| {
        let found = view::bar(&program.tables, OWNER);
        program.keep_text(found.map(|bar| bar.lines()))
    })
}

/// Where the last `*_text` export left its bytes.
#[unsafe(no_mangle)]
pub extern "C" fn text_ptr() -> *const u8 {
    with(|program| program.text.borrow().as_ptr())
}

/// The player pressed control `id`: 1 taken, 0 for a box not open.
#[unsafe(no_mangle)]
pub extern "C" fn press(handle: u32, id: u32) -> u32 {
    let id = u16::try_from(id).unwrap_or_default();
    with(|program| u32::from(program.front().press(handle, id)))
}

/// A key down in the box, a Windows key code: 1 taken, 0 not.
#[unsafe(no_mangle)]
pub extern "C" fn key(handle: u32, code: u32) -> u32 {
    let code = u8::try_from(code).unwrap_or_default();
    with(|program| u32::from(program.front().key(handle, code)))
}

/// The browser moved the focus to `handle`, with no call a program made.
#[unsafe(no_mangle)]
pub extern "C" fn focus(handle: u32) {
    with(|program| program.desk.state().focus = handle);
}

/// The focus's handle as the desk holds it.
#[unsafe(no_mangle)]
pub extern "C" fn focused() -> u32 {
    with(|program| program.desk.state().focus)
}

/// The player picked the bar's row `id`: handed to the back as a command.
#[unsafe(no_mangle)]
pub extern "C" fn pick(id: u32) {
    let id = u16::try_from(id).unwrap_or_default();
    with(|program| {
        program
            .desk
            .screen()
            .hand(Event::Command { window: OWNER, id });
    });
}

/// The program's turn: every queued message goes to its procedure, oldest first.
#[unsafe(no_mangle)]
pub extern "C" fn run() {
    with(|program| {
        program.desk.pump();
        loop {
            let message = program.desk.state().queue.pop_front();
            let Some(message) = message else {
                break;
            };
            program.procedure(&message);
        }
    });
}

/// The last box's `EndDialog` value; `u32::MAX` for none closed yet.
#[unsafe(no_mangle)]
pub extern "C" fn ended() -> u32 {
    with(|program| {
        let tables = program.tables.state();
        tables.ended.last().map_or(NONE, |(_, value)| *value)
    })
}

/// The last row picked on the bar; `u32::MAX` for none yet.
#[unsafe(no_mangle)]
pub extern "C" fn picked() -> u32 {
    with(|program| program.picked.get())
}
