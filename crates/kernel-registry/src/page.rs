// SPDX-License-Identifier: MIT
//! The page's settings: the tree in memory, everything else handed in.
//! The page's owner hands in the text it saved last (the browser's
//! `localStorage`, reached from its glue), and each made key and each
//! write hands the whole text to the owner's `save`; nothing here reaches
//! a store, and nothing here can fail to save. The host tests use it as
//! their store.
use crate::Settings;
use crate::table::Row;
use crate::tree::Tree;
use core::cell::RefCell;

/// The settings a page's owner loads and saves.
pub struct Page {
    tree: RefCell<Tree>,
    save: fn(&[u8]),
}

impl Page {
    /// The settings the text `saved` holds, the row's first values when it
    /// is empty; `None` for a text not in the settings form.
    #[must_use]
    pub fn new(row: &Row, saved: &[u8], save: fn(&[u8])) -> Option<Self> {
        Tree::new(row, saved).map(|tree| Self {
            tree: RefCell::new(tree),
            save,
        })
    }

    /// Hands the whole tree's text to the owner.
    fn saved(&self) {
        (self.save)(&self.tree.borrow().text());
    }
}

impl Settings for Page {
    fn open(&self, within: Option<u32>, path: &[u8], create: bool) -> Option<(u32, bool)> {
        let opened = self.tree.borrow_mut().open(within, path, create);
        if opened.is_some_and(|(_, made)| made) {
            self.saved();
        }
        opened
    }

    fn holds(&self, handle: u32) -> bool {
        self.tree.borrow().holds(handle)
    }

    fn read(&self, handle: u32, name: &[u8]) -> Option<u32> {
        self.tree.borrow().read(handle, name)
    }

    fn write(&self, handle: u32, name: &[u8], value: u32) -> bool {
        let kept = self.tree.borrow_mut().write(handle, name, value);
        if kept {
            self.saved();
        }
        kept
    }

    fn close(&self, handle: u32) -> bool {
        self.tree.borrow_mut().close(handle)
    }
}
