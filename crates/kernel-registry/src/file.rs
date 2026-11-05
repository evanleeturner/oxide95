// SPDX-License-Identifier: MIT
//! The settings file on Linux, macOS and Windows: the tree in one plain
//! text file, `settings.txt`, in the row's folder under the host's
//! settings folder, by `std::env` and `std::fs` alone, never the host's
//! own registry. Each save writes a sibling `settings.txt.new` whole and
//! renames it over the file, so a program that dies mid-save leaves the
//! last whole file: the original dies 17 ms after its writes.
//! macOS's folder is written, not yet measured.
use crate::Settings;
use crate::table::Row;
use crate::tree::Tree;
use core::cell::RefCell;
use std::path::{Path, PathBuf};
use std::{env, fs, io};

/// The file's name in its folder, and the sibling a save writes first.
const NAME: &str = "settings.txt";
const NEW: &str = "settings.txt.new";

/// The settings a program keeps in its own text file.
pub struct SettingsFile {
    tree: RefCell<Tree>,
    folder: PathBuf,
}

impl SettingsFile {
    /// The settings in the row's folder under the host's settings folder;
    /// a folder with no file yet starts as the row's first values.
    pub fn for_program(row: &Row) -> io::Result<Self> {
        let host = settings_folder().ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::NotFound,
                "no settings folder: APPDATA, XDG_CONFIG_HOME and HOME unset",
            )
        })?;
        Self::in_folder(&host.join(row.folder), row)
    }

    /// The settings in `folder`, the tests' own; the folder is made at the
    /// first save.
    pub fn in_folder(folder: &Path, row: &Row) -> io::Result<Self> {
        let saved = match fs::read(folder.join(NAME)) {
            Ok(saved) => saved,
            Err(error) if error.kind() == io::ErrorKind::NotFound => Vec::new(),
            Err(error) => return Err(error),
        };
        let tree = Tree::new(row, &saved).ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                "settings.txt is not in the settings form",
            )
        })?;
        Ok(Self {
            tree: RefCell::new(tree),
            folder: folder.to_path_buf(),
        })
    }

    /// Writes the whole tree to the sibling file and renames it over the
    /// last; whether both took.
    fn saved(&self) -> bool {
        let new = self.folder.join(NEW);
        fs::create_dir_all(&self.folder)
            .and_then(|()| fs::write(&new, self.tree.borrow().text()))
            .and_then(|()| fs::rename(&new, self.folder.join(NAME)))
            .is_ok()
    }
}

/// The host's settings folder: `%APPDATA%` on Windows, `~/Library/
/// Application Support` on macOS, `$XDG_CONFIG_HOME` or `~/.config`
/// elsewhere; `None` when its variable is unset or empty.
fn settings_folder() -> Option<PathBuf> {
    let named = |variable| {
        env::var_os(variable)
            .filter(|folder| !folder.is_empty())
            .map(PathBuf::from)
    };
    if cfg!(windows) {
        named("APPDATA")
    } else if cfg!(target_os = "macos") {
        named("HOME").map(|home| home.join("Library").join("Application Support"))
    } else {
        named("XDG_CONFIG_HOME").or_else(|| named("HOME").map(|home| home.join(".config")))
    }
}

impl Settings for SettingsFile {
    /// A made key whose save fails is let go, and answers `None`.
    fn open(&self, within: Option<u32>, path: &[u8], create: bool) -> Option<(u32, bool)> {
        let opened = self.tree.borrow_mut().open(within, path, create);
        match opened {
            Some((handle, true)) if !self.saved() => {
                self.tree.borrow_mut().close(handle);
                None
            }
            other => other,
        }
    }

    fn holds(&self, handle: u32) -> bool {
        self.tree.borrow().holds(handle)
    }

    fn read(&self, handle: u32, name: &[u8]) -> Option<u32> {
        self.tree.borrow().read(handle, name)
    }

    fn write(&self, handle: u32, name: &[u8], value: u32) -> bool {
        let kept = self.tree.borrow_mut().write(handle, name, value);
        kept && self.saved()
    }

    fn close(&self, handle: u32) -> bool {
        self.tree.borrow_mut().close(handle)
    }
}
