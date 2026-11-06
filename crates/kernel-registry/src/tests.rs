// SPDX-License-Identifier: MIT
//! The fronts on a row of no game, through both portable backs: a value
//! set reads back after the store is closed and reopened, the first values,
//! the shapes a query is handed, and the refusals, each a logged line. The
//! file back writes only in a folder the test makes under the temporary
//! folder, never the box's own settings folder. The census rows from real
//! runs replay in each game's rows crate.

use super::*;
use crate::file::SettingsFile;
use crate::page::Page;
use crate::test_log::debug_lines;
use alloc::vec::Vec;
use core::cell::RefCell;
use std::path::PathBuf;
use std::{fs, io};
use table::{Calls, Errors};

const ROOT: u32 = 0x8000_0002;

static ROW: Row = Row {
    file: "test",
    calls: Calls {
        open_key: "open",
        query_value: "query",
        close_key: "close",
        create_key: "create",
        set_value: "set",
    },
    root: ROOT,
    key: b"Software\\Maker\\Program",
    first: &[(b"First", 1)],
    folder: "test-program",
    errors: Errors {
        not_found: 2,
        invalid_handle: 6,
        invalid_parameter: 87,
        more_data: 234,
        cannot_write: 1013,
    },
};

thread_local! {
    static SAVED: RefCell<Vec<u8>> = const { RefCell::new(Vec::new()) };
}

/// The page's save: keeps the text on this thread.
fn keep(text: &[u8]) {
    SAVED.with(|saved| *saved.borrow_mut() = text.to_vec());
}

fn saved() -> Vec<u8> {
    SAVED.with(|saved| saved.borrow().clone())
}

fn page(saved: &[u8]) -> Page {
    Page::new(&ROW, saved, keep).expect("the text is in the settings form")
}

/// A folder of the test's own under the temporary folder, removed before
/// and after.
struct Folder(PathBuf);

impl Folder {
    fn new(name: &str) -> Self {
        let path =
            std::env::temp_dir().join(format!("kernel-registry-{}-{name}", std::process::id()));
        let _ = fs::remove_dir_all(&path);
        Self(path)
    }
}

