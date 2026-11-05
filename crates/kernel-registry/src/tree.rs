// SPDX-License-Identifier: MIT
//! The settings tree both portable backs keep: each key by its path under
//! the root with its values, the handles held open, and the text form both
//! save. A key is there when its path or a key under it is held, as a
//! key's parents are on Windows; paths and names compare without ASCII
//! case, as Windows' do.
//!
//! The text form, one line a key or value: a key with no values its path
//! alone; a value the key's path, a tab, its name, a tab, `dword:` and 8
//! hex digits. A path or name with a tab or a line break cannot be held.
use crate::table::Row;
use alloc::vec::Vec;

/// The first handle a back gives, each next one 4 past it: never 0, and
/// below the predefined roots.
const FIRST_HANDLE: u32 = 0x100;
/// The handles end before the roots begin.
const PAST_HANDLES: u32 = 0x8000_0000;
const DWORD_TAG: &[u8] = b"dword:";
const HEX: &[u8; 16] = b"0123456789abcdef";

struct Key {
    path: Vec<u8>,
    values: Vec<(Vec<u8>, u32)>,
}

pub(crate) struct Tree {
    keys: Vec<Key>,
    open: Vec<(u32, Vec<u8>)>,
    next: u32,
}

/// Whether the text form can hold `bytes`.
fn holdable(bytes: &[u8]) -> bool {
    !bytes
        .iter()
        .any(|byte| matches!(byte, b'\t' | b'\n' | b'\r'))
}

/// `value` as 8 lower-case hex digits.
fn hex(value: u32) -> [u8; 8] {
    let mut digits = [0; 8];
    for (at, byte) in value.to_be_bytes().into_iter().enumerate() {
        digits[2 * at] = HEX[usize::from(byte >> 4)];
        digits[2 * at + 1] = HEX[usize::from(byte & 0xf)];
    }
    digits
}

/// The value a text form's `dword:` field holds.
fn dword(field: &[u8]) -> Option<u32> {
    let digits = field.strip_prefix(DWORD_TAG)?;
    if digits.len() != 8 || !digits.iter().all(u8::is_ascii_hexdigit) {
        return None;
    }
    u32::from_str_radix(core::str::from_utf8(digits).ok()?, 16).ok()
}

/// `path` under `base`, a backslash between.
fn joined(base: &[u8], path: &[u8]) -> Vec<u8> {
    match (base.is_empty(), path.is_empty()) {
        (true, _) => path.to_vec(),
        (_, true) => base.to_vec(),
        _ => [base, b"\\", path].concat(),
    }
}

/// The key at `path`, added with no values when not held.
fn key_mut<'a>(keys: &'a mut Vec<Key>, path: &[u8]) -> &'a mut Key {
    let at = if let Some(at) = keys
        .iter()
        .position(|key| key.path.eq_ignore_ascii_case(path))
    {
        at
    } else {
        keys.push(Key {
            path: path.to_vec(),
            values: Vec::new(),
        });
        keys.len() - 1
    };
    &mut keys[at]
}

/// Keeps `value` as `name` under `key`, replacing a value of that name.
fn set(key: &mut Key, name: &[u8], value: u32) {
    match key
        .values
        .iter_mut()
        .find(|(each, _)| each.eq_ignore_ascii_case(name))
    {
        Some((_, kept)) => *kept = value,
        None => key.values.push((name.to_vec(), value)),
    }
}

/// The keys the text form `saved` holds, `None` for a line not in it.
fn parse(saved: &[u8]) -> Option<Vec<Key>> {
    let mut keys = Vec::new();
    for line in saved.split(|byte| *byte == b'\n') {
        let line = line.strip_suffix(b"\r").unwrap_or(line);
        if line.is_empty() {
            continue;
        }
        let mut fields = line.split(|byte| *byte == b'\t');
        let key = key_mut(&mut keys, fields.next()?);
        match (fields.next(), fields.next(), fields.next()) {
            (None, _, _) => {}
            (Some(name), Some(value), None) => set(key, name, dword(value)?),
            _ => return None,
        }
    }
    Some(keys)
}

