// SPDX-License-Identifier: MIT
//! Wine's own USER32 beside the crate's `Front`, on the same planted menu and box bytes, in one
//! 32-bit process. One flat line a row, `row=<name> user32=<answer> front=<answer>`, read by
//! `user32_check.py`, which sets the two sides apart. A handle prints by its kind alone.
//!
//! Built for `i686-pc-windows-gnu` under its own feature, so a native `cargo test --workspace`
//! links no `user32`. The mode `first <way>` asks whether a box closes with no key from outside.
#![allow(unsafe_code)]

use std::cell::{Cell, RefCell};
use std::fmt::Display;

use kernel_process::resource;
use window::desk::{Desk, Window};
use window::page::Page;
use window::{Event, Rect};
use window_dialogs::front::Front;
use window_dialogs::table::{Calls, Row};
use window_dialogs::tables::Tables;
use window_dialogs::template::{Item, find_item};
use window_dialogs::{Resources, plant};

const WM_INITDIALOG: u32 = 0x110;
const WM_COMMAND: u32 = 0x111;
const WM_KEYDOWN: u32 = 0x100;
const WM_KEYUP: u32 = 0x101;
const VK_RETURN: usize = 0x0d;
const IDOK: usize = 1;
/// A key up's `l`, `0xc000_0001`: one repeat, the key down before, the key released.
const KEY_UP_L: isize = -0x3fff_ffff;
const BOX_STYLE: u32 = 0x80c8_0080;
const DEFAULT: u32 = 0x5001_0001;
const AUTO: u32 = 0x5001_0003;
const ENDED: isize = 7;

type Procedure = extern "system" fn(usize, u32, usize, isize) -> isize;

#[link(name = "user32")]
unsafe extern "system" {
    fn DialogBoxIndirectParamA(
        instance: usize,
        template: *const u32,
        owner: usize,
        procedure: Procedure,
        param: isize,
    ) -> isize;
    fn EndDialog(window: usize, value: isize) -> i32;
    fn PostMessageA(window: usize, message: u32, w: usize, l: isize) -> i32;
    fn GetFocus() -> usize;
    fn SetFocus(window: usize) -> usize;
    fn GetDlgItem(window: usize, id: i32) -> usize;
}

#[link(name = "kernel32")]
unsafe extern "system" {
    fn GetModuleHandleA(name: *const u8) -> usize;
}

thread_local! {
    static WAY: Cell<u32> = const { Cell::new(0) };
    static INIT_W: Cell<usize> = const { Cell::new(0) };
}

/// A handle by its kind: `0`, `-1`, `same-as <name>` for one named, else `live`.
fn kind(handle: usize, named: &[(&str, usize)]) -> String {
    match handle {
        0 => "0".into(),
        usize::MAX => "-1".into(),
        _ => named
            .iter()
            .find(|(_, other)| *other == handle)
            .map_or_else(|| "live".into(), |(name, _)| format!("same-as-{name}")),
    }
}

/// The template's bytes on a 4-byte boundary, as USER32 reads a `DLGTEMPLATE`.
fn aligned(bytes: &[u8]) -> Vec<u32> {
    bytes
        .chunks(4)
        .map(|chunk| {
            let mut word = [0; 4];
            word[..chunk.len()].copy_from_slice(chunk);
            u32::from_le_bytes(word)
        })
        .collect()
}

/// The module `Front` reads its templates from, and the menu's number in it.
const INSTANCE: u32 = 0x7000;
const MENU_ID: u32 = 7;
const MF_BYPOSITION: u32 = 0x400;
/// The state bits a menu row compares: grayed, disabled, checked.
const STATE_BITS: u32 = 0x0b;
/// The planted menu both sides read, each row its options, id and words: a popup holding a
/// grayed item, a nested popup, a disabled and a plain item; a plain item; a last popup.
const MENU_ROWS: &[(u16, u16, &str)] = &[
    (0x10, 0, "File"),
    (0x01, 41, "Gray"),
    (0x10, 0, "Nested"),
    (0x08, 51, "Checked"),
    (0x80, 52, "Inner"),
    (0x02, 42, "Disabled"),
    (0x80, 43, "Plain"),
    (0x00, 45, "Top"),
    (0x90, 0, "Last"),
    (0x80, 44, "Tail"),
];
/// Each item a change names: the row's word for it, the item, by place or by id.
const TARGETS: &[(&str, u32, bool)] = &[
    ("id41", 41, false),
    ("id42", 42, false),
    ("id51", 51, false),
    ("id44", 44, false),
    ("id45", 45, false),
    ("id0", 0, false),
    ("id99", 99, false),
    ("place0", 0, true),
    ("place1", 1, true),
    ("place2", 2, true),
    ("place9", 9, true),
];
const FLAGS: [u32; 5] = [0, 1, 2, 3, 8];

#[link(name = "user32")]
unsafe extern "system" {
    fn LoadMenuIndirectA(template: *const u32) -> usize;
    fn DestroyMenu(menu: usize) -> i32;
    fn EnableMenuItem(menu: usize, item: u32, flags: u32) -> u32;
    fn CheckMenuItem(menu: usize, item: u32, flags: u32) -> u32;
    fn GetMenuState(menu: usize, item: u32, flags: u32) -> u32;
    fn GetMenuItemCount(menu: usize) -> i32;
    fn GetSubMenu(menu: usize, place: i32) -> usize;
    fn GetMenuItemID(menu: usize, place: i32) -> u32;
    fn GetMenuStringA(menu: usize, item: u32, words: *mut u8, room: i32, flags: u32) -> i32;
}

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

/// The planted file `Front` reads, one module, `INSTANCE`.
struct Disc(Vec<u8>);

impl Resources for Disc {
    fn resource(&self, instance: u32, kind: u32, id: u32) -> Option<&[u8]> {
        (instance == INSTANCE)
            .then(|| resource::find(&self.0, kind, id))
            .flatten()
    }
}

type Own<'d> = Front<'d, Page, Disc>;

/// One flat row: its name, USER32's answer, `Front`'s.
fn row(name: &str, user32: impl Display, front: impl Display) {
    println!("row={name} user32={user32} front={front}");
}

/// A number as a row prints it, -1 for all bits set.
fn number(value: u32) -> String {
    if value == u32::MAX {
        "-1".into()
    } else {
        value.to_string()
    }
}

/// A menu state's compared bits, -1 for an item not there.
fn bits(state: u32) -> String {
    if state == u32::MAX {
        "-1".into()
    } else {
        (state & STATE_BITS).to_string()
    }
}

fn shown(words: &str) -> &str {
    if words.is_empty() { "none" } else { words }
}

/// `Front`'s handle as wide as USER32's, so `kind` reads both.
fn wide(handle: u32) -> usize {
    usize::try_from(handle).unwrap_or(usize::MAX)
}

/// The menu's shape, place by place, through each popup: count, words, popup or not, id and
/// state by USER32's readers beside `Front`'s table.
fn shape(prefix: &str, user32: usize, items: Option<&[Item]>) {
    let items = items.unwrap_or_default();
    // SAFETY: `user32` is a menu this run loaded and has not destroyed, or 0, refused softly.
    let count = unsafe { GetMenuItemCount(user32) };
    row(&format!("{prefix}.count"), count, items.len());
    let places = usize::try_from(count).unwrap_or(0).max(items.len());
    for place in 0..places {
        let (at, index) = (u32::try_from(place), i32::try_from(place));
        let (at, index) = (at.unwrap_or(u32::MAX), index.unwrap_or(i32::MAX));
        let mut words = [0u8; 32];
        // SAFETY: as above; `words` holds 32 bytes and USER32 is told 32.
        let (length, sub, id, state) = unsafe {
            let length = GetMenuStringA(user32, at, words.as_mut_ptr(), 32, MF_BYPOSITION);
            let sub = GetSubMenu(user32, index);
            let state = GetMenuState(user32, at, MF_BYPOSITION);
            (length, sub, GetMenuItemID(user32, index), state)
        };
        let length = usize::try_from(length).unwrap_or(0).min(words.len());
        let theirs = String::from_utf8_lossy(&words[..length]).into_owned();
        let ours = items.get(place);
        let name = format!("{prefix}.{place}");
        let own_words = ours.map_or("none", |item| shown(&item.words));
        row(&format!("{name}.words"), shown(&theirs), own_words);
        let popup = ours.is_some_and(|item| item.popup);
        row(&format!("{name}.popup"), sub != 0, popup);
        if sub == 0 {
            let own_id = ours.map_or(u32::MAX, |item| u32::from(item.id));
            row(&format!("{name}.id"), number(id), number(own_id));
        }
        let own_state = ours.map_or(u32::MAX, |item| item.state);
        row(&format!("{name}.state"), bits(state), bits(own_state));
        if sub != 0 || popup {
            shape(&name, sub, ours.map(|item| &item.items[..]));
        }
    }
}