impl Drop for Folder {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// Makes a key under the row's and sets `Volume` to 7 in it.
fn set_volume<S: Settings>(settings: S) {
    let front = Front::new(&ROW, settings);
    let (key, disposition) = front
        .create_key(ROOT, b"Software\\Maker\\Program\\Sound", 0, 0xf_003f)
        .expect("the key is made");
    assert_eq!(disposition, CREATED);
    assert_eq!(
        front.set_value(key, b"Volume", DWORD, &7_u32.to_le_bytes()),
        0
    );
    assert_eq!(front.close_key(key), 0);
}

/// The value `name` under `path`, or the refusal's code.
fn value<S: Settings>(settings: S, path: &[u8], name: &[u8]) -> Result<u32, u32> {
    let front = Front::new(&ROW, settings);
    let key = front.open_key(ROOT, path, 0, 1)?;
    let mut data = [0; 4];
    let queried = front.query_value(key, name, 0, Some(SIZE), Some(&mut data));
    assert_eq!(front.close_key(key), 0);
    if queried.code == 0 {
        assert_eq!(queried.wrote, Some((DWORD, SIZE)));
        Ok(u32::from_le_bytes(data))
    } else {
        Err(queried.code)
    }
}

#[test]
fn a_value_set_reads_back_after_a_reopen_on_the_page() {
    keep(b"");
    set_volume(page(b""));
    let text = saved();
    assert_eq!(
        text,
        b"Software\\Maker\\Program\tFirst\tdword:00000001\n\
          Software\\Maker\\Program\\Sound\tVolume\tdword:00000007\n"
    );
    let reopened = page(&text);
    assert_eq!(
        value(&reopened, b"Software\\Maker\\Program\\Sound", b"Volume"),
        Ok(7)
    );
    assert_eq!(
        value(&reopened, b"Software\\Maker\\Program", b"First"),
        Ok(1)
    );
}

#[test]
fn a_value_set_reads_back_after_a_reopen_on_the_file() {
    let folder = Folder::new("reopen");
    set_volume(SettingsFile::in_folder(&folder.0, &ROW).expect("an empty folder opens"));
    assert!(!folder.0.join("settings.txt.new").exists());
    let reopened = SettingsFile::in_folder(&folder.0, &ROW).expect("the saved file opens");
    assert_eq!(
        value(&reopened, b"software\\maker\\program\\sound", b"VOLUME"),
        Ok(7),
        "paths and names compare without case"
    );
    assert_eq!(
        value(&reopened, b"Software\\Maker\\Program", b"First"),
        Ok(1)
    );
}

#[test]
fn the_first_values_answer_before_any_save() {
    keep(b"");
    assert_eq!(
        value(page(b""), b"Software\\Maker\\Program", b"First"),
        Ok(1)
    );
    assert!(saved().is_empty(), "a read saves nothing");
}

#[test]
fn a_missing_key_or_value_answers_2() {
    let page = page(b"");
    assert_eq!(value(&page, b"Software\\Nobody", b"First"), Err(2));
    assert_eq!(
        value(&page, b"Software\\Maker\\Program", b"Nothing"),
        Err(2)
    );
    assert_eq!(
        value(&page, b"Software\\Maker", b"First"),
        Err(2),
        "a parent opens and holds no value"
    );
}

#[test]
fn a_key_opens_under_a_held_key() {
    let page = page(b"");
    let front = Front::new(&ROW, &page);
    let maker = front
        .open_key(ROOT, b"Software\\Maker", 0, 1)
        .expect("a parent opens");
    let program = front
        .open_key(maker, b"Program", 0, 1)
        .expect("a key opens under it");
    let mut data = [0; 4];
    assert_eq!(
        front.query_value(program, b"First", 0, Some(SIZE), Some(&mut data)),
        Queried {
            code: 0,
            wrote: Some((DWORD, SIZE))
        }
    );
    assert_eq!(u32::from_le_bytes(data), 1);
}

#[test]
fn another_root_answers_2_and_a_stray_handle_6_each_logged() {
    let page = page(b"");
    let front = Front::new(&ROW, &page);
    let lines = debug_lines(|| {
        assert_eq!(front.open_key(0x8000_0001, b"Software", 0, 1), Err(2));
        assert_eq!(front.open_key(0x1234, b"Software", 0, 1), Err(6));
        assert_eq!(
            front
                .query_value(0x8000_0001, b"First", 0, Some(SIZE), None)
                .code,
            2
        );
        assert_eq!(front.set_value(0x1234, b"First", DWORD, &[1, 0, 0, 0]), 6);
        assert_eq!(front.close_key(0x8000_0001), 0);
        assert_eq!(front.close_key(0x1234), 6);
    });
    assert_eq!(
        lines,
        [
            "imp.call c=open a=80000001,0,1 s=Software o=\"\" v=2 by=kernel-registry",
            "imp.call c=open a=1234,0,1 s=Software o=\"\" v=6 by=kernel-registry",
            "imp.call c=query a=80000001,0,4 s=First o=\"\" v=2 by=kernel-registry",
            "imp.call c=set a=1234,4,4 s=First o=\"\" v=6 by=kernel-registry",
            "imp.call c=close a=80000001 v=0 by=kernel-registry",
            "imp.call c=close a=1234 v=6 by=kernel-registry",
        ]
    );
}

#[test]
fn a_type_size_root_or_option_past_the_store_answers_87_logged() {
    let page = page(b"");
    let front = Front::new(&ROW, &page);
    let key = front
        .open_key(ROOT, ROW.key, 0, 1)
        .expect("the row's key opens");
    let lines = debug_lines(|| {
        assert_eq!(front.set_value(key, b"Name", 1, b"text\0"), 87);
        assert_eq!(front.set_value(key, b"Name", DWORD, &[1, 0]), 87);
        assert_eq!(front.set_value(ROOT, b"Name", DWORD, &[1, 0, 0, 0]), 87);
        assert_eq!(front.open_key(ROOT, ROW.key, 1, 1), Err(87));
    });
    assert_eq!(
        lines,
        [
            "imp.call c=set a=100,1,5 s=Name o=\"\" v=57 by=kernel-registry",
            "imp.call c=set a=100,4,2 s=Name o=\"\" v=57 by=kernel-registry",
            "imp.call c=set a=80000002,4,4 s=Name o=\"\" v=57 by=kernel-registry",
            "imp.call c=open a=80000002,1,1 s=Software\\Maker\\Program o=\"\" v=57 by=kernel-registry",
        ]
    );
    assert_eq!(
        value(&page, ROW.key, b"Name"),
        Err(2),
        "nothing refused is kept"
    );
}

#[test]
fn a_query_answers_each_shape_it_is_handed() {
    let page = page(b"");
    let front = Front::new(&ROW, &page);
    let key = front
        .open_key(ROOT, ROW.key, 0, 1)
        .expect("the row's key opens");
    let mut short = [0; 2];
    let lines = debug_lines(|| {
        assert_eq!(
            front.query_value(key, b"First", 0, Some(2), Some(&mut short)),
            Queried {
                code: 234,
                wrote: Some((DWORD, SIZE))
            }
        );
        assert_eq!(
            front.query_value(key, b"First", 0, Some(0), None),
            Queried {
                code: 0,
                wrote: Some((DWORD, SIZE))
            }
        );
        assert_eq!(
            front.query_value(key, b"First", 1, Some(SIZE), None).code,
            87
        );
        let mut data = [0; 4];
        assert_eq!(
            front
                .query_value(key, b"First", 0, None, Some(&mut data))
                .code,
            87
        );
    });
    assert_eq!(short, [0, 0], "a short buffer is not written");
    assert_eq!(
        lines,
        [
            "imp.call c=query a=100,0,2 s=First o=4,4 v=ea by=kernel-registry",
            "imp.call c=query a=100,0,0 s=First o=4,4 v=0 by=kernel-registry",
            "imp.call c=query a=100,1,4 s=First o=\"\" v=57 by=kernel-registry",
            "imp.call c=query a=100,0,0 s=First o=\"\" v=57 by=kernel-registry",
        ]
    );
}

#[test]
fn a_name_the_store_cannot_hold_answers_1013() {
    let page = page(b"");
    let front = Front::new(&ROW, &page);
    let key = front
        .open_key(ROOT, ROW.key, 0, 1)
        .expect("the row's key opens");
    assert_eq!(
        front.set_value(key, b"Two\tWords", DWORD, &[1, 0, 0, 0]),
        1013
    );
    assert_eq!(front.create_key(ROOT, b"Two\nLines", 0, 0), Err(1013));
}

#[test]
fn a_save_that_fails_answers_1013_and_the_value_stays_held() {
    let folder = Folder::new("unsaved");
    fs::create_dir_all(folder.0.join("settings.txt.new")).expect("the blocking folder is made");
    let file = SettingsFile::in_folder(&folder.0, &ROW).expect("no settings file reads as none");
    let front = Front::new(&ROW, &file);
    let key = front
        .open_key(ROOT, ROW.key, 0, 1)
        .expect("the row's key opens");
    assert_eq!(front.set_value(key, b"First", DWORD, &[5, 0, 0, 0]), 1013);
    assert!(!folder.0.join("settings.txt").exists());
    assert_eq!(value(&file, ROW.key, b"First"), Ok(5));
}

#[test]
fn a_file_not_in_the_settings_form_is_refused() {
    let folder = Folder::new("malformed");
    fs::create_dir_all(&folder.0).expect("the folder is made");
    fs::write(folder.0.join("settings.txt"), b"Key\tName\tsz:words\n")
        .expect("the file is written");
    assert_eq!(
        SettingsFile::in_folder(&folder.0, &ROW)
            .err()
            .map(|error| error.kind()),
        Some(io::ErrorKind::InvalidData)
    );
}
