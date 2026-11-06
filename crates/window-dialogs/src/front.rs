// SPDX-License-Identifier: MIT
//! The 10 fronts: one file's box and menu calls, each answering in the file's
//! numbers with one `imp.call` line. A procedure is the program's code: the one
//! front that calls one takes it as `call`, as `window`'s fronts do.
//!
//! [`Front::press`] and [`Front::key`] are no call of the program's: they are
//! the player's answers, and write `box.press` alone. No line here is a call the
//! program never made: `WM_INITDIALOG` and a press reach the procedure and the
//! queue without `SendMessageA`'s or `PostMessageA`'s line.
//!
//! `DialogBoxParamA` has two fronts, one line each: [`Front::dialog_box_param`]
//! returns at once, for a page; [`Front::dialog_box_wait`] loops until
//! `EndDialog`, for a native target.

use alloc::string::String;
use alloc::vec::Vec;

use window::desk::{Desk, Window};
use window::{Message, Numbers, Rect, Screen};

use crate::Resources;
use crate::table::Row;
use crate::tables::{Control, Menu, Open, Tables};
use crate::template::{self, Dialog, find_item};

const RT_MENU: u32 = kernel_process::resource::MENU;
const RT_DIALOG: u32 = kernel_process::resource::DIALOG;
/// `WM_INITDIALOG` and `WM_COMMAND`: the two messages a box is sent.
const WM_INITDIALOG: u32 = 0x110;
const WM_COMMAND: u32 = 0x111;
/// The 4 key messages a box's own loop takes.
const WM_KEYDOWN: u32 = 0x100;
const WM_KEYUP: u32 = 0x101;
const WM_SYSKEYDOWN: u32 = 0x104;
const WM_SYSKEYUP: u32 = 0x105;
/// `MF_BYPOSITION`: the item is named by its place, not its id.
const MF_BYPOSITION: u32 = 0x400;
/// The state bits `EnableMenuItem` and `CheckMenuItem` each change and answer.
pub(crate) const ENABLE_BITS: u32 = 0x3;
pub(crate) const CHECK_BITS: u32 = 0x8;
/// A menu item not there, and a box whose template cannot be read: -1.
const ABSENT: u32 = u32::MAX;
/// A button's low digit: the default push button, and the auto check box.
pub(crate) const BS_DEFPUSHBUTTON: u32 = 1;
pub(crate) const BS_AUTOCHECKBOX: u32 = 3;
pub(crate) const BS_TYPE: u32 = 0xf;
/// The class that makes a control's low style digit a button's kind.
const BUTTON_CLASS: &str = "BUTTON";
const WS_TABSTOP: u32 = 0x1_0000;
/// `IDOK` and `IDCANCEL`.
const IDOK: u16 = 1;
const IDCANCEL: u16 = 2;
/// The keys a box takes.
const VK_TAB: u8 = 0x09;
const VK_RETURN: u8 = 0x0d;
const VK_ESCAPE: u8 = 0x1b;
const VK_SPACE: u8 = 0x20;
/// The class Windows gives a box.
const BOX_CLASS: &[u8] = b"#32770";

/// One row of `window`'s table for a box or a control, the template's place
/// kept in its dialog units: a box draws nothing, so none is turned to pixels.
fn row(
    handle: u32,
    parent: u32,
    procedure: u32,
    made: (&[u8], &str, u32, u32, [i16; 4]),
) -> Window {
    let (class, words, style, ex_style, place) = made;
    let [left, top, width, height] = place.map(i32::from);
    Window {
        handle,
        class: class.to_vec(),
        procedure,
        rect: Rect::sized(left, top, width, height),
        style,
        ex_style,
        user: 0,
        longs: Vec::new(),
        title: words.as_bytes().to_vec(),
        parent,
        shown: true,
    }
}

/// One file's box and menu calls: its row, its program's desk and tables, and
/// where its templates are read.
pub struct Front<'d, S, R> {
    file: &'static Row,
    desk: &'d Desk<S>,
    tables: &'d Tables,
    resources: &'d R,
}