/// One change both sides make: `CheckMenuItem` for `check`, else `EnableMenuItem`.
fn change(check: bool, menus: (usize, u32), front: &Own<'_>, item: u32, flags: u32) -> (u32, u32) {
    let (user32, own) = menus;
    // SAFETY: `user32` is a menu this run loaded and has not destroyed; an item not there
    // answers -1.
    let theirs = unsafe {
        if check {
            CheckMenuItem(user32, item, flags)
        } else {
            EnableMenuItem(user32, item, flags)
        }
    };
    let ours = if check {
        front.check_menu_item(own, item, flags)
    } else {
        front.enable_menu_item(own, item, flags)
    };
    (theirs, ours)
}

/// `Front`'s item state as its own finder reads it, -1 for none.
fn state_of(tables: &Tables, own: u32, item: u32, by_place: bool) -> u32 {
    let mut state = tables.state();
    let menu = state.menu_mut(own);
    let found = menu.and_then(|menu| find_item(&mut menu.items, item, by_place));
    found.map_or(u32::MAX, |found| found.state)
}

/// The menu rows: the load and its shape, then each change on a fresh menu each side, called
/// twice so each old state reads back, and the item's state after.
fn menus() {
    let bytes = plant::menu(MENU_ROWS);
    let template = aligned(&bytes);
    let disc = Disc(kernel_process::plant::image(&[(
        resource::MENU,
        MENU_ID,
        &bytes,
    )]));
    let (desk, tables) = (Desk::new(Page::new(0x280, 0x1e0)), Tables::new());
    let front = Front::new(&ROW, &desk, &tables, &disc);
    let load = || {
        // SAFETY: the template is a whole planted menu on a 4-byte boundary, alive for the call.
        let user32 = unsafe { LoadMenuIndirectA(template.as_ptr()) };
        (user32, front.load_menu(INSTANCE, MENU_ID))
    };
    let (user32, own) = load();
    row("menu.load", kind(user32, &[]), kind(wide(own), &[]));
    let mut state = tables.state();
    shape(
        "menu.shape",
        user32,
        state.menu_mut(own).map(|menu| &menu.items[..]),
    );
    drop(state);
    for (check, call) in [(false, "enable"), (true, "check")] {
        for &(target, item, by_place) in TARGETS {
            for flags in FLAGS {
                let menus = load();
                let by = if by_place { MF_BYPOSITION } else { 0 };
                let name = format!("menu.{call}.{target}.f{flags}");
                for turn in 1..=2 {
                    let (theirs, ours) = change(check, menus, &front, item, flags | by);
                    row(&format!("{name}.{turn}"), number(theirs), number(ours));
                }
                // SAFETY: as at the load; the menu is destroyed after this read, never used again.
                let theirs = unsafe {
                    let state = GetMenuState(menus.0, item, by);
                    DestroyMenu(menus.0);
                    state
                };
                let ours = state_of(&tables, menus.1, item, by_place);
                row(&format!("{name}.state"), bits(theirs), bits(ours));
            }
        }
    }
}

#[link(name = "user32")]
unsafe extern "system" {
    fn LoadMenuA(instance: usize, name: *const u8) -> usize;
    fn SetMenu(window: usize, menu: usize) -> i32;
    fn GetMenu(window: usize) -> usize;
    fn DrawMenuBar(window: usize) -> i32;
    fn RegisterClassA(class: *const WndClassA) -> u16;
    #[allow(clippy::too_many_arguments)]
    fn CreateWindowExA(
        ex_style: u32,
        class: *const u8,
        title: *const u8,
        style: u32,
        x: i32,
        y: i32,
        width: i32,
        height: i32,
        parent: usize,
        menu: usize,
        instance: usize,
        param: *const u8,
    ) -> usize;
    fn DestroyWindow(window: usize) -> i32;
    fn DefWindowProcA(window: usize, message: u32, w: usize, l: isize) -> isize;
}

#[link(name = "kernel32")]
unsafe extern "system" {
    fn GetLastError() -> u32;
    fn SetLastError(code: u32);
}

/// USER32's `WNDCLASSA`, as a 32-bit process lays it out.
#[repr(C)]
struct WndClassA {
    style: u32,
    procedure: Procedure,
    class_extra: i32,
    window_extra: i32,
    instance: usize,
    icon: usize,
    cursor: usize,
    background: usize,
    menu_name: *const u8,
    class_name: *const u8,
}

/// The bar rows' windows on `Front`'s desk: a top window, a second with no bar, a child
/// control of the first; `GONE` is never placed, so it reads as a window gone.
const TOP: u32 = 0x77;
const BARE: u32 = 0x78;
const CHILD: u32 = 0x79;
const GONE: u32 = 0x7a;
/// A menu handle neither side ever handed out.
const NEVER: u32 = 0x7777;
const WS_CHILD: u32 = 0x4000_0000;
/// A menu number the module lacks.
const LACKING: u32 = 99;

/// The probe's own window procedure: USER32's default for every message.
extern "system" fn plain(window: usize, message: u32, w: usize, l: isize) -> isize {
    // SAFETY: USER32 calls this with its own live window and message, passed on unchanged.
    unsafe { DefWindowProcA(window, message, w, l) }
}

/// A USER32 call's answer and the error it left, the error word cleared before the call.
fn traced<T>(call: impl FnOnce() -> T) -> (T, u32) {
    // SAFETY: the error word is this thread's own.
    unsafe { SetLastError(0) };
    let answer = call();
    // SAFETY: as above.
    (answer, unsafe { GetLastError() })
}

/// A row with USER32's error word beside its answer: `Front` keeps none.
fn row_error(name: &str, user32: &(impl Display, u32), front: impl Display) {
    println!(
        "row={name} user32={} front={front} error={}",
        user32.0, user32.1
    );
}

fn desk_window(handle: u32, parent: u32, style: u32) -> Window {
    Window {
        handle,
        class: Vec::new(),
        procedure: 0,
        rect: Rect::default(),
        style,
        ex_style: 0,
        user: 0,
        longs: Vec::new(),
        title: Vec::new(),
        parent,
        shown: true,
    }
}

