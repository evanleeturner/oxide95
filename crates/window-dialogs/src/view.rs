// SPDX-License-Identifier: MIT
//! What a page shows of a box and a menu bar, as data read off `window`'s desk
//! and [`Tables`]. It reaches no browser: a script takes the text of
//! [`BoxView::lines`] and [`BarView::lines`] and hands its answers back through
//! `press`, `key` and `window`'s `Event::Command`.
//!
//! Call: [`open_box`] `(desk, tables, window) -> Option<BoxView>`, [`bar`]
//! `(tables, window) -> Option<BarView>`, [`shown`] `(words) -> String`.
//! Places stay in a template's dialog units; nothing here turns them to pixels.

use alloc::string::{String, ToString};
use alloc::vec::Vec;
use core::fmt::Write;

use window::desk::Desk;
use window::{Rect, Screen};

use crate::front::{BS_AUTOCHECKBOX, BS_DEFPUSHBUTTON, BS_TYPE, CHECK_BITS, ENABLE_BITS};
use crate::tables::{Control, Tables};
use crate::template::Item;

#[cfg(test)]
mod tests;

/// What a control is to a page. A class other than `BUTTON` reads as a label and
/// a button of a kind other than these as a plain one: neither is measured.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Default,
    Button,
    Check,
    Label,
}

impl Kind {
    fn of(control: &Control) -> Self {
        if !control.button {
            return Self::Label;
        }
        match control.style & BS_TYPE {
            BS_DEFPUSHBUTTON => Self::Default,
            BS_AUTOCHECKBOX => Self::Check,
            _ => Self::Button,
        }
    }

    const fn word(self) -> &'static str {
        match self {
            Self::Default => "default",
            Self::Button => "button",
            Self::Check => "check",
            Self::Label => "label",
        }
    }
}

/// One control as a page shows it: `words` already [`shown`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ControlView {
    pub handle: u32,
    pub id: u16,
    pub kind: Kind,
    pub words: String,
    pub checked: bool,
    pub focused: bool,
    pub place: Rect,
}

/// One box open as a page shows it, its controls in the template's order. The
/// title keeps every `&`, as a caption takes no access key.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BoxView {
    pub handle: u32,
    pub title: String,
    pub place: Rect,
    pub controls: Vec<ControlView>,
}

/// What one row of a menu is to a page.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RowKind {
    Popup,
    Item,
    Separator,
}

/// One row of a menu: `words` already [`shown`], a tab before a shortcut's words
/// kept; `rows` a popup's own, empty for the rest.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MenuRow {
    pub id: u16,
    pub kind: RowKind,
    pub words: String,
    pub enabled: bool,
    pub checked: bool,
    pub rows: Vec<MenuRow>,
}

/// One window's bar: `drawn` counts `DrawMenuBar`'s calls, so a page knows when
/// to show the bar anew.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BarView {
    pub window: u32,
    pub drawn: u32,
    pub rows: Vec<MenuRow>,
}

/// A template's words as a person reads them: `&&` is one `&`, and any other `&`
/// goes, a last one too. Does not name the access key it marked.
#[must_use]
pub fn shown(words: &str) -> String {
    let mut out = String::with_capacity(words.len());
    let mut letters = words.chars();
    while let Some(letter) = letters.next() {
        match letter {
            '&' => out.extend(letters.next()),
            other => out.push(other),
        }
    }
    out
}

/// The box open as `window`; `None` for one not open, a box closed included.
#[must_use]
pub fn open_box<S: Screen>(desk: &Desk<S>, tables: &Tables, window: u32) -> Option<BoxView> {
    let mut tables = tables.state();
    let open = tables.open_mut(window)?;
    let state = desk.state();
    let row = |handle| state.windows.iter().find(|row| row.handle == handle);
    let boxed = row(window)?;
    let view = |control: &Control| ControlView {
        handle: control.handle,
        id: control.id,
        kind: Kind::of(control),
        words: shown(&control.words),
        checked: control.checked,
        focused: state.focus == control.handle,
        place: row(control.handle).map_or_else(Rect::default, |found| found.rect),
    };
    Some(BoxView {
        handle: window,
        title: String::from_utf8_lossy(&boxed.title).to_string(),
        place: boxed.rect,
        controls: open.controls.iter().map(view).collect(),
    })
}