impl<'d, S: Screen, R: Resources> Front<'d, S, R> {
    pub const fn new(
        file: &'static Row,
        desk: &'d Desk<S>,
        tables: &'d Tables,
        resources: &'d R,
    ) -> Self {
        Self {
            file,
            desk,
            tables,
            resources,
        }
    }

    /// Writes the call's one line.
    fn line(&self, call: &str, arguments: &[u32], answer: u32) {
        let (a, v) = (Numbers(arguments), oxide_trace::__shown!(answer));
        self.desk
            .screen()
            .quiet(|| log::debug!(c = call, a:% = a, v:% = v, by = "window-dialogs"; "imp.call"));
    }

    /// `LoadMenuA`, its name a resource number: the menu's handle, one
    /// [`Front::set_menu`] takes; 0 for a menu the module lacks. Callers: the shell.
    pub fn load_menu(&self, instance: u32, menu: u32) -> u32 {
        let read = self.resources.resource(instance, RT_MENU, menu);
        let handle = read.and_then(template::menu).map_or(0, |items| {
            let handle = self.desk.state().handle();
            self.tables.state().menus.push(Menu { handle, items });
            handle
        });
        self.line(self.file.calls.load_menu, &[instance, menu], handle);
        handle
    }

    /// `SetMenu`: 1, the window's bar now `menu`, 0 for none; 0 and nothing
    /// changed for a window not here or a menu never read. It shows nothing:
    /// a page shows the bar anew at [`Front::draw_menu_bar`]. Callers: the
    /// shell, the launcher.
    #[must_use]
    pub fn set_menu(&self, window: u32, menu: u32) -> u32 {
        let here = self.desk.state().window(window).is_some();
        let mut tables = self.tables.state();
        let known = menu == 0 || tables.menu_mut(menu).is_some();
        let answer = u32::from(here && known);
        if answer == 1 {
            tables.bar_mut(window).menu = menu;
        }
        drop(tables);
        self.line(self.file.calls.set_menu, &[window, menu], answer);
        answer
    }

    /// `DrawMenuBar`: 1, the window's bar counted drawn; 0 for a window not
    /// here. Callers: the shell.
    #[must_use]
    pub fn draw_menu_bar(&self, window: u32) -> u32 {
        let here = self.desk.state().window(window).is_some();
        if here {
            self.tables.state().bar_mut(window).drawn += 1;
        }
        self.line(self.file.calls.draw_menu_bar, &[window], u32::from(here));
        u32::from(here)
    }

    /// The bits `mask` of the item, as they stood, then `flags`' in their
    /// place; -1 and nothing changed for a menu or an item not there.
    fn change_item(&self, menu: u32, item: u32, flags: u32, mask: u32) -> u32 {
        let mut tables = self.tables.state();
        let by_place = flags & MF_BYPOSITION != 0;
        tables
            .menu_mut(menu)
            .and_then(|menu| find_item(&mut menu.items, item, by_place))
            .map_or(ABSENT, |found| {
                let before = found.state & mask;
                found.state = found.state & !mask | flags & mask;
                before
            })
    }

    /// `EnableMenuItem`: the item's enable bits before the call, the template's
    /// on the first; -1 and nothing changed for an item the menu lacks, by id
    /// through every popup, or by place with `0x400`. Callers: the shell.
    #[must_use]
    pub fn enable_menu_item(&self, menu: u32, item: u32, flags: u32) -> u32 {
        let answer = self.change_item(menu, item, flags, ENABLE_BITS);
        let arguments = [menu, item, flags];
        self.line(self.file.calls.enable_menu_item, &arguments, answer);
        answer
    }