/// USER32's hidden windows for the bar rows: top, bare, child of top, and one destroyed.
fn user32_windows(instance: usize) -> [usize; 4] {
    let name = c"probe".as_ptr().cast::<u8>();
    let class = WndClassA {
        style: 0,
        procedure: plain,
        class_extra: 0,
        window_extra: 0,
        instance,
        icon: 0,
        cursor: 0,
        background: 0,
        menu_name: std::ptr::null(),
        class_name: name,
    };
    // SAFETY: the class and its name live across the call; the procedure is `extern "system"`.
    if unsafe { RegisterClassA(&raw const class) } == 0 {
        eprintln!("RegisterClassA refused");
        std::process::exit(4);
    }
    let make = |parent: usize, style: u32| {
        // SAFETY: the class is registered; its name and the empty title live across the call.
        unsafe {
            let title = c"".as_ptr().cast::<u8>();
            CreateWindowExA(
                0,
                name,
                title,
                style,
                0,
                0,
                64,
                48,
                parent,
                0,
                instance,
                std::ptr::null(),
            )
        }
    };
    let (top, bare, gone) = (make(0, 0), make(0, 0), make(0, 0));
    let child = make(top, WS_CHILD);
    // SAFETY: `gone` is a window this run made; it is never used but as a handle gone.
    if [top, bare, gone, child].contains(&0) || unsafe { DestroyWindow(gone) } == 0 {
        eprintln!("a window was refused");
        std::process::exit(4);
    }
    [top, bare, child, gone]
}

/// An extended menu template, version word 1: its header (the version, an offset of 4, a help
/// id of 0), then one plain item, id 0x41, words "A", the last. USER32 reads a version word of 1
/// in this form, so the planted template with only its version word changed is read past its end.
fn extended_menu() -> Vec<u8> {
    let mut bytes = Vec::new();
    bytes.extend_from_slice(&1u16.to_le_bytes());
    bytes.extend_from_slice(&4u16.to_le_bytes());
    bytes.extend_from_slice(&0u32.to_le_bytes());
    for word in [0u32, 0, 0x41] {
        bytes.extend_from_slice(&word.to_le_bytes());
    }
    bytes.extend_from_slice(&0x80u16.to_le_bytes());
    bytes.extend_from_slice(&[b'A', 0, 0, 0, 0, 0]);
    bytes
}

/// The load and bar rows: `LoadMenuA` on a menu lacking, a version word of 1 on a whole extended
/// template, a version word of 2 on the planted one, then `SetMenu` and `DrawMenuBar` on a top
/// window, one with no bar, a window gone and a control.
fn bars() {
    let bytes = plant::menu(MENU_ROWS);
    let template = aligned(&bytes);
    let extended = extended_menu();
    let extended_template = aligned(&extended);
    let mut version2 = bytes.clone();
    version2[0] = 2;
    let version2_template = aligned(&version2);
    let disc = Disc(kernel_process::plant::image(&[
        (resource::MENU, MENU_ID, &bytes),
        (resource::MENU, MENU_ID + 1, &extended),
        (resource::MENU, MENU_ID + 2, &version2),
    ]));
    let (desk, tables) = (Desk::new(Page::new(0x280, 0x1e0)), Tables::new());
    for (handle, parent, style) in [(TOP, 0, 0), (BARE, 0, 0), (CHILD, TOP, WS_CHILD)] {
        desk.state()
            .windows
            .push(desk_window(handle, parent, style));
    }
    let front = Front::new(&ROW, &desk, &tables, &disc);
    // SAFETY: a null name asks the exe's own module.
    let instance = unsafe { GetModuleHandleA(std::ptr::null()) };
    let lacking = std::ptr::without_provenance::<u8>(LACKING as usize);
    // SAFETY: a number in a name's place is a resource number, read and never written.
    let theirs = traced(|| kind(unsafe { LoadMenuA(instance, lacking) }, &[]));
    row_error(
        "bar.load.lacking",
        &theirs,
        kind(wide(front.load_menu(INSTANCE, LACKING)), &[]),
    );
    let theirs = traced(|| {
        // SAFETY: a whole extended template, on a 4-byte boundary, alive for the call.
        let menu = unsafe { LoadMenuIndirectA(extended_template.as_ptr()) };
        kind(menu, &[])
    });
    let ours = kind(wide(front.load_menu(INSTANCE, MENU_ID + 1)), &[]);
    row_error("bar.load.version1", &theirs, ours);
    let theirs = traced(|| {
        // SAFETY: a whole planted menu but its version word, on a 4-byte boundary, alive for the
        // call.
        let menu = unsafe { LoadMenuIndirectA(version2_template.as_ptr()) };
        kind(menu, &[])
    });
    let ours = kind(wide(front.load_menu(INSTANCE, MENU_ID + 2)), &[]);
    row_error("bar.load.version2", &theirs, ours);
    let [top, bare, child, gone] = user32_windows(instance);
    // SAFETY: as at `menus`' load.
    let menus = (
        unsafe { LoadMenuIndirectA(template.as_ptr()) },
        front.load_menu(INSTANCE, MENU_ID),
    );
    let read = |name: &str| {
        // SAFETY: `top` is this run's live window.
        let theirs = kind(unsafe { GetMenu(top) }, &[("menu", menus.0)]);
        let ours = tables.state().bar_mut(TOP).menu;
        row(name, theirs, kind(wide(ours), &[("menu", wide(menus.1))]));
    };
    let never = (wide(NEVER), NEVER);
    for (name, window, menu) in [
        ("bar.set.menu", (top, TOP), menus),
        ("bar.set.zero", (top, TOP), (0, 0)),
        ("bar.set.never", (top, TOP), never),
        ("bar.set.again", (top, TOP), menus),
        ("bar.set.gone", (gone, GONE), menus),
        ("bar.set.control", (child, CHILD), menus),
    ] {
        // SAFETY: each window is this run's, live or destroyed; a menu not there fails softly.
        let theirs = traced(|| unsafe { SetMenu(window.0, menu.0) });
        row_error(name, &theirs, front.set_menu(window.1, menu.1));
        read(&format!("{name}.read"));
    }
    for (name, window) in [
        ("with", (top, TOP)),
        ("none", (bare, BARE)),
        ("gone", (gone, GONE)),
    ] {
        // SAFETY: as above.
        let theirs = traced(|| unsafe { DrawMenuBar(window.0) });
        row_error(
            &format!("bar.draw.{name}"),
            &theirs,
            front.draw_menu_bar(window.1),
        );
    }
}

const DS_SETFONT: u32 = 0x40;
/// A label's style: a visible child, no tab stop.
const LABEL: u32 = 0x5000_0000;
/// The command a box's procedure posts itself at `WM_INITDIALOG`, to read the focus once the
/// box is up, then end it.
const MARKER: u16 = 0x99;
const PARAM: isize = 0x33;
/// `Front`'s planted boxes sit at this id and on, one a case, in the order of `CASES`.
const FIRST_BOX: u32 = 20;

#[link(name = "user32")]
unsafe extern "system" {
    fn DialogBoxParamA(
        instance: usize,
        name: *const u8,
        owner: usize,
        procedure: Procedure,
        param: isize,
    ) -> isize;
}

/// A box case's owner: the top window, none, or one gone.
#[derive(Clone, Copy)]
enum Owner {
    Top,
    Zero,
    Gone,
}

/// One box case: its row name, the box's style and controls, its owner, the value its
/// procedure ends it with, the procedure's answer to `WM_INITDIALOG`, and whether its template
/// is the extended form, which `template::dialog` refuses.
struct Case {
    name: &'static str,
    style: u32,
    controls: &'static [(u16, u16, u32, &'static str)],
    owner: Owner,
    value: i32,
    init: bool,
    extended: bool,
}

/// A default push whose words are an odd count with their end, so the next control's header
/// needs the reader's alignment, then an auto check.
const TWO: &[(u16, u16, u32, &str)] = &[(1, 0x80, DEFAULT, "Ok"), (11, 0x80, AUTO, "One")];

const fn case(
    name: &'static str,
    controls: &'static [(u16, u16, u32, &'static str)],
    value: i32,
) -> Case {
    Case {
        name,
        style: BOX_STYLE,
        controls,
        owner: Owner::Top,
        value,
        init: true,
        extended: false,
    }
}

