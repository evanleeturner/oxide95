// SPDX-License-Identifier: MIT
//! Wine's own USER32 beside the crate's `Front`, on the same planted menu and box bytes, in one
//! 32-bit process. One flat line a row, `row=<name> user32=<answer> front=<answer>`, read by
//! `user32_check.py`, which sets the two sides apart. A handle prints by its kind alone.
//!
//! Built for `i686-pc-windows-gnu` under its own feature, so a native `cargo test --workspace`
//! links no `user32`. The mode `first <way>` asks whether a box closes with no key from outside.
#![allow(unsafe_code)]

use std::cell::Cell;
use std::fmt::Display;

use kernel_process::resource;
use window::desk::Desk;
use window::page::Page;
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

fn main() {
    let arguments: Vec<String> = std::env::args().collect();
    match arguments.get(1).map(String::as_str) {
        Some("hello") => println!("hello"),
        Some("menus") => menus(),
        Some("first") => {
            let way = arguments
                .get(2)
                .and_then(|way| way.parse().ok())
                .unwrap_or(1);
            first_read(way);
        }
        _ => {
            eprintln!("usage: user32_probe hello | menus | first <0|1|2|3>");
            std::process::exit(2);
        }
    }
}