    /// `CheckMenuItem`: the item's check bit before the call; -1 and nothing
    /// changed for an item the menu lacks, as [`Front::enable_menu_item`]
    /// finds one. Callers: the shell.
    #[must_use]
    pub fn check_menu_item(&self, menu: u32, item: u32, flags: u32) -> u32 {
        let answer = self.change_item(menu, item, flags, CHECK_BITS);
        let arguments = [menu, item, flags];
        self.line(self.file.calls.check_menu_item, &arguments, answer);
        answer
    }

    /// The box's rows made and its table row kept; the first control's handle,
    /// 0 for none. The focus stays where it is.
    fn open(&self, dialog: &Dialog, owner: u32, procedure: u32) -> (u32, u32) {
        let mut state = self.desk.state();
        let handle = state.handle();
        let before = state.focus;
        let made = (
            BOX_CLASS,
            dialog.title.as_str(),
            dialog.style,
            dialog.ex_style,
            dialog.place,
        );
        state.windows.push(row(handle, owner, procedure, made));
        let mut controls = Vec::with_capacity(dialog.controls.len());
        for control in &dialog.controls {
            let child = state.handle();
            let made = (
                control.class.as_bytes(),
                control.words.as_str(),
                control.style,
                control.ex_style,
                control.place,
            );
            state.windows.push(row(child, handle, 0, made));
            controls.push(Control {
                handle: child,
                id: control.id,
                style: control.style,
                button: control.class.eq_ignore_ascii_case(BUTTON_CLASS),
                words: String::from(control.words.as_str()),
                checked: false,
            });
        }
        drop(state);
        let first = controls.first().map_or(0, |control| control.handle);
        self.tables.state().boxes.push(Open {
            handle,
            owner,
            before,
            controls,
        });
        (handle, first)
    }

    /// The box opened from `opening`'s template and given `WM_INITDIALOG`: its
    /// handle; `None`, with the -1 line written, for a template the module lacks.
    fn begin(&self, opening: [u32; 5], call: impl FnOnce(u32, &Message) -> u32) -> Option<u32> {
        let [instance, name, owner, procedure, param] = opening;
        let read = self.resources.resource(instance, RT_DIALOG, name);
        let Some(dialog) = read.and_then(template::dialog) else {
            self.line(self.file.calls.dialog_box_param, &opening, ABSENT);
            return None;
        };
        let (handle, first) = self.open(&dialog, owner, procedure);
        let init = Message {
            window: handle,
            message: WM_INITDIALOG,
            w: first,
            l: param,
            time: self.desk.screen().now(),
            ..Message::default()
        };
        if call(procedure, &init) != 0 && self.tables.state().open_mut(handle).is_some() {
            self.desk.state().focus = first;
        }
        Some(handle)
    }

    /// `DialogBoxParamA` for a target that cannot wait: the box opened from the
    /// module's template, `call` given `WM_INITDIALOG` with the first control's
    /// handle and `param`. An answer of 0 leaves the focus where the procedure
    /// put it; any other puts it on that control, unmeasured. Answers 0 at once,
    /// the box still open, which holds only for a caller that drops the result;
    /// -1 for a template the module lacks. Draws nothing. Callers: the shell.
    pub fn dialog_box_param(
        &self,
        opening: [u32; 5],
        call: impl FnOnce(u32, &Message) -> u32,
    ) -> u32 {
        if self.begin(opening, call).is_none() {
            return ABSENT;
        }
        self.line(self.file.calls.dialog_box_param, &opening, 0);
        0
    }