const CASES: &[Case] = &[
    case("value0", TWO, 0),
    case("value7", TWO, 7),
    case("valueneg1", TWO, -1),
    Case {
        style: BOX_STYLE | DS_SETFONT,
        ..case("font", TWO, 7)
    },
    case("none", &[], 7),
    case("one", &[(1, 0x80, DEFAULT, "Ok")], 7),
    case(
        "label",
        &[(0xffff, 0x82, LABEL, "Label"), (1, 0x80, DEFAULT, "Ok")],
        7,
    ),
    Case {
        owner: Owner::Zero,
        ..case("owner0", TWO, 7)
    },
    Case {
        owner: Owner::Gone,
        ..case("ownergone", TWO, 7)
    },
    Case {
        init: false,
        ..case("answer0", TWO, 7)
    },
    Case {
        extended: true,
        ..case("extended", &[], 7)
    },
];

thread_local! {
    /// The box case's value and `WM_INITDIALOG` answer, for `boxed`.
    static CASE: Cell<(isize, isize)> = const { Cell::new((0, 0)) };
    static IDS: RefCell<Vec<u16>> = const { RefCell::new(Vec::new()) };
    static OWNER_NOW: Cell<usize> = const { Cell::new(0) };
    /// What `boxed` read, in order: `w`, `l`, the focus, `EndDialog`'s answer.
    static SEEN: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
}

/// A handle by its kind, against names built at the read.
fn named(handle: usize, names: &[(String, usize)]) -> String {
    let names: Vec<(&str, usize)> = names
        .iter()
        .map(|(name, other)| (name.as_str(), *other))
        .collect();
    kind(handle, &names)
}

/// The names a box row reads a handle by under USER32: each control by its id, the box, the owner.
fn user32_names(window: usize) -> Vec<(String, usize)> {
    let mut names: Vec<(String, usize)> = IDS.with_borrow(|ids| {
        ids.iter()
            .map(|&id| {
                // SAFETY: `window` is the open box; `GetDlgItem` answers 0 for an id it lacks.
                let handle = unsafe { GetDlgItem(window, i32::from(id)) };
                (format!("id{id}"), handle)
            })
            .collect()
    });
    names.push(("box".into(), window));
    names.push(("owner".into(), OWNER_NOW.get()));
    names
}

/// The same names under `Front`.
fn front_names(front: &Own<'_>, window: u32, ids: &[u16], owner: u32) -> Vec<(String, usize)> {
    let mut names: Vec<(String, usize)> = ids
        .iter()
        .map(|&id| {
            let handle = front.get_dlg_item(window, u32::from(id));
            (format!("id{id}"), wide(handle))
        })
        .collect();
    names.push(("box".into(), wide(window)));
    names.push(("owner".into(), wide(owner)));
    names
}

/// The box rows' procedure under USER32: at `WM_INITDIALOG` it reads `w` and `l` and posts
/// itself the marker, answering the case's word; at the marker it reads the focus and ends the
/// box with the case's value.
extern "system" fn boxed(window: usize, message: u32, w: usize, l: isize) -> isize {
    let (value, init) = CASE.get();
    match message {
        WM_INITDIALOG => {
            let names = user32_names(window);
            SEEN.with_borrow_mut(|seen| {
                seen.push(named(w, &names));
                seen.push(l.to_string());
            });
            // SAFETY: `window` is the box USER32 is opening, live for this call; a post takes
            // any handle and fails softly.
            unsafe { PostMessageA(window, WM_COMMAND, usize::from(MARKER), 0) };
            init
        }
        WM_COMMAND if w & 0xffff == usize::from(MARKER) => {
            let names = user32_names(window);
            // SAFETY: the box is open while its procedure runs; the focus call takes nothing.
            let focus = unsafe { GetFocus() };
            // SAFETY: as above, the box open.
            let ended = unsafe { EndDialog(window, value) };
            SEEN.with_borrow_mut(|seen| {
                seen.push(named(focus, &names));
                seen.push(u8::from(ended != 0).to_string());
            });
            1
        }
        _ => 0,
    }
}

/// USER32's side of a box case: `DialogBoxIndirectParamA`'s answer and error word, then what
/// `boxed` read, `none` for each read the box never reached.
fn user32_box(
    case: &Case,
    template: &[u32],
    instance: usize,
    owner: usize,
) -> ((isize, u32), Vec<String>) {
    CASE.set((
        isize::try_from(case.value).unwrap_or_default(),
        isize::from(case.init),
    ));
    IDS.set(case.controls.iter().map(|control| control.0).collect());
    OWNER_NOW.set(owner);
    SEEN.set(Vec::new());
    let answer = traced(|| {
        // SAFETY: the template lives across the call, a whole planted template on a 4-byte
        // boundary; the procedure is `extern "system"`; an owner gone fails softly.
        unsafe { DialogBoxIndirectParamA(instance, template.as_ptr(), owner, boxed, PARAM) }
    });
    let mut seen = SEEN.take();
    seen.resize(4, "none".into());
    (answer, seen)
}

/// `Front`'s side of a box case, over a fresh desk whose top window holds the focus: the
/// wait's answer, then the same reads as `boxed`'s.
fn front_box(disc: &Disc, case: &Case, id: u32) -> (u32, Vec<String>) {
    let (desk, tables) = (Desk::new(Page::new(0x280, 0x1e0)), Tables::new());
    desk.state().windows.push(desk_window(TOP, 0, 0));
    desk.state().focus = TOP;
    let front = Front::new(&ROW, &desk, &tables, disc);
    let owner = match case.owner {
        Owner::Top => TOP,
        Owner::Zero => 0,
        Owner::Gone => GONE,
    };
    let ids: Vec<u16> = case.controls.iter().map(|control| control.0).collect();
    let value = u32::from_ne_bytes(case.value.to_ne_bytes());
    let mut seen = Vec::new();
    let answer = front.dialog_box_wait([INSTANCE, id, owner, 0x50, 0x33], |_, sent| {
        let names = front_names(&front, sent.window, &ids, owner);
        match sent.message {
            WM_INITDIALOG => {
                seen.push(named(wide(sent.w), &names));
                seen.push(sent.l.to_string());
                let window = sent.window;
                desk.screen().hand(Event::Command { window, id: MARKER });
                u32::from(case.init)
            }
            WM_COMMAND if sent.w & 0xffff == u32::from(MARKER) => {
                let focus = desk.state().focus;
                seen.push(named(wide(focus), &names));
                seen.push(front.end_dialog(sent.window, value).to_string());
                0
            }
            _ => 0,
        }
    });
    seen.resize(4, "none".into());
    (answer, seen)
}

/// An extended box template, no controls: the version word 1 and `0xffff`, a help id, an
/// extended style and the style, then a count of 0, the place, and no menu, class or title.
fn extended_box() -> Vec<u8> {
    let mut out = Vec::new();
    for word in [1u16, 0xffff] {
        out.extend_from_slice(&word.to_le_bytes());
    }
    for long in [0u32, 0, BOX_STYLE] {
        out.extend_from_slice(&long.to_le_bytes());
    }
    for word in [0u16, 0, 0, 10, 10, 0, 0, 0] {
        out.extend_from_slice(&word.to_le_bytes());
    }
    out
}

