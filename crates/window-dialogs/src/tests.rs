// SPDX-License-Identifier: MIT
//! The fronts over a planted file. The numbers are the crate's own, no game's.

use kernel_process::{plant, resource};
use window::desk::{Desk, Window};
use window::page::Page;
use window::table::{Calls as WindowCalls, Row as WindowRow};
use window::{Event, Message, Rect};

use crate::front::Front;
use crate::table::{Calls, Row};
use crate::tables::Tables;
use crate::test_log::debug_lines;
use crate::{Resources, plant as templates};

const INSTANCE: u32 = 0x7000;
const MENU: u32 = 7;
const BOX: u32 = 9;
const OWNER: u32 = 0x77;
const NEVER: u32 = 99;
const AUTO: u32 = 0x5001_0003;
const DEFAULT: u32 = 0x5001_0001;
const LABELLED: u32 = 10;
const PLAIN: u32 = 0x5001_0000;
/// `STATIC` styles: the low digit 1 is `SS_CENTER`, as a real template holds
/// it, and 3 is `SS_ICON`. On a button those digits are the default push button
/// and the auto check box.
const CENTERED: u32 = 0x5002_0001;
const ICON: u32 = 0x5002_0003;

pub(crate) static ROW: Row = Row {
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

static WINDOW_ROW: WindowRow = WindowRow {
    file: "planted",
    calls: &WindowCalls {
        register_class: "a",
        create_window: "a",
        destroy_window: "a",
        find_window: "a",
        load_icon: "a",
        show_window: "a",
        set_window_pos: "a",
        adjust_window_rect: "a",
        get_window_rect: "a",
        get_window_long: "a",
        set_window_long: "a",
        client_to_screen: "a",
        screen_to_client: "a",
        set_focus: "SetFocus",
        get_system_metrics: "a",
        def_window_proc: "a",
        peek_message: "a",
        wait_message: "a",
        translate_message: "a",
        dispatch_message: "a",
        send_message: "a",
        post_message: "a",
        post_quit_message: "a",
        get_message_time: "a",
    },
    metrics: &[],
};

struct Disc(Vec<u8>);

impl Resources for Disc {
    fn resource(&self, instance: u32, kind: u32, id: u32) -> Option<&[u8]> {
        (instance == INSTANCE)
            .then(|| resource::find(&self.0, kind, id))
            .flatten()
    }
}

fn disc() -> Disc {
    let menu = templates::menu(&[
        (0x10, 0, "First"),
        (0x01, 41, "Gray"),
        (0x82, 42, "Disabled"),
        (0x90, 0, "Last"),
        (0x80, 43, "Plain"),
    ]);
    let boxed = templates::dialog(
        0,
        "Title",
        &[
            (1, 0x80, DEFAULT, "Yes"),
            (11, 0x80, AUTO, "One"),
            (12, 0x80, AUTO, "Two"),
        ],
    );
    let labelled = templates::dialog(
        0,
        "",
        &[
            (0xffff, 0x82, CENTERED, "Label"),
            (20, 0x82, ICON, ""),
            (1, 0x80, PLAIN, "Yes"),
        ],
    );
    Disc(plant::image(&[
        (resource::MENU, MENU, &menu),
        (resource::DIALOG, BOX, &boxed),
        (resource::DIALOG, LABELLED, &labelled),
    ]))
}

/// Runs `test` over one program with an owner window holding the focus.
fn program(test: impl FnOnce(&Desk<Page>, &Front<'_, Page, Disc>)) {
    let (desk, tables, disc) = (Desk::new(Page::new(0x280, 0x1e0)), Tables::new(), disc());
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
    desk.state().focus = OWNER;
    test(&desk, &Front::new(&ROW, &desk, &tables, &disc));
}

fn open(front: &Front<'_, Page, Disc>, answer: u32) -> Message {
    let mut init = Message::default();
    front.dialog_box_param([INSTANCE, BOX, OWNER, 0x50, 0x33], |_, message| {
        init = *message;
        answer
    });
    init
}

#[test]
fn a_controls_handle_is_one_set_focus_takes() {
    program(|desk, front| {
        let init = open(front, 0);
        let handle = front.get_dlg_item(init.window, 1);
        assert_ne!(handle, 0);
        assert_eq!(front.get_dlg_item(init.window, NEVER), 0);
        assert_eq!(
            window::front::Front::new(&WINDOW_ROW, desk).set_focus(handle),
            OWNER
        );
        assert_eq!(desk.state().focus, handle);
    });
}

#[test]
fn is_dlg_button_checked_answers_exactly_1_or_0() {
    program(|_, front| {
        let window = open(front, 0).window;
        assert_eq!(front.is_dlg_button_checked(window, 11), 0);
        assert_eq!(front.check_dlg_button(window, 11, 1), 1);
        assert_eq!(front.is_dlg_button_checked(window, 11), 1);
        assert_eq!(front.check_dlg_button(window, 11, 0), 1);
        assert_eq!(front.is_dlg_button_checked(window, 11), 0);
        assert_eq!(front.check_dlg_button(window, NEVER, 1), 0);
        assert_eq!(front.is_dlg_button_checked(window, NEVER), 0);
    });
}

#[test]
fn a_loaded_menus_handle_is_one_set_menu_takes() {
    program(|_, front| {
        let menu = front.load_menu(INSTANCE, MENU);
        assert_ne!(menu, 0);
        assert_eq!(front.set_menu(OWNER, menu), 1);
        assert_eq!(front.set_menu(OWNER, 0), 1);
        assert_eq!(front.set_menu(OWNER, NEVER), 0);
        assert_eq!(front.set_menu(NEVER, menu), 0);
        assert_eq!(front.load_menu(INSTANCE, NEVER), 0);
        assert_eq!(front.load_menu(NEVER, MENU), 0);
    });
}

#[test]
fn an_item_answers_its_state_before_and_one_not_there_answers_minus_1() {
    program(|_, front| {
        let menu = front.load_menu(INSTANCE, MENU);
        assert_eq!(front.enable_menu_item(menu, 41, 0), 1);
        assert_eq!(front.enable_menu_item(menu, 41, 1), 0);
        assert_eq!(front.enable_menu_item(menu, 41, 0), 1);
        assert_eq!(front.enable_menu_item(menu, 42, 1), 2);
        assert_eq!(front.enable_menu_item(menu, 1, 0x401), 0);
        assert_eq!(front.enable_menu_item(menu, 1, 0x400), 1);
        assert_eq!(front.enable_menu_item(menu, NEVER, 1), u32::MAX);
        assert_eq!(front.check_menu_item(menu, NEVER, 8), u32::MAX);
        assert_eq!(front.check_menu_item(menu, 43, 8), 0);
        assert_eq!(front.check_menu_item(menu, 43, 0), 8);
    });
}

#[test]
fn an_auto_check_box_flips_before_its_command_is_queued() {
    program(|desk, front| {
        let window = open(front, 0).window;
        assert!(front.press(window, 11));
        assert_eq!(front.is_dlg_button_checked(window, 11), 1);
        let sent = desk
            .state()
            .queue
            .pop_front()
            .expect("the press queued one");
        assert_eq!(
            (sent.window, sent.message, sent.w, sent.l),
            (window, 0x111, 11, 0)
        );
        assert!(front.press(window, 1));
        assert_eq!(front.is_dlg_button_checked(window, 1), 0);
    });
}

#[test]
fn init_takes_the_first_control_and_param_and_0_leaves_the_focus() {
    program(|desk, front| {
        let init = open(front, 0);
        assert_eq!(init.message, 0x110);
        assert_eq!(init.w, front.get_dlg_item(init.window, 1));
        assert_eq!(init.l, 0x33);
        assert_eq!(desk.state().focus, OWNER);
    });
    program(|desk, front| {
        let init = open(front, 1);
        assert_eq!(desk.state().focus, init.w);
    });
}

#[test]
fn return_reaches_the_default_and_escape_reaches_cancel() {
    program(|desk, front| {
        let window = open(front, 0).window;
        assert!(front.press(window, 11));
        desk.state().queue.clear();
        assert!(front.key(window, 0x0d));
        assert_eq!(desk.state().queue.pop_front().map(|sent| sent.w), Some(1));
        assert!(front.key(window, 0x1b));
        assert_eq!(desk.state().queue.pop_front().map(|sent| sent.w), Some(2));
        assert!(!front.key(window, b'Q'));
    });
}

#[test]
fn a_labels_style_digit_makes_it_no_button() {
    program(|desk, front| {
        let mut window = 0;
        front.dialog_box_param([INSTANCE, LABELLED, OWNER, 0x50, 0], |_, init| {
            window = init.window;
            0
        });
        assert!(front.key(window, 0x0d));
        let sent = desk.state().queue.pop_front().map(|sent| sent.w);
        assert_eq!(sent, Some(1), "Return reaches IDOK, not the centered label");
        assert!(front.press(window, 20));
        assert_eq!(
            front.is_dlg_button_checked(window, 20),
            0,
            "an icon flips no check"
        );
        desk.state().focus = front.get_dlg_item(window, 0xffff);
        desk.state().queue.clear();
        assert!(!front.key(window, 0x20), "Space presses no label");
        assert!(desk.state().queue.is_empty());
    });
}

const WM_INITDIALOG: u32 = 0x110;
const WM_COMMAND: u32 = 0x111;
const WM_CLOSE: u32 = 0x10;
const ESCAPE: u8 = 0x1b;

/// One key's down and up, handed to the back as a host would: on the owner.
fn tap(desk: &Desk<Page>, key: u8) {
    for down in [true, false] {
        desk.screen().hand(Event::Key {
            window: OWNER,
            key,
            down,
        });
    }
}

#[test]
fn the_wait_answers_end_dialogs_value_and_closes_the_box() {
    program(|desk, front| {
        tap(desk, ESCAPE);
        let mut seen = Vec::new();
        let answer =
            front.dialog_box_wait([INSTANCE, BOX, OWNER, 0x50, 0x33], |procedure, sent| {
                seen.push((procedure, sent.message, sent.w & 0xffff));
                if sent.message == WM_COMMAND {
                    assert_eq!(front.end_dialog(sent.window, 7), 1);
                }
                0
            });
        assert_eq!(answer, 7);
        let init = (0x50, WM_INITDIALOG, seen[0].2);
        assert_eq!(seen, [init, (0x50, WM_COMMAND, 2)]);
        assert_eq!(desk.state().focus, OWNER);
    });
}

#[test]
fn the_keyboard_alone_flips_a_check_the_procedure_reads_back() {
    program(|desk, front| {
        tap(desk, 0x09);
        tap(desk, 0x20);
        let answer = front.dialog_box_wait([INSTANCE, BOX, OWNER, 0x50, 0], |_, sent| {
            if sent.message == WM_COMMAND {
                assert_eq!(
                    sent.w, 11,
                    "Tab moved off the default, Space pressed the next"
                );
                let checked = front.is_dlg_button_checked(sent.window, 11);
                assert_eq!(front.end_dialog(sent.window, checked), 1);
            }
            u32::from(sent.message == WM_INITDIALOG)
        });
        assert_eq!(answer, 1);
    });
}

#[test]
fn the_wait_takes_a_command_handed_to_the_box() {
    program(|desk, front| {
        let answer = front.dialog_box_wait([INSTANCE, BOX, OWNER, 0x50, 0], |_, sent| {
            if sent.message == WM_INITDIALOG {
                let (window, id) = (sent.window, 12);
                desk.screen().hand(Event::Command { window, id });
            } else {
                assert_eq!(front.end_dialog(sent.window, sent.w), 1);
            }
            0
        });
        assert_eq!(answer, 12);
    });
}

#[test]
fn the_wait_leaves_the_owners_other_messages_queued_and_sends_it_nothing() {
    program(|desk, front| {
        let window = OWNER;
        desk.screen().hand(Event::Close { window });
        tap(desk, ESCAPE);
        desk.screen().hand(Event::Active {
            window,
            active: true,
        });
        let mut windows = Vec::new();
        let answer = front.dialog_box_wait([INSTANCE, BOX, OWNER, 0x50, 0], |_, sent| {
            windows.push(sent.window);
            if sent.message == WM_COMMAND {
                assert_eq!(front.end_dialog(sent.window, 3), 1);
            }
            0
        });
        assert_eq!(answer, 3);
        assert!(!windows.contains(&OWNER));
        let left: Vec<u32> = desk.state().queue.iter().map(|sent| sent.message).collect();
        assert_eq!(left, [WM_CLOSE, 0x1c], "in order, and no key of the box's");
    });
}

#[test]
fn a_box_closed_by_its_init_is_never_waited_on() {
    program(|_, front| {
        let answer = front.dialog_box_wait([INSTANCE, BOX, OWNER, 0x50, 0], |_, sent| {
            assert_eq!(front.end_dialog(sent.window, 9), 1);
            0
        });
        assert_eq!(answer, 9);
    });
}

#[test]
fn the_wait_on_a_template_the_module_lacks_answers_minus_1() {
    program(|_, front| {
        let answer = front.dialog_box_wait([INSTANCE, NEVER, OWNER, 0, 0], |_, _| 1);
        assert_eq!(answer, u32::MAX);
    });
}

#[test]
fn the_wait_writes_box_wait_then_the_calls_one_line_with_the_value() {
    program(|desk, front| {
        tap(desk, ESCAPE);
        let lines = debug_lines(|| {
            let answer = front.dialog_box_wait([INSTANCE, BOX, OWNER, 0x50, 0], |_, sent| {
                if sent.message == WM_COMMAND {
                    assert_eq!(front.end_dialog(sent.window, 4), 1);
                }
                0
            });
            assert_eq!(answer, 4);
        });
        let events: Vec<&str> = lines
            .iter()
            .map(|line| line.split_whitespace().next().expect("invariant: an event"))
            .collect();
        assert_eq!(events, ["box.wait", "box.press", "imp.call", "imp.call"]);
        assert!(lines[3].contains("DialogBoxParamA"), "{}", lines[3]);
    });
}

#[test]
fn a_box_with_a_font_reads_its_controls_past_the_face() {
    let boxed = templates::dialog(0x90c8_00c0, "Title", &[(1, 0x80, DEFAULT, "Yes")]);
    let read = crate::template::dialog(&boxed).expect("invariant: planted whole");
    assert_eq!(read.title, "Title");
    assert_eq!(read.controls[0].id, 1);
    assert_eq!(read.controls[0].words, "Yes");
}

#[test]
fn end_dialog_closes_at_once_and_each_open_makes_new_handles() {
    program(|desk, front| {
        let first = open(front, 0).window;
        let control = front.get_dlg_item(first, 1);
        desk.state().focus = control;
        assert_eq!(front.end_dialog(first, 5), 1);
        assert_eq!(desk.state().focus, OWNER);
        assert!(desk.state().window(control).is_none());
        assert_eq!(front.end_dialog(first, 5), 0);
        assert!(!front.press(first, 1));
        assert_ne!(open(front, 0).window, first);
    });
}

#[test]
fn a_template_the_module_lacks_answers_minus_1_and_calls_nothing() {
    program(|_, front| {
        let answer = front.dialog_box_param([INSTANCE, NEVER, OWNER, 0, 0], |_, _| 1);
        assert_eq!(answer, u32::MAX);
    });
}

#[test]
fn each_front_writes_one_imp_call_line() {
    program(|_, front| {
        let window = open(front, 0).window;
        let menu = front.load_menu(INSTANCE, MENU);
        let calls: [&dyn Fn() -> u32; 10] = [
            &|| front.load_menu(INSTANCE, MENU),
            &|| front.set_menu(OWNER, menu),
            &|| front.draw_menu_bar(OWNER),
            &|| front.enable_menu_item(menu, 41, 1),
            &|| front.check_menu_item(menu, 41, 8),
            &|| front.get_dlg_item(window, 1),
            &|| front.check_dlg_button(window, 11, 1),
            &|| front.is_dlg_button_checked(window, 11),
            &|| front.dialog_box_param([INSTANCE, BOX, OWNER, 0, 0], |_, _| 0),
            &|| front.end_dialog(window, 0),
        ];
        for call in calls {
            let lines = debug_lines(|| {
                call();
            });
            assert_eq!(lines.len(), 1, "{lines:?}");
            assert!(lines[0].starts_with("imp.call"), "{lines:?}");
        }
    });
}

#[test]
fn a_press_writes_box_press_alone_and_a_box_not_open_writes_nothing() {
    program(|_, front| {
        let window = open(front, 0).window;
        for lines in [
            debug_lines(|| assert!(front.press(window, 11))),
            debug_lines(|| assert!(front.key(window, 0x0d))),
        ] {
            assert_eq!(lines.len(), 1, "{lines:?}");
            assert!(lines[0].starts_with("box.press"), "{lines:?}");
        }
        let none = debug_lines(|| assert!(!front.press(NEVER, 1)));
        assert!(none.is_empty(), "{none:?}");
    });
}