    /// `DialogBoxParamA` for a native target: opens as
    /// [`Front::dialog_box_param`] does, then runs the box's own loop until
    /// [`Front::end_dialog`] closes it, and answers `EndDialog`'s value; -1 for
    /// a template the module lacks, with no loop.
    ///
    /// The loop pumps the back and takes, oldest first, the box's messages,
    /// each given to `call`, and the 4 key messages of the box or its owner: a
    /// back's keys arrive on the owner, the box having no window of its own. A
    /// key down goes to [`Front::key`]; the rest of the 4 are dropped. Every
    /// other message stays queued, in order, for the program's pump after the
    /// close: the owner's procedure is not re-entered, unlike under Windows.
    ///
    /// Does not return until the box closes: a box no key or procedure closes
    /// spins for good, on a whole core, as `Screen` has no blocking wait, so a
    /// page must never call this. Draws nothing, and sends the owner none of
    /// the messages Windows sends it while a box is open; `box.wait` says so.
    #[must_use]
    pub fn dialog_box_wait(
        &self,
        opening: [u32; 5],
        mut call: impl FnMut(u32, &Message) -> u32,
    ) -> u32 {
        let Some(handle) = self.begin(opening, &mut call) else {
            return ABSENT;
        };
        let [_, _, owner, procedure, _] = opening;
        let w = oxide_trace::__shown!(handle);
        self.desk
            .screen()
            .quiet(|| log::debug!(w:% = w; "box.wait"));
        let value = loop {
            if let Some(value) = self.ended_value(handle) {
                break value;
            }
            self.desk.pump();
            let Some(message) = self.take_for_box(handle, owner) else {
                continue;
            };
            match message.message {
                WM_KEYDOWN => {
                    let _ = u8::try_from(message.w).is_ok_and(|key| self.key(handle, key));
                }
                WM_KEYUP | WM_SYSKEYDOWN | WM_SYSKEYUP => {}
                _ => {
                    call(procedure, &message);
                }
            }
        };
        self.line(self.file.calls.dialog_box_param, &opening, value);
        value
    }

    /// `EndDialog`'s value for the box `handle`, `None` while it stays open.
    fn ended_value(&self, handle: u32) -> Option<u32> {
        let tables = self.tables.state();
        let closed = tables.ended.iter().rev().find(|(box_, _)| *box_ == handle);
        closed.map(|(_, value)| *value)
    }

    /// The oldest queued message the box's loop takes, off the queue; `None`
    /// for none, every other message left in place.
    fn take_for_box(&self, handle: u32, owner: u32) -> Option<Message> {
        let mut state = self.desk.state();
        let at = state.queue.iter().position(|message| {
            let key = matches!(
                message.message,
                WM_KEYDOWN | WM_KEYUP | WM_SYSKEYDOWN | WM_SYSKEYUP
            );
            message.window == handle || key && message.window == owner
        })?;
        state.queue.remove(at)
    }

    /// `EndDialog`: 1, the box closed at once: its rows gone, the focus back
    /// where it stood as the box opened, `value` kept in the tables' `ended`;
    /// 0 for a box not open. Does not guarantee Windows' order, where the box
    /// lives until its procedure returns: a call on it after this one answers
    /// as for a box gone. Callers: the shell.
    #[must_use]
    pub fn end_dialog(&self, window: u32, value: u32) -> u32 {
        let mut tables = self.tables.state();
        let at = tables.boxes.iter().position(|open| open.handle == window);
        let answer = at.map_or(0, |at| {
            let open = tables.boxes.remove(at);
            tables.ended.push((window, value));
            let mut state = self.desk.state();
            state
                .windows
                .retain(|row| row.handle != window && row.parent != window);
            state.focus = open.before;
            1
        });
        drop(tables);
        self.line(self.file.calls.end_dialog, &[window, value], answer);
        answer
    }

    /// The control `id` of the box open as `window`: `read` given it.
    fn control<T>(&self, window: u32, id: u32, read: impl FnOnce(&mut Control) -> T) -> Option<T> {
        let mut tables = self.tables.state();
        let open = tables.open_mut(window)?;
        let found = open.controls.iter_mut().find(|c| u32::from(c.id) == id)?;
        Some(read(found))
    }

    /// `GetDlgItem`: the control's handle, one `window`'s `SetFocus` takes; 0
    /// for a box not open or an id it lacks. Callers: the shell.
    #[must_use]
    pub fn get_dlg_item(&self, window: u32, id: u32) -> u32 {
        let answer = self.control(window, id, |found| found.handle).unwrap_or(0);
        self.line(self.file.calls.get_dlg_item, &[window, id], answer);
        answer
    }

