// SPDX-License-Identifier: MIT
//! The winmm back over a fake winmm: a code passes up as nothing there, a good answer
//! maps field by field, and each call hands winmm what it asks for.
#![allow(unsafe_code)]

use std::cell::Cell;

use crate::pads::{Pads, Reading, Shape, Span};
use crate::winmm::{JoyCaps, JoyInfoEx, Through, Winmm};

/// Answers with `code`, filling each record with numbers no two fields share.
struct Fake {
    code: u32,
    caps_size: Cell<u32>,
    position_size: Cell<u32>,
    position_flags: Cell<u32>,
}

fn fake(code: u32) -> Through<Fake> {
    Through(Fake {
        code,
        caps_size: Cell::new(0),
        position_size: Cell::new(0),
        position_flags: Cell::new(0),
    })
}

impl Winmm for Fake {
    fn devices(&self) -> u32 {
        16
    }

    unsafe fn caps(&self, _device: usize, record: *mut JoyCaps, size: u32) -> u32 {
        self.caps_size.set(size);
        // SAFETY: the trait's own, and nothing else holds the record during the call.
        let record = unsafe { &mut *record };
        record.x_min = 1;
        record.x_max = 2;
        record.y_min = 3;
        record.y_max = 4;
        record.z_min = 5;
        record.z_max = 6;
        record.r_min = 7;
        record.r_max = 8;
        record.u_min = 9;
        record.u_max = 10;
        record.v_min = 11;
        record.v_max = 12;
        record.buttons = 13;
        record.caps = 14;
        record.axes = 15;
        self.code
    }

    unsafe fn position(&self, _device: u32, record: *mut JoyInfoEx) -> u32 {
        // SAFETY: the trait's own, and nothing else holds the record during the call.
        let record = unsafe { &mut *record };
        self.position_size.set(record.size);
        self.position_flags.set(record.flags);
        record.x = 21;
        record.y = 22;
        record.z = 23;
        record.r = 24;
        record.u = 25;
        record.v = 26;
        record.buttons = 27;
        record.button_number = 28;
        record.pov = 29;
        self.code
    }
}

#[test]
fn a_nonzero_code_is_nothing_there() {
    for code in [1, 165, 167] {
        let pads = fake(code);
        assert_eq!(pads.shape(0), None, "code {code}");
        assert_eq!(pads.reading(0), None, "code {code}");
    }
}

#[test]
fn the_count_is_winmm_s() {
    assert_eq!(fake(0).devices(), 16);
}

#[test]
fn a_good_shape_maps_field_by_field_and_hands_the_record_s_size() {
    let pads = fake(0);
    assert_eq!(
        pads.shape(0),
        Some(Shape {
            spans: [
                Span { min: 1, max: 2 },
                Span { min: 3, max: 4 },
                Span { min: 5, max: 6 },
                Span { min: 7, max: 8 },
                Span { min: 9, max: 10 },
                Span { min: 11, max: 12 },
            ],
            caps: 14,
            axes: 15,
            buttons: 13,
        }),
    );
    assert_eq!(pads.0.caps_size.get(), 404);
}

#[test]
fn a_good_reading_maps_field_by_field_and_asks_for_everything() {
    let pads = fake(0);
    assert_eq!(
        pads.reading(0),
        Some(Reading {
            axes: [21, 22, 23, 24, 25, 26],
            buttons: 27,
            pressed: 28,
            pov: 29
        }),
    );
    assert_eq!(pads.0.position_size.get(), 0x34);
    assert_eq!(pads.0.position_flags.get(), 0x4ff);
}
