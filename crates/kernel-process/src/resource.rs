// SPDX-License-Identifier: MIT
//! The `.rsrc` read: one resource's bytes out of a PE file's bytes, portable,
//! with no allocator. A menu's or a box's template is read from the module on
//! the player's disc at run time, never stored.
//!
//! Call: [`find`] `(image, kind, id) -> Option<&[u8]>`. A file's bytes as they
//! lie on the disc, not as a loader maps them. PE32 alone, the one form
//! measured. Bad bytes answer `None`, never a panic.

/// `RT_MENU`.
pub const MENU: u32 = 4;
/// `RT_DIALOG`.
pub const DIALOG: u32 = 5;

/// The high bit of a directory entry's offset: another directory.
const SUBDIRECTORY: u32 = 0x8000_0000;

fn word(bytes: &[u8], at: usize) -> Option<u16> {
    let found = bytes.get(at..at.checked_add(2)?)?;
    Some(u16::from_le_bytes(found.try_into().ok()?))
}

fn long(bytes: &[u8], at: usize) -> Option<u32> {
    let found = bytes.get(at..at.checked_add(4)?)?;
    Some(u32::from_le_bytes(found.try_into().ok()?))
}

fn place(bytes: &[u8], at: usize) -> Option<usize> {
    usize::try_from(long(bytes, at)?).ok()
}

/// The resource directory's bytes to the section's end, and its address: the
/// addresses inside it are counted from the file's base, not from the section.
fn section(image: &[u8]) -> Option<(&[u8], u32)> {
    let pe = place(image, 0x3c)?;
    if image.get(pe..pe.checked_add(4)?)? != b"PE\0\0" {
        return None;
    }
    let optional = pe.checked_add(24)?;
    if word(image, optional)? != 0x10b {
        return None;
    }
    let address = long(image, optional.checked_add(112)?)?;
    let table = optional.checked_add(usize::from(word(image, pe.checked_add(20)?)?))?;
    (0..usize::from(word(image, pe.checked_add(6)?)?)).find_map(|index| {
        let row = table.checked_add(index.checked_mul(40)?)?;
        let start = long(image, row.checked_add(12)?)?;
        let size = long(image, row.checked_add(16)?)?;
        let raw = place(image, row.checked_add(20)?)?;
        let inside = address.checked_sub(start).filter(|inside| *inside < size)?;
        let from = raw.checked_add(usize::try_from(inside).ok()?)?;
        let to = raw.checked_add(usize::try_from(size).ok()?)?;
        Some((image.get(from..to)?, address))
    })
}

/// The offset the entry named `id` holds in the directory at `at`; `id` `None`
/// takes the directory's first entry, as a language's level is read here.
fn entry(directory: &[u8], at: usize, id: Option<u32>) -> Option<u32> {
    let named = usize::from(word(directory, at.checked_add(12)?)?);
    let numbered = usize::from(word(directory, at.checked_add(14)?)?);
    let row = |index: usize| at.checked_add(16)?.checked_add(index.checked_mul(8)?);
    let Some(id) = id else {
        return long(directory, row(0)?.checked_add(4)?);
    };
    (named..named + numbered).find_map(|index| {
        let row = row(index)?;
        (long(directory, row)? == id).then(|| long(directory, row.checked_add(4)?))?
    })
}

/// The directory an entry's offset points at; `None` for a leaf.
fn inner(offset: u32) -> Option<usize> {
    if offset & SUBDIRECTORY == 0 {
        return None;
    }
    usize::try_from(offset & !SUBDIRECTORY).ok()
}

/// The bytes of resource `kind`, `id` in the PE file `image`, in the first
/// language its directory lists; `None` for bytes that are no PE32 file or a
/// resource it lacks.
#[must_use]
pub fn find(image: &[u8], kind: u32, id: u32) -> Option<&[u8]> {
    let (directory, address) = section(image)?;
    let ids = inner(entry(directory, 0, Some(kind))?)?;
    let languages = inner(entry(directory, ids, Some(id))?)?;
    let leaf = entry(directory, languages, None)?;
    if leaf & SUBDIRECTORY != 0 {
        return None;
    }
    let leaf = usize::try_from(leaf).ok()?;
    let from = long(directory, leaf)?.checked_sub(address)?;
    let from = usize::try_from(from).ok()?;
    let size = place(directory, leaf.checked_add(4)?)?;
    directory.get(from..from.checked_add(size)?)
}

#[cfg(test)]
mod tests {
    use super::{DIALOG, MENU, find};
    use crate::plant::image;

    #[test]
    fn a_planted_resource_is_found_by_its_kind_and_its_id() {
        let file = image(&[
            (MENU, 7, b"seven"),
            (DIALOG, 9, b"nine"),
            (MENU, 8, b"eight"),
        ]);
        assert_eq!(find(&file, MENU, 7), Some(&b"seven"[..]));
        assert_eq!(find(&file, MENU, 8), Some(&b"eight"[..]));
        assert_eq!(find(&file, DIALOG, 9), Some(&b"nine"[..]));
    }

    #[test]
    fn a_kind_or_an_id_the_file_lacks_answers_none() {
        let file = image(&[(MENU, 7, b"seven")]);
        assert_eq!(find(&file, DIALOG, 7), None);
        assert_eq!(find(&file, MENU, 8), None);
    }

    #[test]
    fn bad_bytes_answer_none_and_never_panic() {
        assert_eq!(find(b"", MENU, 7), None);
        assert_eq!(find(&[0xff; 0x400], MENU, 7), None);
        let file = image(&[(MENU, 7, b"seven")]);
        for end in 0..file.len() {
            assert_ne!(find(&file[..end], MENU, 7), Some(&b"wrong"[..]));
        }
        let mut bent = file.clone();
        bent[0x200 + 20] ^= 0xff;
        assert_eq!(find(&bent, MENU, 7), None);
    }
}