    /// `CheckDlgButton`: 1, the check kept: on for `check` 1, off for any
    /// other, the third state unmeasured; 0 for a box not open or an id it
    /// lacks. Callers: the shell.
    #[must_use]
    pub fn check_dlg_button(&self, window: u32, id: u32, check: u32) -> u32 {
        let done = self.control(window, id, |found| found.checked = check == 1);
        let answer = u32::from(done.is_some());
        self.line(
            self.file.calls.check_dlg_button,
            &[window, id, check],
            answer,
        );
        answer
    }

    /// `IsDlgButtonChecked`: exactly 1 or 0, its reader's `cmp eax,1`; 0 for a
    /// box not open or an id it lacks. Callers: the shell.
    #[must_use]
    pub fn is_dlg_button_checked(&self, window: u32, id: u32) -> u32 {
        let checked = self.control(window, id, |found| found.checked);
        let answer = u32::from(checked == Some(true));
        self.line(self.file.calls.is_dlg_button_checked, &[window, id], answer);
        answer
    }

    /// The player chose `id` in the box: a `BUTTON` of the auto check kind flips first, then
    /// `WM_COMMAND` is queued for the box, `id` the low word and `l` 0, so the
    /// procedure reads the new state. An id with no control still queues, as
    /// Escape reaches `IDCANCEL` with none. False for a box not open.
    #[must_use]
    pub fn press(&self, window: u32, id: u16) -> bool {
        if self.tables.state().open_mut(window).is_none() {
            return false;
        }
        let checked = self
            .control(window, u32::from(id), |found| {
                if found.button && found.style & BS_TYPE == BS_AUTOCHECKBOX {
                    found.checked = !found.checked;
                }
                found.checked
            })
            .unwrap_or(false);
        let time = self.desk.screen().now();
        self.desk.state().queue.push_back(Message {
            window,
            message: WM_COMMAND,
            w: u32::from(id),
            time,
            ..Message::default()
        });
        let (w, shown) = (oxide_trace::__shown!(window), u32::from(id));
        self.desk.screen().quiet(|| {
            log::debug!(w:% = w, id = shown, checked = checked; "box.press");
        });
        true
    }

    /// The next tab stop after the focus, round to the first; 0 for none.
    fn next_stop(&self, open: &Open) -> u32 {
        let focus = self.desk.state().focus;
        let stops: Vec<u32> = open
            .controls
            .iter()
            .filter(|control| control.style & WS_TABSTOP != 0)
            .map(|control| control.handle)
            .collect();
        let after = stops.iter().position(|stop| *stop == focus);
        let next = after.map_or(0, |after| (after + 1) % stops.len());
        stops.get(next).copied().unwrap_or(0)
    }

    /// A key down in the box: Return presses the default button, `IDOK` if it
    /// has none; Escape `IDCANCEL`; Space the button holding the focus; Tab
    /// moves the focus to the next tab stop. Any other key, Space on no button,
    /// or a box not open, answers false and does nothing.
    #[must_use]
    pub fn key(&self, window: u32, key: u8) -> bool {
        let Some(open) = self
            .tables
            .state()
            .open_mut(window)
            .map(|open| open.clone())
        else {
            return false;
        };
        let default = open
            .controls
            .iter()
            .find(|control| control.button && control.style & BS_TYPE == BS_DEFPUSHBUTTON);
        match key {
            VK_RETURN => self.press(window, default.map_or(IDOK, |control| control.id)),
            VK_ESCAPE => self.press(window, IDCANCEL),
            VK_SPACE => {
                let focus = self.desk.state().focus;
                let held = open.controls.iter().find(|control| control.handle == focus);
                held.filter(|control| control.button)
                    .is_some_and(|control| self.press(window, control.id))
            }
            VK_TAB => {
                let next = self.next_stop(&open);
                self.desk.state().focus = next;
                next != 0
            }
            _ => false,
        }
    }
}
