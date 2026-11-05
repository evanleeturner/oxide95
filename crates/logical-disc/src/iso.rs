// SPDX-License-Identifier: MIT
//! The volume's table, read from ISO 9660 as ECMA-119 lays it out, the
//! fields used alone. The primary volume descriptor at sector 16: type 1,
//! `CD001`, the volume identifier at bytes 40 to 72, the block size at
//! 128, the root folder's record at 156. A folder's records: each its
//! length (byte 0; a 0 there makes the sector's rest padding), its extent
//! (2) and size (10) as little-endian halves, its flags (25, bit 1 a
//! folder) and its name (its length at 32, itself from 33; `\0` the folder
//! itself, `\1` its parent, the `;1` version dropped). The folders are
//! read at open; the files' bytes never.
use alloc::collections::{BTreeMap, BTreeSet};
use alloc::vec::Vec;
use kernel_files::{Error, names};

use crate::disc::OpenError;

/// The primary volume descriptor's sector.
const DESCRIPTOR: u32 = 16;
/// The one block size this reader takes, a sector's user bytes.
const BLOCK: u16 = 2048;

/// A file or folder on the volume: its name as the volume spells it, its
/// first sector, its bytes.
pub(crate) struct Entry {
    pub(crate) name: Vec<u8>,
    pub(crate) extent: u32,
    pub(crate) size: u32,
    pub(crate) folder: bool,
}

/// The volume's label, and every entry by its path's key: each byte
/// folded as names compare, the folders joined by `\`.
#[derive(Default)]
pub(crate) struct Volume {
    label: Vec<u8>,
    entries: BTreeMap<Vec<u8>, Entry>,
}

impl Volume {
    /// The volume on the sectors `sector` reads, its folders walked from
    /// the root.
    pub(crate) fn read(
        sector: impl Fn(u32, &mut [u8]) -> Result<(), Error>,
    ) -> Result<Self, OpenError> {
        let mut block = [0; BLOCK as usize];
        sector(DESCRIPTOR, &mut block).map_err(|_| OpenError::Volume)?;
        let size = u16::from_le_bytes([block[128], block[129]]);
        if block[0] != 1 || &block[1..6] != b"CD001" || size != BLOCK {
            return Err(OpenError::Volume);
        }
        let label = block[40..72].trim_ascii_end().to_vec();
        let root = record(&block[156..190]).ok_or(OpenError::Volume)?;
        let mut entries = BTreeMap::new();
        let mut read = BTreeSet::new();
        let mut folders = alloc::vec![(Vec::new(), root.extent, root.size)];
        while let Some((path, extent, size)) = folders.pop() {
            if !read.insert(extent) {
                return Err(OpenError::Volume);
            }
            for step in 0..size.div_ceil(u32::from(BLOCK)) {
                let at = extent.checked_add(step).ok_or(OpenError::Volume)?;
                sector(at, &mut block).map_err(OpenError::Read)?;
                let mut at = 0;
                while let Some(&length) = block.get(at).filter(|&&length| length != 0) {
                    let length = usize::from(length);
                    let entry = block
                        .get(at..at + length)
                        .and_then(record)
                        .ok_or(OpenError::Volume)?;
                    at += length;
                    if matches!(entry.name.as_slice(), [0 | 1]) {
                        continue;
                    }
                    let mut key = path.clone();
                    if !key.is_empty() {
                        key.push(b'\\');
                    }
                    key.extend(entry.name.iter().map(|&byte| names::fold(byte)));
                    if entry.folder {
                        folders.push((key.clone(), entry.extent, entry.size));
                    }
                    entries.insert(key, entry);
                }
            }
        }
        Ok(Self { label, entries })
    }

    pub(crate) fn label(&self) -> &[u8] {
        &self.label
    }

    /// The entry `name` names, from the top.
    pub(crate) fn entry(&self, name: &[u8]) -> Option<&Entry> {
        self.entries.get(&key(name))
    }

    /// The entries in the folder whose key is `folder`, in key order, each
    /// with its key.
    pub(crate) fn within<'a>(
        &'a self,
        folder: &'a [u8],
    ) -> impl Iterator<Item = (&'a [u8], &'a Entry)> {
        self.entries
            .iter()
            .map(|(key, entry)| (key.as_slice(), entry))
            .filter(move |(key, _)| names::parent(key) == folder)
    }
}

/// A name's key, as the table keeps it: from the top, a leading `\` or `/`
/// read as the top, each byte folded.
pub(crate) fn key(name: &[u8]) -> Vec<u8> {
    let name = name
        .strip_prefix(b"\\")
        .or_else(|| name.strip_prefix(b"/"))
        .unwrap_or(name);
    name.iter().map(|&byte| names::fold(byte)).collect()
}

/// A record's entry, or `None` when its fields run past its length.
fn record(bytes: &[u8]) -> Option<Entry> {
    let length = usize::from(*bytes.first()?);
    let name_length = usize::from(*bytes.get(32)?);
    if 33 + name_length > length {
        return None;
    }
    let name = bytes.get(33..33 + name_length)?;
    let extent = u32::from_le_bytes(bytes.get(2..6)?.try_into().ok()?);
    let size = u32::from_le_bytes(bytes.get(10..14)?.try_into().ok()?);
    let folder = bytes.get(25)? & 2 != 0;
    let name = name.split(|&byte| byte == b';').next().unwrap_or(name);
    let name = name.strip_suffix(b".").unwrap_or(name);
    Some(Entry {
        name: name.to_vec(),
        extent,
        size,
        folder,
    })
}