impl Tree {
    /// The tree the text form `saved` holds; for an empty one, the row's
    /// key with its first values. `None` for a text not in the form.
    pub(crate) fn new(row: &Row, saved: &[u8]) -> Option<Self> {
        let keys = if saved.is_empty() {
            alloc::vec![Key {
                path: row.key.to_vec(),
                values: row
                    .first
                    .iter()
                    .map(|(name, value)| (name.to_vec(), *value))
                    .collect(),
            }]
        } else {
            parse(saved)?
        };
        Some(Self {
            keys,
            open: Vec::new(),
            next: FIRST_HANDLE,
        })
    }

    /// The tree in the text form.
    pub(crate) fn text(&self) -> Vec<u8> {
        let mut text = Vec::new();
        for key in &self.keys {
            if key.values.is_empty() {
                text.extend_from_slice(&key.path);
                text.push(b'\n');
            }
            for (name, value) in &key.values {
                for field in [
                    &key.path[..],
                    b"\t",
                    name,
                    b"\t",
                    DWORD_TAG,
                    &hex(*value),
                    b"\n",
                ] {
                    text.extend_from_slice(field);
                }
            }
        }
        text
    }

    /// The path of the key `handle` holds.
    fn path(&self, handle: u32) -> Option<&[u8]> {
        self.open
            .iter()
            .find(|(each, _)| *each == handle)
            .map(|(_, path)| path.as_slice())
    }

    /// Whether a key is at `path`: the root, a key held, or a key's parent.
    fn there(&self, path: &[u8]) -> bool {
        path.is_empty()
            || self.keys.iter().any(|key| {
                key.path.eq_ignore_ascii_case(path)
                    || (key.path.len() > path.len()
                        && key.path[path.len()] == b'\\'
                        && key.path[..path.len()].eq_ignore_ascii_case(path))
            })
    }

    /// [`crate::Settings::open`]'s answer.
    pub(crate) fn open(
        &mut self,
        within: Option<u32>,
        path: &[u8],
        create: bool,
    ) -> Option<(u32, bool)> {
        let full = match within {
            None => path.to_vec(),
            Some(handle) => joined(self.path(handle)?, path),
        };
        let there = self.there(&full);
        if self.next >= PAST_HANDLES || !(there || create && holdable(&full)) {
            return None;
        }
        if !there {
            self.keys.push(Key {
                path: full.clone(),
                values: Vec::new(),
            });
        }
        let handle = self.next;
        self.next += 4;
        self.open.push((handle, full));
        Some((handle, !there))
    }

    pub(crate) fn holds(&self, handle: u32) -> bool {
        self.path(handle).is_some()
    }

    pub(crate) fn read(&self, handle: u32, name: &[u8]) -> Option<u32> {
        let path = self.path(handle)?;
        let key = self
            .keys
            .iter()
            .find(|key| key.path.eq_ignore_ascii_case(path))?;
        key.values
            .iter()
            .find(|(each, _)| each.eq_ignore_ascii_case(name))
            .map(|(_, value)| *value)
    }

    /// Keeps `value` as `name` under the key `handle`; `false` for a name
    /// the text form cannot hold or a handle not held.
    pub(crate) fn write(&mut self, handle: u32, name: &[u8], value: u32) -> bool {
        let Some(path) = self.path(handle).map(<[u8]>::to_vec) else {
            return false;
        };
        if !holdable(name) {
            return false;
        }
        set(key_mut(&mut self.keys, &path), name, value);
        true
    }

    pub(crate) fn close(&mut self, handle: u32) -> bool {
        let Some(at) = self.open.iter().position(|(each, _)| *each == handle) else {
            return false;
        };
        self.open.remove(at);
        true
    }
}