/// The box rows: each case's answer, `WM_INITDIALOG`'s `w` and `l`, the focus once the box is
/// up, `EndDialog`'s answer, both sides; then `DialogBoxParamA` on an id the module lacks.
fn boxes() {
    let bytes: Vec<Vec<u8>> = CASES
        .iter()
        .map(|case| {
            if case.extended {
                extended_box()
            } else {
                plant::dialog(case.style, "Box", case.controls)
            }
        })
        .collect();
    let entries: Vec<(u32, u32, &[u8])> = (FIRST_BOX..)
        .zip(&bytes)
        .map(|(id, bytes)| (resource::DIALOG, id, &bytes[..]))
        .collect();
    let disc = Disc(kernel_process::plant::image(&entries));
    // SAFETY: a null name asks the exe's own module.
    let instance = unsafe { GetModuleHandleA(std::ptr::null()) };
    let windows = user32_windows(instance);
    for ((case, bytes), id) in CASES.iter().zip(&bytes).zip(FIRST_BOX..) {
        let owner = match case.owner {
            Owner::Top => windows[0],
            Owner::Zero => 0,
            Owner::Gone => windows[3],
        };
        let (theirs, seen) = user32_box(case, &aligned(bytes), instance, owner);
        let (ours, own_seen) = front_box(&disc, case, id);
        let name = case.name;
        row_error(&format!("box.{name}.answer"), &theirs, number(ours));
        let reads = ["init_w", "init_l", "focus", "ended"];
        for (read, (user32, front)) in reads.iter().zip(seen.iter().zip(&own_seen)) {
            row(&format!("box.{name}.{read}"), user32, front);
        }
    }
    let theirs = traced(|| {
        let name = std::ptr::without_provenance(usize::try_from(LACKING).unwrap_or_default());
        // SAFETY: a resource name by its number, which the module lacks; the procedure is
        // `extern "system"`.
        unsafe { DialogBoxParamA(instance, name, windows[0], boxed, PARAM) }
    });
    let (desk, tables) = (Desk::new(Page::new(0x280, 0x1e0)), Tables::new());
    let front = Front::new(&ROW, &desk, &tables, &disc);
    let ours = front.dialog_box_wait([INSTANCE, LACKING, TOP, 0x50, 0x33], |_, _| 0);
    row_error("box.lacking.answer", &theirs, number(ours));
}

const THREE: u32 = 0x5001_0005;
const PUSH: u32 = 0x5001_0000;
const FIELD: u32 = 0x5081_0000;
const CHOICE: u32 = 0x5001_0003;
/// The procedure rows' box: a default push, an auto check, a 3-state, a push, a label, then one
/// control of each class ordinal left, `EDIT` to `COMBOBOX`.
const INSIDE: &[(u16, u16, u32, &str)] = &[
    (1, 0x80, DEFAULT, "Ok"),
    (11, 0x80, AUTO, "One"),
    (12, 0x80, THREE, "Three"),
    (13, 0x80, PUSH, "Push"),
    (14, 0x82, LABEL, "Words"),
    (15, 0x81, FIELD, "Text"),
    (16, 0x83, FIELD, "List"),
    (17, 0x84, LABEL, "Bar"),
    (18, 0x85, CHOICE, "Combo"),
];
/// An id no control holds.
const ABSENT: u16 = 99;
/// The controls `CheckDlgButton` is read on: an auto check, a 3-state, a push, a label, none.
const CHECKED: [u16; 5] = [11, 12, 13, 14, ABSENT];
const INSIDE_BOX: u32 = 40;
/// A box whose one control names a class ordinal past `COMBOBOX`.
const UNKNOWN_BOX: u32 = 41;

#[link(name = "user32")]
unsafe extern "system" {
    fn GetClassNameA(window: usize, name: *mut u8, room: i32) -> i32;
    fn GetWindowTextA(window: usize, words: *mut u8, room: i32) -> i32;
    fn GetDlgCtrlID(window: usize) -> i32;
    fn CheckDlgButton(window: usize, id: i32, check: u32) -> i32;
    fn IsDlgButtonChecked(window: usize, id: i32) -> u32;
}

thread_local! {
    /// The procedure rows `inner` read, and the box it read them in.
    static INSIDE_SEEN: RefCell<Vec<(String, String)>> = const { RefCell::new(Vec::new()) };
    static BOX_NOW: Cell<usize> = const { Cell::new(0) };
}

/// One side's calls inside an open box, each answer as a row prints it.
trait Inside {
    fn item(&self, id: u16) -> usize;
    fn class(&self, handle: usize) -> String;
    fn words(&self, handle: usize) -> String;
    fn control_id(&self, handle: usize) -> String;
    fn check(&self, id: u16, check: u32) -> String;
    fn checked(&self, id: u16) -> String;
    fn end(&self) -> String;
}

/// The words a USER32 reader wrote.
fn written(buffer: &[u8], length: i32) -> String {
    let length = usize::try_from(length).unwrap_or(0).min(buffer.len());
    String::from_utf8_lossy(&buffer[..length]).into()
}

/// USER32 inside the box open as this handle.
struct Theirs(usize);

impl Inside for Theirs {
    fn item(&self, id: u16) -> usize {
        // SAFETY: the box is open while its procedure runs; an id it lacks answers 0.
        unsafe { GetDlgItem(self.0, i32::from(id)) }
    }

    /// Upper case: Windows matches a class name without case, and the crate keeps the
    /// template's own spelling.
    fn class(&self, handle: usize) -> String {
        let mut name = [0u8; 64];
        // SAFETY: the room is the buffer's length; a handle of 0 writes nothing.
        let length = unsafe { GetClassNameA(handle, name.as_mut_ptr(), 64) };
        shown(&written(&name, length).to_ascii_uppercase()).into()
    }

    fn words(&self, handle: usize) -> String {
        let mut words = [0u8; 64];
        // SAFETY: as above.
        let length = unsafe { GetWindowTextA(handle, words.as_mut_ptr(), 64) };
        shown(&written(&words, length)).into()
    }

    fn control_id(&self, handle: usize) -> String {
        // SAFETY: any handle; one not a window answers 0.
        unsafe { GetDlgCtrlID(handle) }.to_string()
    }

    fn check(&self, id: u16, check: u32) -> String {
        // SAFETY: the box is open; an id it lacks fails softly.
        unsafe { CheckDlgButton(self.0, i32::from(id), check) }.to_string()
    }

    fn checked(&self, id: u16) -> String {
        // SAFETY: as above.
        number(unsafe { IsDlgButtonChecked(self.0, i32::from(id)) })
    }

    fn end(&self) -> String {
        // SAFETY: as above.
        u8::from(unsafe { EndDialog(self.0, ENDED) } != 0).to_string()
    }
}

/// `Front` inside the box open as `window`, its rows read from the desk and the tables.
struct Ours<'a, 'd> {
    front: &'a Own<'d>,
    desk: &'a Desk<Page>,
    tables: &'a Tables,
    window: u32,
}

impl Ours<'_, '_> {
    /// The control row holding `handle` in any box open: its id and words.
    fn control(&self, handle: usize) -> Option<(u16, String)> {
        let state = self.tables.state();
        let mut controls = state.boxes.iter().flat_map(|open| &open.controls);
        let found = controls.find(|control| wide(control.handle) == handle)?;
        Some((found.id, found.words.clone()))
    }
}

impl Inside for Ours<'_, '_> {
    fn item(&self, id: u16) -> usize {
        wide(self.front.get_dlg_item(self.window, u32::from(id)))
    }

    fn class(&self, handle: usize) -> String {
        let state = self.desk.state();
        let found = state.windows.iter().find(|row| wide(row.handle) == handle);
        found.map_or_else(
            || "none".into(),
            |row| shown(&String::from_utf8_lossy(&row.class).to_ascii_uppercase()).into(),
        )
    }

    fn words(&self, handle: usize) -> String {
        self.control(handle)
            .map_or_else(|| "none".into(), |(_, words)| shown(&words).into())
    }

    fn control_id(&self, handle: usize) -> String {
        self.control(handle).map_or(0, |(id, _)| id).to_string()
    }

    fn check(&self, id: u16, check: u32) -> String {
        let answer = self
            .front
            .check_dlg_button(self.window, u32::from(id), check);
        answer.to_string()
    }

    fn checked(&self, id: u16) -> String {
        let answer = self.front.is_dlg_button_checked(self.window, u32::from(id));
        number(answer)
    }

    fn end(&self) -> String {
        self.front.end_dialog(self.window, 7).to_string()
    }
}

