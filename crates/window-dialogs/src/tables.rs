// SPDX-License-Identifier: MIT
//! The portable part every target shares: the menus, each window's bar and the
//! boxes open, kept for one program beside its `window` desk. This is what a
//! page shows, as data; nothing here reaches a browser or a pixel.

use alloc::string::String;
use alloc::vec::Vec;
use core::cell::{RefCell, RefMut};

use crate::template::Item;

/// One menu `LoadMenuA` read.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Menu {
    pub handle: u32,
    pub items: Vec<Item>,
}

/// One window's bar: `menu` 0 for none; `drawn` counts `DrawMenuBar`'s calls,
/// so a page knows when to show the bar anew.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Bar {
    pub window: u32,
    pub menu: u32,
    pub drawn: u32,
}

/// One control of a box open: its row in `window`'s table holds its place and
/// its class; this holds what a page shows and asks.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Control {
    pub handle: u32,
    pub id: u16,
    pub style: u32,
    /// Its class is `BUTTON`: only then does the style's low digit name a
    /// button's kind. A label's same digit is its alignment.
    pub button: bool,
    pub words: String,
    pub checked: bool,
}

/// One box open: `before` the window that held the focus as it opened.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Open {
    pub handle: u32,
    pub owner: u32,
    pub before: u32,
    pub controls: Vec<Control>,
}

/// The tables.
#[derive(Debug, Default)]
pub struct State {
    pub menus: Vec<Menu>,
    pub bars: Vec<Bar>,
    pub boxes: Vec<Open>,
    /// Each box closed and `EndDialog`'s value, oldest first.
    pub ended: Vec<(u32, u32)>,
}

impl State {
    /// The menu `handle`, `None` for one `LoadMenuA` never gave.
    pub fn menu_mut(&mut self, handle: u32) -> Option<&mut Menu> {
        self.menus.iter_mut().find(|menu| menu.handle == handle)
    }

    /// The box open as `handle`, `None` for one not open.
    pub fn open_mut(&mut self, handle: u32) -> Option<&mut Open> {
        self.boxes.iter_mut().find(|open| open.handle == handle)
    }

    /// The bar of `window`, a row made for one with none yet.
    pub fn bar_mut(&mut self, window: u32) -> &mut Bar {
        let at = self.bars.iter().position(|bar| bar.window == window);
        let at = at.unwrap_or_else(|| {
            self.bars.push(Bar {
                window,
                menu: 0,
                drawn: 0,
            });
            self.bars.len() - 1
        });
        &mut self.bars[at]
    }
}

/// One program's menus and boxes.
#[derive(Debug, Default)]
pub struct Tables {
    state: RefCell<State>,
}

impl Tables {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// The tables, to read or change; never held across a procedure's call,
    /// which may call a front again.
    pub fn state(&self) -> RefMut<'_, State> {
        self.state.borrow_mut()
    }
}
