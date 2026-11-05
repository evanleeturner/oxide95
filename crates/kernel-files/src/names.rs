// SPDX-License-Identifier: MIT
//! Names as Windows matches them, one rule for every folder, the memory
//! folder's and the disc's: ASCII case folded, `/` read as `\`; a
//! find's `*` any run and `?` one byte. Byte by byte, so it needs no
//! allocator.

/// One byte of a name as names compare: `/` as `\`, ASCII lower case.
#[must_use]
pub const fn fold(byte: u8) -> u8 {
    if byte == b'/' {
        b'\\'
    } else {
        byte.to_ascii_lowercase()
    }
}

/// Whether two names are one name.
#[must_use]
pub fn same(one: &[u8], other: &[u8]) -> bool {
    one.len() == other.len() && one.iter().zip(other).all(|(&a, &b)| fold(a) == fold(b))
}

/// A name's folder: the part before its last `\` or `/`, empty at the top.
#[must_use]
pub fn parent(name: &[u8]) -> &[u8] {
    name.iter()
        .rposition(|&byte| byte == b'\\' || byte == b'/')
        .map_or(&[][..], |at| &name[..at])
}

/// A name's last part, after its last `\` or `/`.
#[must_use]
pub fn last(name: &[u8]) -> &[u8] {
    name.iter()
        .rposition(|&byte| byte == b'\\' || byte == b'/')
        .map_or(name, |at| &name[at + 1..])
}

/// Whether `name` fits `pattern`: `*` any run, `?` one byte, the rest
/// folded.
#[must_use]
pub fn matches(pattern: &[u8], name: &[u8]) -> bool {
    match pattern.split_first() {
        None => name.is_empty(),
        Some((b'*', rest)) => (0..=name.len()).any(|skip| matches(rest, &name[skip..])),
        Some((b'?', rest)) => name
            .split_first()
            .is_some_and(|(_, tail)| matches(rest, tail)),
        Some((&byte, rest)) => name
            .split_first()
            .is_some_and(|(&first, tail)| fold(first) == fold(byte) && matches(rest, tail)),
    }
}