/// The procedure rows, in one order for both sides: each control's handle, class, words and id,
/// then an id absent's handle alone, as USER32's readers on no window answer nothing a row can
/// judge; `CheckDlgButton` 0 to 3 on each of `CHECKED`, `IsDlgButtonChecked` after each; then the
/// auto check set, `EndDialog`, and what answers after it while the procedure runs.
fn inside_reads(side: &impl Inside) -> Vec<(String, String)> {
    let mut out = Vec::new();
    for id in INSIDE.iter().map(|control| control.0) {
        let handle = side.item(id);
        out.push((format!("inside.id{id}.item"), kind(handle, &[])));
        out.push((format!("inside.id{id}.class"), side.class(handle)));
        out.push((format!("inside.id{id}.words"), side.words(handle)));
        out.push((format!("inside.id{id}.ctrl_id"), side.control_id(handle)));
    }
    let absent = kind(side.item(ABSENT), &[]);
    out.push((format!("inside.id{ABSENT}.item"), absent));
    for id in CHECKED {
        for check in 0..4 {
            out.push((
                format!("inside.check.id{id}.{check}"),
                side.check(id, check),
            ));
            out.push((format!("inside.checked.id{id}.{check}"), side.checked(id)));
        }
    }
    out.push(("inside.check.id11.set".into(), side.check(11, 1)));
    out.push(("inside.end".into(), side.end()));
    out.push(("inside.after_end.item".into(), kind(side.item(1), &[])));
    out.push(("inside.after_end.checked".into(), side.checked(11)));
    out.push(("inside.after_end.end".into(), side.end()));
    out
}

/// The procedure rows' procedure under USER32: at `WM_INITDIALOG` it posts itself the marker; at
/// the marker it reads the rows, which end the box.
extern "system" fn inner(window: usize, message: u32, w: usize, _l: isize) -> isize {
    match message {
        WM_INITDIALOG => {
            // SAFETY: `window` is the box USER32 is opening, live for this call.
            unsafe { PostMessageA(window, WM_COMMAND, usize::from(MARKER), 0) };
            1
        }
        WM_COMMAND if w & 0xffff == usize::from(MARKER) => {
            BOX_NOW.set(window);
            INSIDE_SEEN.set(inside_reads(&Theirs(window)));
            1
        }
        _ => 0,
    }
}

/// `Front`'s side of the procedure rows: the wait's answer, the rows, then `GetDlgItem` and
/// `EndDialog` on the box once closed.
fn front_inside(disc: &Disc) -> (u32, Vec<(String, String)>, (u32, u32)) {
    let (desk, tables) = (Desk::new(Page::new(0x280, 0x1e0)), Tables::new());
    desk.state().windows.push(desk_window(TOP, 0, 0));
    desk.state().focus = TOP;
    let front = Front::new(&ROW, &desk, &tables, disc);
    let (mut seen, mut opened) = (Vec::new(), 0);
    let answer = front.dialog_box_wait([INSTANCE, INSIDE_BOX, 0, 0x50, 0x33], |_, sent| match sent
        .message
    {
        WM_INITDIALOG => {
            let window = sent.window;
            desk.screen().hand(Event::Command { window, id: MARKER });
            1
        }
        WM_COMMAND if sent.w & 0xffff == u32::from(MARKER) => {
            opened = sent.window;
            let side = Ours {
                front: &front,
                desk: &desk,
                tables: &tables,
                window: opened,
            };
            seen = inside_reads(&side);
            0
        }
        _ => 0,
    });
    let closed = (front.get_dlg_item(opened, 1), front.end_dialog(opened, 0));
    (answer, seen, closed)
}

/// The procedure rows: the box's answer, the reads inside it by name, the box once closed, then
/// the box holding an unknown class ordinal.
fn inside() {
    let bytes = [
        plant::dialog(BOX_STYLE, "Box", INSIDE),
        plant::dialog(BOX_STYLE, "Box", &[(1, 0x86, PUSH, "Odd")]),
    ];
    let entries = [
        (resource::DIALOG, INSIDE_BOX, &bytes[0][..]),
        (resource::DIALOG, UNKNOWN_BOX, &bytes[1][..]),
    ];
    let disc = Disc(kernel_process::plant::image(&entries));
    // SAFETY: a null name asks the exe's own module.
    let instance = unsafe { GetModuleHandleA(std::ptr::null()) };
    let template = aligned(&bytes[0]);
    let theirs = traced(|| {
        // SAFETY: the template lives across the call, a whole planted template on a 4-byte
        // boundary; the procedure is `extern "system"`.
        unsafe { DialogBoxIndirectParamA(instance, template.as_ptr(), 0, inner, PARAM) }
    });
    let opened = BOX_NOW.get();
    // SAFETY: the box is closed; both calls take a handle gone and fail softly.
    let closed = unsafe { (GetDlgItem(opened, 1), EndDialog(opened, 0)) };
    let reads = INSIDE_SEEN.take();
    let (ours, own_reads, own_closed) = front_inside(&disc);
    row_error("inside.answer", &theirs, number(ours));
    for at in 0..reads.len().max(own_reads.len()) {
        let name = reads.get(at).or_else(|| own_reads.get(at));
        let name = name.map_or("", |read| read.0.as_str());
        let answer =
            |list: &[(String, String)]| list.get(at).map_or("none", |read| &read.1).to_owned();
        row(name, answer(&reads), answer(&own_reads));
    }
    let own_item = kind(wide(own_closed.0), &[]);
    row("inside.closed.item", kind(closed.0, &[]), own_item);
    row("inside.closed.end", u8::from(closed.1 != 0), own_closed.1);
    let template = aligned(&bytes[1]);
    WAY.set(1);
    let theirs = traced(|| {
        // SAFETY: as above; `first` at way 1 ends the box in `WM_INITDIALOG`.
        unsafe { DialogBoxIndirectParamA(instance, template.as_ptr(), 0, first, PARAM) }
    });
    let (desk, tables) = (Desk::new(Page::new(0x280, 0x1e0)), Tables::new());
    let front = Front::new(&ROW, &desk, &tables, &disc);
    let ours = front.dialog_box_wait([INSTANCE, UNKNOWN_BOX, 0, 0x50, 0x33], |_, sent| {
        if sent.message == WM_INITDIALOG {
            let _ = front.end_dialog(sent.window, 7);
        }
        0
    });
    row_error("inside.unknown.answer", &theirs, number(ours));
}

/// The first read's procedure: way 0 never ends its box, way 1 ends it in `WM_INITDIALOG`,
/// way 2 posts itself `IDOK`, way 3 posts Return's down and up to the control the focus goes to.
extern "system" fn first(window: usize, message: u32, w: usize, l: isize) -> isize {
    let way = WAY.get();
    match message {
        WM_INITDIALOG => {
            INIT_W.set(w);
            // SAFETY: `window` is the box USER32 is opening and `w` its first tab stop,
            // both live for this call; posting and ending take any handle and fail softly.
            unsafe {
                match way {
                    1 => {
                        EndDialog(window, ENDED);
                    }
                    2 => {
                        PostMessageA(window, WM_COMMAND, IDOK, 0);
                    }
                    3 => {
                        PostMessageA(w, WM_KEYDOWN, VK_RETURN, 1);
                        PostMessageA(w, WM_KEYUP, VK_RETURN, KEY_UP_L);
                    }
                    _ => {}
                }
            }
            1
        }
        WM_COMMAND => {
            let init = INIT_W.get();
            // SAFETY: `window` is the open box, live while its procedure runs; the focus
            // calls take any handle.
            let (before, other, set, after) = unsafe {
                let before = GetFocus();
                let other = GetDlgItem(window, 11);
                let set = SetFocus(other);
                (before, other, set, GetFocus())
            };
            let named = [("init", init), ("other", other)];
            println!(
                "first way={way} command_id={} command_code={} command_l={} focus_before={} \
                 set_answer={} focus_after={}",
                w & 0xffff,
                w >> 16,
                kind(usize::from_ne_bytes(l.to_ne_bytes()), &named),
                kind(before, &named),
                kind(set, &named),
                kind(after, &named),
            );
            // SAFETY: as above, the box open.
            unsafe { EndDialog(window, ENDED) };
            1
        }
        _ => 0,
    }
}

