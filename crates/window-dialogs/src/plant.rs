// SPDX-License-Identifier: MIT
//! Planted templates, built in code for a test or a page's walk, so no
//! template's bytes lie in this tree and no test needs a player's disc.
//!
//! Call: [`menu`] `(rows) -> Vec<u8>`, [`dialog`] `(style, title, controls) ->
//! Vec<u8>`. Hand them to `kernel_process::plant::image` to make a file.

use alloc::vec::Vec;

/// The bit that says a row opens a popup: it writes no id.
const MF_POPUP: u16 = 0x10;
/// The style bit that says a size and a face follow the title.
const DS_SETFONT: u32 = 0x40;

fn text(into: &mut Vec<u8>, words: &str) {
    for unit in words.encode_utf16().chain([0]) {
        into.extend_from_slice(&unit.to_le_bytes());
    }
}

/// A menu template: each row its options, its id and its words, in the order
/// the template lists them, `MF_END` the caller's to set.
#[must_use]
pub fn menu(rows: &[(u16, u16, &str)]) -> Vec<u8> {
    let mut out = alloc::vec![0; 4];
    for (options, id, words) in rows {
        out.extend_from_slice(&options.to_le_bytes());
        if options & MF_POPUP == 0 {
            out.extend_from_slice(&id.to_le_bytes());
        }
        text(&mut out, words);
    }
    out
}

/// A box's template: each control its id, its class ordinal, its style and its
/// words, the box and each control at place 0 and of size 10. A style with
/// `DS_SETFONT` writes a size and a face, as a real template does: nothing reads them.
#[must_use]
pub fn dialog(style: u32, title: &str, controls: &[(u16, u16, u32, &str)]) -> Vec<u8> {
    let mut out = Vec::new();
    let place = [0i16, 0, 10, 10];
    out.extend_from_slice(&style.to_le_bytes());
    out.extend_from_slice(&[0; 4]);
    let count = u16::try_from(controls.len()).expect("invariant: a planted box is small");
    out.extend_from_slice(&count.to_le_bytes());
    for side in place {
        out.extend_from_slice(&side.to_le_bytes());
    }
    out.extend_from_slice(&[0; 4]);
    text(&mut out, title);
    if style & DS_SETFONT != 0 {
        out.extend_from_slice(&8u16.to_le_bytes());
        text(&mut out, "Face");
    }
    for (id, class, style, words) in controls {
        out.resize(out.len().next_multiple_of(4), 0);
        out.extend_from_slice(&style.to_le_bytes());
        out.extend_from_slice(&[0; 4]);
        for side in place {
            out.extend_from_slice(&side.to_le_bytes());
        }
        out.extend_from_slice(&id.to_le_bytes());
        out.extend_from_slice(&0xffffu16.to_le_bytes());
        out.extend_from_slice(&class.to_le_bytes());
        text(&mut out, words);
        out.extend_from_slice(&[0; 2]);
    }
    out
}