fn menu_row(item: &Item) -> MenuRow {
    let kind = match (item.popup, item.id, item.words.is_empty()) {
        (true, _, _) => RowKind::Popup,
        (false, 0, true) => RowKind::Separator,
        (false, _, _) => RowKind::Item,
    };
    MenuRow {
        id: item.id,
        kind,
        words: shown(&item.words),
        enabled: item.state & ENABLE_BITS == 0,
        checked: item.state & CHECK_BITS != 0,
        rows: item.items.iter().map(menu_row).collect(),
    }
}

/// The bar of `window`; `None` for a window with no menu set.
#[must_use]
pub fn bar(tables: &Tables, window: u32) -> Option<BarView> {
    let mut tables = tables.state();
    let found = *tables.bars.iter().find(|bar| bar.window == window)?;
    let menu = tables.menu_mut(found.menu)?;
    Some(BarView {
        window,
        drawn: found.drawn,
        rows: menu.items.iter().map(menu_row).collect(),
    })
}

/// A `\`, a tab or a line end in words, written so one line stays one record.
fn escaped(words: &str) -> String {
    let mut out = String::with_capacity(words.len());
    for letter in words.chars() {
        match letter {
            '\\' => out.push_str("\\\\"),
            '\t' => out.push_str("\\t"),
            '\n' => out.push_str("\\n"),
            other => out.push(other),
        }
    }
    out
}

fn side_text(place: Rect) -> String {
    let Rect {
        left,
        top,
        right,
        bottom,
    } = place;
    let mut out = String::new();
    let _ = write!(out, "{left}\t{top}\t{right}\t{bottom}");
    out
}

impl BoxView {
    /// The box as text, tab-separated, one record a line, `\` escaping the words.
    /// First `box`, handle, the 4 sides, title; then each control: its kind's word,
    /// handle, id, checked 0 or 1, focused 0 or 1, the 4 sides, words.
    #[must_use]
    pub fn lines(&self) -> String {
        let mut out = String::new();
        let (place, title) = (side_text(self.place), escaped(&self.title));
        let _ = writeln!(out, "box\t{}\t{place}\t{title}", self.handle);
        for control in &self.controls {
            let _ = writeln!(
                out,
                "{}\t{}\t{}\t{}\t{}\t{}\t{}",
                control.kind.word(),
                control.handle,
                control.id,
                u8::from(control.checked),
                u8::from(control.focused),
                side_text(control.place),
                escaped(&control.words),
            );
        }
        out
    }
}

fn write_rows(out: &mut String, rows: &[MenuRow], depth: u32) {
    for row in rows {
        let kind = match row.kind {
            RowKind::Popup => "popup",
            RowKind::Item => "item",
            RowKind::Separator => "separator",
        };
        let _ = writeln!(
            out,
            "{kind}\t{depth}\t{}\t{}\t{}\t{}",
            row.id,
            u8::from(row.enabled),
            u8::from(row.checked),
            escaped(&row.words),
        );
        write_rows(out, &row.rows, depth + 1);
    }
}

impl BarView {
    /// The bar as text, as [`BoxView::lines`] writes: first `bar`, window, drawn;
    /// then each row, a popup's own after it: its kind's word (`popup`, `item`,
    /// `separator`), depth from 0, id, enabled 0 or 1, checked 0 or 1, words.
    #[must_use]
    pub fn lines(&self) -> String {
        let mut out = String::new();
        let _ = writeln!(out, "bar\t{}\t{}", self.window, self.drawn);
        write_rows(&mut out, &self.rows, 0);
        out
    }
}
