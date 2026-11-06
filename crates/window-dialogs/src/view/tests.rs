// SPDX-License-Identifier: MIT
//! The view over a planted file; the numbers and words are the crate's own.

use alloc::string::String;
use alloc::vec::Vec;

use kernel_process::{plant, resource};
use window::Rect;
use window::desk::{Desk, Window};
use window::page::Page;

use super::{BarView, BoxView, ControlView, Kind, MenuRow, RowKind, bar, open_box, shown};
use crate::front::Front;
use crate::tables::Tables;
use crate::tests::ROW;
use crate::{Resources, plant as templates};

const INSTANCE: u32 = 0x7000;
const MENU: u32 = 7;
const BOX: u32 = 9;
const OWNER: u32 = 0x77;
const VK_TAB: u8 = 0x09;

struct Disc(Vec<u8>);

impl Resources for Disc {
    fn resource(&self, _: u32, kind: u32, id: u32) -> Option<&[u8]> {
        resource::find(&self.0, kind, id)
    }
}

fn disc() -> Disc {
    let menu = templates::menu(&[
        (0x10, 0, "&File"),
        (0x01, 0x10, "&Open\tCtrl+O"),
        (0x00, 0, ""),
        (0x88, 0x11, "Save && go"),
        (0x91, 0, "&Help"),
        (0x80, 0x12, "About"),
    ]);
    let controls = [
        (1, 0x80, 0x5001_0001, "&OK"),
        (2, 0x80, 0x5001_0000, "Cancel"),
        (0xffff, 0x82, 0x5002_0001, "A label"),
        (0x30, 0x80, 0x5001_0003, "Pick me"),
    ];
    let boxed = templates::dialog(0x90c8_00c0, "A & title", &controls);
    Disc(plant::image(&[
        (resource::MENU, MENU, &menu),
        (resource::DIALOG, BOX, &boxed),
    ]))
}

fn stage(test: impl FnOnce(&Desk<Page>, &Tables, &Front<'_, Page, Disc>)) {
    let (desk, tables, disc) = (Desk::new(Page::new(640, 480)), Tables::new(), disc());
    desk.state().windows.push(Window {
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
    test(&desk, &tables, &Front::new(&ROW, &desk, &tables, &disc));
}

/// The box opened with the focus on its first control, by an init answering 1.
fn opened(tables: &Tables, front: &Front<'_, Page, Disc>) -> u32 {
    assert_eq!(
        front.dialog_box_param([INSTANCE, BOX, OWNER, 0, 0], |_, _| 1),
        0
    );
    tables.state().boxes[0].handle
}

#[test]
fn a_box_shows_each_controls_kind_words_place_and_the_focus() {
    stage(|desk, tables, front| {
        let handle = opened(tables, front);
        let view = open_box(desk, tables, handle).expect("the box is open");
        assert_eq!(view.title, "A & title");
        assert_eq!(view.place, Rect::sized(0, 0, 10, 10));
        let kinds: Vec<Kind> = view.controls.iter().map(|control| control.kind).collect();
        assert_eq!(
            kinds,
            [Kind::Default, Kind::Button, Kind::Label, Kind::Check]
        );
        let words: Vec<&str> = view.controls.iter().map(|c| c.words.as_str()).collect();
        assert_eq!(words, ["OK", "Cancel", "A label", "Pick me"]);
        let focused: Vec<bool> = view.controls.iter().map(|c| c.focused).collect();
        assert_eq!(focused, [true, false, false, false]);
        assert_eq!(view.controls[3].place, Rect::sized(0, 0, 10, 10));
    });
}

#[test]
fn the_view_follows_a_press_and_a_tab() {
    stage(|desk, tables, front| {
        let handle = opened(tables, front);
        assert!(front.press(handle, 0x30));
        assert!(front.key(handle, VK_TAB));
        let view = open_box(desk, tables, handle).expect("the box is open");
        assert!(view.controls[3].checked);
        assert!(view.controls[1].focused);
    });
}

#[test]
fn a_closed_box_shows_nothing_and_neither_does_one_never_open() {
    stage(|desk, tables, front| {
        let handle = opened(tables, front);
        assert_eq!(open_box(desk, tables, handle + 99), None);
        assert_eq!(front.end_dialog(handle, 1), 1);
        assert_eq!(open_box(desk, tables, handle), None);
    });
}

#[test]
fn a_bar_shows_its_rows_and_follows_the_fronts() {
    stage(|_, tables, front| {
        assert_eq!(bar(tables, OWNER), None, "no menu set yet");
        let menu = front.load_menu(INSTANCE, MENU);
        assert_ne!(front.set_menu(OWNER, menu), 0);
        let view = bar(tables, OWNER).expect("the menu is set");
        let top: Vec<RowKind> = view.rows.iter().map(|row| row.kind).collect();
        assert_eq!(top, [RowKind::Popup, RowKind::Popup]);
        assert!(view.rows[0].enabled);
        assert!(!view.rows[1].enabled, "a popup's own state");
        let file = &view.rows[0].rows;
        assert_eq!(file[0].words, "Open\tCtrl+O");
        assert!(!file[0].enabled);
        assert_eq!(file[1].kind, RowKind::Separator);
        assert_eq!(file[2].words, "Save & go");
        assert!(file[2].checked);
        assert_eq!(view.drawn, 0);
        assert_eq!(front.enable_menu_item(menu, 0x10, 0), 1, "the state before");
        assert_ne!(front.draw_menu_bar(OWNER), 0);
        let view = bar(tables, OWNER).expect("the menu is set");
        assert!(view.rows[0].rows[0].enabled);
        assert_eq!(view.drawn, 1);
    });
}

#[test]
fn shown_takes_each_marker_and_keeps_a_doubled_one() {
    for (words, wanted) in [
        ("&File", "File"),
        ("Save && go", "Save & go"),
        ("end&", "end"),
        ("plain", "plain"),
    ] {
        assert_eq!(shown(words), wanted);
    }
}

#[test]
fn a_box_writes_one_record_a_line_with_its_words_escaped() {
    let view = BoxView {
        handle: 5,
        title: String::from("a\tb"),
        place: Rect::sized(1, 2, 3, 4),
        controls: alloc::vec![ControlView {
            handle: 6,
            id: 7,
            kind: Kind::Check,
            words: String::from("c\\d\ne"),
            checked: true,
            focused: false,
            place: Rect::sized(0, 0, 1, 1),
        }],
    };
    assert_eq!(
        view.lines(),
        "box\t5\t1\t2\t4\t6\ta\\tb\ncheck\t6\t7\t1\t0\t0\t0\t1\t1\tc\\\\d\\ne\n"
    );
}

#[test]
fn a_bar_writes_a_popups_rows_after_it_one_deeper() {
    let row = |kind, id, words: &str, rows| MenuRow {
        id,
        kind,
        words: String::from(words),
        enabled: true,
        checked: false,
        rows,
    };
    let inner = alloc::vec![
        row(RowKind::Item, 9, "x", Vec::new()),
        row(RowKind::Separator, 0, "", Vec::new()),
    ];
    let view = BarView {
        window: 3,
        drawn: 2,
        rows: alloc::vec![row(RowKind::Popup, 0, "p", inner)],
    };
    assert_eq!(
        view.lines(),
        "bar\t3\t2\npopup\t0\t0\t1\t0\tp\nitem\t1\t9\t1\t0\tx\nseparator\t1\t0\t1\t0\t\n"
    );
}
