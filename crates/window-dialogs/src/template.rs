// SPDX-License-Identifier: MIT
//! The two template readers, portable: a menu's and a box's bytes to what the
//! tables hold. The older forms alone, the ones measured: an extended template
//! answers `None`, as do bytes that end early.
//!
//! Call: [`menu`] `(bytes) -> Option<Vec<Item>>`, [`dialog`] `(bytes) ->
//! Option<Dialog>`, [`find_item`] `(items, item, by_place) -> Option<&mut Item>`.

use alloc::string::String;
use alloc::vec::Vec;

/// `MF_POPUP` and `MF_END`: the template's own bits, kept off an item's state.
const MF_POPUP: u16 = 0x10;
const MF_END: u16 = 0x80;
/// `DS_SETFONT`: a size and a face follow the title.
const DS_SETFONT: u32 = 0x40;
/// A menu nests no deeper than this: bad bytes never run the stack out.
const DEEPEST: u32 = 8;

/// One menu item: `state` holds its `MF_GRAYED`, `MF_DISABLED` and `MF_CHECKED`
/// bits; a popup has no id of its own, so `id` is 0.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Item {
    pub id: u16,
    pub state: u32,
    pub words: String,
    pub popup: bool,
    pub items: Vec<Item>,
}

/// One control in a box's template. `class` is the name Windows gives its
/// ordinal, `place` left, top, width and height in the template's dialog units.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Control {
    pub id: u16,
    pub class: String,
    pub style: u32,
    pub ex_style: u32,
    pub place: [i16; 4],
    pub words: String,
}

/// A box's template.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Dialog {
    pub style: u32,
    pub ex_style: u32,
    pub place: [i16; 4],
    pub title: String,
    pub controls: Vec<Control>,
}

/// The bytes, read forward; each read answers `None` past their end.
struct Reader<'a> {
    bytes: &'a [u8],
    at: usize,
}

impl Reader<'_> {
    fn word(&mut self) -> Option<u16> {
        let found = self.bytes.get(self.at..self.at.checked_add(2)?)?;
        self.at += 2;
        Some(u16::from_le_bytes(found.try_into().ok()?))
    }

    fn long(&mut self) -> Option<u32> {
        let low = u32::from(self.word()?);
        Some(low | u32::from(self.word()?) << 16)
    }

    fn place(&mut self) -> Option<[i16; 4]> {
        let mut place = [0; 4];
        for side in &mut place {
            *side = i16::from_le_bytes(self.word()?.to_le_bytes());
        }
        Some(place)
    }

    /// UTF-16 to its NUL, a unit no text can hold replaced.
    fn text(&mut self) -> Option<String> {
        let mut units = Vec::new();
        loop {
            match self.word()? {
                0 => return Some(String::from_utf16_lossy(&units)),
                unit => units.push(unit),
            }
        }
    }

    /// A name that may be none, a number or a text: the number comes back
    /// as `Err`, none as an empty text.
    fn name(&mut self) -> Option<Result<String, u16>> {
        let first = self.word()?;
        if first == 0xffff {
            return Some(Err(self.word()?));
        }
        self.at -= 2;
        self.text().map(Ok)
    }

    fn align(&mut self) {
        self.at = self.at.next_multiple_of(4);
    }
}

fn items(reader: &mut Reader<'_>, depth: u32) -> Option<Vec<Item>> {
    if depth > DEEPEST {
        return None;
    }
    let mut out = Vec::new();
    loop {
        let options = reader.word()?;
        let popup = options & MF_POPUP != 0;
        let id = if popup { 0 } else { reader.word()? };
        let words = reader.text()?;
        let inner = if popup {
            items(reader, depth + 1)?
        } else {
            Vec::new()
        };
        out.push(Item {
            id,
            state: u32::from(options & !(MF_POPUP | MF_END)),
            words,
            popup,
            items: inner,
        });
        if options & MF_END != 0 {
            return Some(out);
        }
    }
}

/// A menu template's items, each popup's nested; `None` for an extended
/// template or bytes that end early.
#[must_use]
pub fn menu(bytes: &[u8]) -> Option<Vec<Item>> {
    let mut reader = Reader { bytes, at: 0 };
    if reader.word()? != 0 {
        return None;
    }
    let offset = usize::from(reader.word()?);
    reader.at += offset;
    items(&mut reader, 0)
}

/// The name Windows gives a control's class ordinal; an unknown ordinal `#` and
/// its number would need an allocation no measured template asks, so it is empty.
fn class_name(ordinal: u16) -> String {
    let name = match ordinal {
        0x80 => "BUTTON",
        0x81 => "EDIT",
        0x82 => "STATIC",
        0x83 => "LISTBOX",
        0x84 => "SCROLLBAR",
        0x85 => "COMBOBOX",
        _ => "",
    };
    String::from(name)
}

fn control(reader: &mut Reader<'_>) -> Option<Control> {
    reader.align();
    let style = reader.long()?;
    let ex_style = reader.long()?;
    let place = reader.place()?;
    let id = reader.word()?;
    let class = reader.name()?.unwrap_or_else(class_name);
    let words = reader.name()?.unwrap_or_default();
    let extra = usize::from(reader.word()?);
    reader.at = reader.at.checked_add(extra)?;
    Some(Control {
        id,
        class,
        style,
        ex_style,
        place,
        words,
    })
}

/// A box's template, each control in template order; `None` for an extended
/// template or bytes that end early.
#[must_use]
pub fn dialog(bytes: &[u8]) -> Option<Dialog> {
    let mut reader = Reader { bytes, at: 0 };
    let style = reader.long()?;
    if style >> 16 == 0xffff {
        return None;
    }
    let ex_style = reader.long()?;
    let count = reader.word()?;
    let place = reader.place()?;
    let _ = reader.name()?;
    let _ = reader.name()?;
    let title = reader.text()?;
    if style & DS_SETFONT != 0 {
        reader.word()?;
        reader.text()?;
    }
    let controls = (0..count)
        .map(|_| control(&mut reader))
        .collect::<Option<Vec<_>>>()?;
    Some(Dialog {
        style,
        ex_style,
        place,
        title,
        controls,
    })
}

/// The item `item` names: by its place in `items` alone, or by id through
/// every popup; a popup itself is never found by id. `None` for one not here.
pub fn find_item(items: &mut [Item], item: u32, by_place: bool) -> Option<&mut Item> {
    if by_place {
        return items.get_mut(usize::try_from(item).ok()?);
    }
    for found in items {
        if !found.popup && u32::from(found.id) == item {
            return Some(found);
        }
        if let Some(inner) = find_item(&mut found.items, item, false) {
            return Some(inner);
        }
    }
    None
}
