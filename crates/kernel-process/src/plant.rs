// SPDX-License-Identifier: MIT
//! A planted file for tests: the least PE32 file that holds the resources it
//! is given, built in code, so no test ever needs a player's disc. It holds no
//! code and no loader could run it.
//!
//! Call: [`image`] `(resources) -> Vec<u8>`, each resource its kind, its id and
//! its bytes, one language each.

use alloc::vec::Vec;

/// Where the section lies in the file, and its address.
const RAW: u32 = 0x200;
const ADDRESS: u32 = 0x1000;
/// Where the headers sit: the PE header, then the optional header.
const PE: usize = 0x40;
const OPTIONAL: usize = PE + 24;
const OPTIONAL_SIZE: u16 = 224;
const SECTION: usize = OPTIONAL + OPTIONAL_SIZE as usize;
/// The high bit of a directory entry's offset: another directory.
const SUBDIRECTORY: u32 = 0x8000_0000;

fn count(number: usize) -> u32 {
    u32::try_from(number).expect("invariant: a planted file is small")
}

fn put(into: &mut [u8], at: usize, value: u32) {
    into[at..at + 4].copy_from_slice(&value.to_le_bytes());
}

fn size(entries: usize) -> u32 {
    16 + count(entries) * 8
}

fn directory(into: &mut Vec<u8>, entries: &[(u32, u32)]) {
    into.extend_from_slice(&[0; 14]);
    let numbered = u16::try_from(entries.len()).expect("invariant: a planted file is small");
    into.extend_from_slice(&numbered.to_le_bytes());
    for (name, offset) in entries {
        into.extend_from_slice(&name.to_le_bytes());
        into.extend_from_slice(&offset.to_le_bytes());
    }
}

/// The section's bytes: the root, each kind's directory, one language
/// directory a resource, one data entry a resource, then the bytes.
fn section(resources: &[(u32, u32, &[u8])]) -> Vec<u8> {
    let mut kinds: Vec<u32> = resources.iter().map(|row| row.0).collect();
    kinds.dedup();
    let rows = |kind: u32| {
        resources
            .iter()
            .enumerate()
            .filter(move |row| row.1.0 == kind)
    };

    let mut root = Vec::new();
    let mut at = size(kinds.len());
    for kind in &kinds {
        root.push((*kind, SUBDIRECTORY | at));
        at += size(rows(*kind).count());
    }
    let languages = at;
    let entries = languages + count(resources.len()) * size(1);
    let bytes = entries + count(resources.len()) * 16;

    let mut out = Vec::new();
    directory(&mut out, &root);
    for kind in &kinds {
        let ids: Vec<(u32, u32)> = rows(*kind)
            .map(|(index, row)| (row.1, SUBDIRECTORY | (languages + count(index) * size(1))))
            .collect();
        directory(&mut out, &ids);
    }
    for index in 0..resources.len() {
        directory(&mut out, &[(0x409, entries + count(index) * 16)]);
    }
    let mut blob = bytes;
    for row in resources {
        for value in [ADDRESS + blob, count(row.2.len()), 0, 0] {
            out.extend_from_slice(&value.to_le_bytes());
        }
        blob += count(row.2.len());
    }
    for row in resources {
        out.extend_from_slice(row.2);
    }
    out
}

/// The file's bytes, ready for [`crate::resource::find`].
#[must_use]
pub fn image(resources: &[(u32, u32, &[u8])]) -> Vec<u8> {
    let mut sorted = resources.to_vec();
    sorted.sort_by_key(|row| row.0);
    let body = section(&sorted);
    let mut out = alloc::vec![0; RAW as usize];
    out[..2].copy_from_slice(b"MZ");
    put(&mut out, 0x3c, count(PE));
    out[PE..PE + 4].copy_from_slice(b"PE\0\0");
    out[PE + 6..PE + 8].copy_from_slice(&1u16.to_le_bytes());
    out[PE + 20..PE + 22].copy_from_slice(&OPTIONAL_SIZE.to_le_bytes());
    out[OPTIONAL..OPTIONAL + 2].copy_from_slice(&0x10bu16.to_le_bytes());
    put(&mut out, OPTIONAL + 112, ADDRESS);
    put(&mut out, OPTIONAL + 116, count(body.len()));
    out[SECTION..SECTION + 5].copy_from_slice(b".rsrc");
    put(&mut out, SECTION + 12, ADDRESS);
    put(&mut out, SECTION + 16, count(body.len()));
    put(&mut out, SECTION + 20, RAW);
    out.extend_from_slice(&body);
    out
}