fn first_read(way: u32) {
    WAY.set(way);
    let bytes = plant::dialog(
        BOX_STYLE,
        "First",
        &[(1, 0x80, DEFAULT, "Yes"), (11, 0x80, AUTO, "One")],
    );
    let template = aligned(&bytes);
    // SAFETY: a null name asks the exe's own module; the template lives across the call and
    // is a whole `DLGTEMPLATE`, planted, on a 4-byte boundary; the procedure is `extern "system"`.
    let answer = unsafe {
        let instance = GetModuleHandleA(std::ptr::null());
        DialogBoxIndirectParamA(instance, template.as_ptr(), 0, first, 0x33)
    };
    println!(
        "first way={way} answer={answer} init_w={}",
        kind(INIT_W.get(), &[])
    );
}

#[link(name = "user32")]
unsafe extern "system" {
    fn SendMessageA(window: usize, message: u32, w: usize, l: isize) -> isize;
}

/// A box's own way to move its focus, `l` 1 naming the control by `w`: under USER32 a push
/// that takes the focus this way takes the default with it.
const WM_NEXTDLGCTL: u32 = 0x28;
const RETURN: u8 = 0x0d;
const TAB: u8 = 0x09;
const ESCAPE: u8 = 0x1b;
const SPACE: u8 = 0x20;
const PLAIN_CHECK: u32 = 0x5001_0002;
const RADIO: u32 = 0x5001_0009;
/// A push with `WS_DISABLED`, and one without `WS_VISIBLE`, each a tab stop.
const OFF: u32 = 0x5801_0000;
const HIDDEN: u32 = 0x4001_0000;
/// The commands the keys rows' procedure posts itself: once the box is up, after each key, and
/// after that step, so a command queued behind the step still arrives before the reads.
const GO: u16 = 0x9a;
const STEP: u16 = 0x9b;
const SETTLE: u16 = 0x9c;
/// `Front`'s keys boxes: the full one at this id, the bare one after it.
const KEYS_BOX: u32 = 50;
/// The controls whose check each keys case reads at its end.
const CHECKS: [u16; 3] = [11, 15, 16];

/// A default push, a label, a push, a disabled push, an auto check, a plain check, an auto
/// radio, a hidden push and an `IDCANCEL` push; every one a tab stop but the label.
const KEYS_FULL: &[(u16, u16, u32, &str)] = &[
    (1, 0x80, DEFAULT, "Ok"),
    (14, 0x82, LABEL, "Name"),
    (13, 0x80, PUSH, "Go"),
    (17, 0x80, OFF, "Off"),
    (11, 0x80, AUTO, "One"),
    (15, 0x80, PLAIN_CHECK, "Two"),
    (16, 0x80, RADIO, "Pick"),
    (18, 0x80, HIDDEN, "Hid"),
    (2, 0x80, PUSH, "Cancel"),
];
/// No default push and no `IDCANCEL`.
const KEYS_BARE: &[(u16, u16, u32, &str)] = &[(13, 0x80, PUSH, "Go"), (11, 0x80, AUTO, "One")];

/// One keys case: its row name, the bare box or the full, the control the focus starts on (0
/// for the box's own choice), and its keys, each a down and an up, or the down alone if false.
struct KeyCase {
    name: &'static str,
    bare: bool,
    start: u16,
    keys: &'static [(u8, bool)],
}

const fn keys_case(name: &'static str, bare: bool, start: u16, key: u8) -> KeyCase {
    KeyCase {
        name,
        bare,
        start,
        keys: match key {
            RETURN => &[(RETURN, true)],
            ESCAPE => &[(ESCAPE, true)],
            _ => &[(SPACE, true)],
        },
    }
}

const KEY_CASES: &[KeyCase] = &[
    keys_case("return", false, 0, RETURN),
    keys_case("returnother", false, 13, RETURN),
    keys_case("returnnone", true, 0, RETURN),
    keys_case("escape", false, 0, ESCAPE),
    keys_case("escapenone", true, 0, ESCAPE),
    KeyCase {
        keys: &[(TAB, true); 8],
        ..keys_case("tab", false, 1, TAB)
    },
    keys_case("spaceauto", false, 11, SPACE),
    keys_case("spaceplain", false, 15, SPACE),
    keys_case("spaceradio", false, 16, SPACE),
    keys_case("spacepush", false, 13, SPACE),
    KeyCase {
        keys: &[(SPACE, false)],
        ..keys_case("spacedown", false, 11, SPACE)
    },
];

thread_local! {
    /// The keys case under way, its next key's place, the commands heard since the last step,
    /// and each read by its row's last words.
    static KEYS_NOW: Cell<usize> = const { Cell::new(0) };
    static KEY_PLACE: Cell<usize> = const { Cell::new(0) };
    static HEARD: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
    static READS: RefCell<Vec<(String, String)>> = const { RefCell::new(Vec::new()) };
}

/// A command the procedure heard: its id, its code, its `l` by kind, and the control's check
/// as the procedure reads it then.
fn heard(w: u32, l: &str, checked: u32) -> String {
    format!("{}/{}/{l}/{checked}", w & 0xffff, w >> 16)
}

/// One step's reads: the focus after its key, and every command heard since the step before.
fn step_reads(reads: &mut Vec<(String, String)>, step: usize, focus: String, heard: &[String]) {
    reads.push((format!("{step}.focus"), focus));
    let heard = if heard.is_empty() {
        "none".into()
    } else {
        heard.join(",")
    };
    reads.push((format!("{step}.commands"), heard));
}

/// Posts the next key of the case under way to the focus, or to the box when nothing holds it,
/// then the step marker; false once no key is left.
fn user32_post_key(window: usize) -> bool {
    let place = KEY_PLACE.get();
    let Some(&(key, both)) = KEY_CASES[KEYS_NOW.get()].keys.get(place) else {
        return false;
    };
    KEY_PLACE.set(place + 1);
    // SAFETY: the box is open while its procedure runs; a post takes any handle and fails softly.
    unsafe {
        let focus = GetFocus();
        let target = if focus == 0 { window } else { focus };
        PostMessageA(target, WM_KEYDOWN, usize::from(key), 1);
        if both {
            PostMessageA(target, WM_KEYUP, usize::from(key), KEY_UP_L);
        }
        PostMessageA(window, WM_COMMAND, usize::from(STEP), 0);
    }
    true
}

/// The keys rows' procedure under USER32: up, it moves the focus to the case's start and posts
/// the first key; at each step it counts the commands heard so far and posts the settle marker;
/// there it reads the focus and posts the next key, or, none left, reads the checks and ends
/// the box; every other command is heard. Each key goes through the box's
/// own loop and its `IsDialogMessage`, as the first step's way 3.
extern "system" fn keyed(window: usize, message: u32, w: usize, l: isize) -> isize {
    let id = w & 0xffff;
    match message {
        WM_INITDIALOG => {
            // SAFETY: `window` is the box USER32 is opening; a post fails softly.
            unsafe { PostMessageA(window, WM_COMMAND, usize::from(GO), 0) };
            1
        }
        WM_COMMAND if id == usize::from(GO) => {
            let start = KEY_CASES[KEYS_NOW.get()].start;
            if start != 0 {
                // SAFETY: the box is open; `GetDlgItem` answers 0 for an id it lacks, and the
                // focus move takes any handle.
                unsafe {
                    let control = GetDlgItem(window, i32::from(start));
                    SendMessageA(window, WM_NEXTDLGCTL, control, 1);
                }
            }
            HEARD.take();
            user32_post_key(window);
            1
        }
        WM_COMMAND if id == usize::from(STEP) => {
            let early = HEARD.with_borrow(Vec::len).to_string();
            let step = KEY_PLACE.get().saturating_sub(1);
            READS.with_borrow_mut(|reads| reads.push((format!("{step}.early"), early)));
            // SAFETY: as above, the box open; a post fails softly.
            unsafe { PostMessageA(window, WM_COMMAND, usize::from(SETTLE), 0) };
            1
        }
        WM_COMMAND if id == usize::from(SETTLE) => {
            let names = user32_names(window);
            // SAFETY: the box is open while its procedure runs; the focus call takes nothing.
            let focus = named(unsafe { GetFocus() }, &names);
            let step = KEY_PLACE.get().saturating_sub(1);
            READS.with_borrow_mut(|reads| step_reads(reads, step, focus, &HEARD.take()));
            if !user32_post_key(window) {
                let mut checks = String::new();
                for control in CHECKS {
                    // SAFETY: as above, the box open; an id it lacks answers 0.
                    let checked = unsafe { IsDlgButtonChecked(window, i32::from(control)) };
                    checks.push_str(&checked.to_string());
                }
                READS.with_borrow_mut(|reads| reads.push(("checks".into(), checks)));
                // SAFETY: as above, the box open.
                unsafe { EndDialog(window, ENDED) };
            }
            1
        }
        WM_COMMAND => {
            let names = user32_names(window);
            // SAFETY: as above, the box open; an id it lacks answers 0.
            let checked = unsafe { IsDlgButtonChecked(window, i32::try_from(id).unwrap_or(0)) };
            let l = named(usize::from_ne_bytes(l.to_ne_bytes()), &names);
            let w = u32::try_from(w).unwrap_or(u32::MAX);
            HEARD.with_borrow_mut(|commands| commands.push(heard(w, &l, checked)));
            1
        }
        _ => 0,
    }
}

/// Hands `Front` the next key of `case` as a host would, on the owner, then the step marker;
/// false once no key is left.
fn front_post_key(desk: &Desk<Page>, case: &KeyCase, window: u32, place: &mut usize) -> bool {
    let Some(&(key, both)) = case.keys.get(*place) else {
        return false;
    };
    *place += 1;
    desk.screen().hand(Event::Key {
        window: TOP,
        key,
        down: true,
    });
    if both {
        desk.screen().hand(Event::Key {
            window: TOP,
            key,
            down: false,
        });
    }
    desk.screen().hand(Event::Command { window, id: STEP });
    true
}

/// `Front`'s side of a keys case over a fresh desk, the same reads as `keyed`'s in one order.
fn front_keys(disc: &Disc, case: &KeyCase, ids: &[u16], id: u32) -> (u32, Vec<(String, String)>) {
    let (desk, tables) = (Desk::new(Page::new(0x280, 0x1e0)), Tables::new());
    desk.state().windows.push(desk_window(TOP, 0, 0));
    desk.state().focus = TOP;
    let front = Front::new(&ROW, &desk, &tables, disc);
    let (mut reads, mut commands, mut place) = (Vec::new(), Vec::new(), 0);
    let answer = front.dialog_box_wait([INSTANCE, id, TOP, 0x50, 0x33], |_, sent| {
        let names = front_names(&front, sent.window, ids, TOP);
        let (window, id) = (sent.window, sent.w & 0xffff);
        match sent.message {
            WM_INITDIALOG => {
                desk.screen().hand(Event::Command { window, id: GO });
                1
            }
            WM_COMMAND if id == u32::from(GO) => {
                if case.start != 0 {
                    let control = front.get_dlg_item(window, u32::from(case.start));
                    desk.state().focus = control;
                }
                front_post_key(&desk, case, window, &mut place);
                0
            }
            WM_COMMAND if id == u32::from(STEP) => {
                let step = place.saturating_sub(1);
                reads.push((format!("{step}.early"), commands.len().to_string()));
                desk.screen().hand(Event::Command { window, id: SETTLE });
                0
            }
            WM_COMMAND if id == u32::from(SETTLE) => {
                let focus = named(wide(desk.state().focus), &names);
                let step = place.saturating_sub(1);
                step_reads(&mut reads, step, focus, &std::mem::take(&mut commands));
                if !front_post_key(&desk, case, window, &mut place) {
                    let checks: String = CHECKS
                        .iter()
                        .map(|&control| {
                            let checked = front.is_dlg_button_checked(window, u32::from(control));
                            checked.to_string()
                        })
                        .collect();
                    reads.push(("checks".into(), checks));
                    let _ = front.end_dialog(window, 7);
                }
                0
            }
            WM_COMMAND => {
                let checked = front.is_dlg_button_checked(window, id);
                commands.push(heard(sent.w, &named(wide(sent.l), &names), checked));
                0
            }
            _ => 0,
        }
    });
    (answer, reads)
}

/// A read by its row's last words, `none` if that side never reached it.
fn read_of(reads: &[(String, String)], last: &str) -> String {
    reads
        .iter()
        .find(|(name, _)| name == last)
        .map_or_else(|| "none".into(), |(_, value)| value.clone())
}

/// The keys rows: each case's answer, then after each key the focus and the commands heard,
/// then the checks, both sides, every key through the box's own loop.
fn keys() {
    let bytes = [
        plant::dialog(BOX_STYLE, "Keys", KEYS_FULL),
        plant::dialog(BOX_STYLE, "Bare", KEYS_BARE),
    ];
    let entries = [
        (resource::DIALOG, KEYS_BOX, &bytes[0][..]),
        (resource::DIALOG, KEYS_BOX + 1, &bytes[1][..]),
    ];
    let disc = Disc(kernel_process::plant::image(&entries));
    let templates = [aligned(&bytes[0]), aligned(&bytes[1])];
    // SAFETY: a null name asks the exe's own module.
    let instance = unsafe { GetModuleHandleA(std::ptr::null()) };
    let windows = user32_windows(instance);
    for (place, case) in KEY_CASES.iter().enumerate() {
        let controls = if case.bare { KEYS_BARE } else { KEYS_FULL };
        let ids: Vec<u16> = controls.iter().map(|control| control.0).collect();
        KEYS_NOW.set(place);
        KEY_PLACE.set(0);
        READS.take();
        IDS.set(ids.clone());
        OWNER_NOW.set(windows[0]);
        let template = &templates[usize::from(case.bare)];
        let theirs = traced(|| {
            // SAFETY: the template lives across the call, a whole planted template on a 4-byte
            // boundary; the procedure is `extern "system"`; the owner is the live top window.
            unsafe {
                DialogBoxIndirectParamA(instance, template.as_ptr(), windows[0], keyed, PARAM)
            }
        });
        let reads = READS.take();
        let (ours, own) = front_keys(&disc, case, &ids, KEYS_BOX + u32::from(case.bare));
        let name = case.name;
        row_error(&format!("keys.{name}.answer"), &theirs, number(ours));
        let lasts = (0..case.keys.len())
            .flat_map(|step| {
                [
                    format!("{step}.early"),
                    format!("{step}.focus"),
                    format!("{step}.commands"),
                ]
            })
            .chain(["checks".to_string()]);
        for last in lasts {
            let (user32, front) = (read_of(&reads, &last), read_of(&own, &last));
            row(&format!("keys.{name}.{last}"), user32, front);
        }
    }
}

fn main() {
    let arguments: Vec<String> = std::env::args().collect();
    match arguments.get(1).map(String::as_str) {
        Some("hello") => println!("hello"),
        Some("menus") => menus(),
        Some("bars") => bars(),
        Some("box") => boxes(),
        Some("inside") => inside(),
        Some("keys") => keys(),
        Some("first") => {
            let way = arguments
                .get(2)
                .and_then(|way| way.parse().ok())
                .unwrap_or(1);
            first_read(way);
        }
        _ => {
            eprintln!(
                "usage: user32_probe hello | menus | bars | box | inside | keys | first <0|1|2|3>"
            );
            std::process::exit(2);
        }
    }
}
