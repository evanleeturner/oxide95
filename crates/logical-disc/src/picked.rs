// SPDX-License-Identifier: MIT
//! A picked folder's table: a mounted disc's files by their paths from the
//! disc's top, and every folder those paths pass through, keyed as the
//! volume's table keys its own. Nothing is read but each file's size.
use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use kernel_files::Error;

use crate::ReadAt;
use crate::disc::OpenError;
use crate::iso::{Entry, Place, Volume, join};

/// The table of `picked`, each file by its path, its parts joined by `/`
/// or `\`, each entry named as the first path through it spells it. A path
/// with an empty, `.` or `..` part, or one whose key another path's file or
/// folder already holds, refuses the folder `Path`; a file of 4 GiB or
/// more, `Read`.
pub(crate) fn volume<R: ReadAt>(picked: &[(String, R)]) -> Result<Volume, OpenError> {
    let mut entries = BTreeMap::new();
    for (file, (path, source)) in picked.iter().enumerate() {
        let parts: Vec<&[u8]> = path
            .as_bytes()
            .split(|&byte| byte == b'/' || byte == b'\\')
            .collect();
        if parts
            .iter()
            .any(|part| part.is_empty() || *part == b"." || *part == b"..")
        {
            return Err(OpenError::Path);
        }
        let (name, folders) = parts.split_last().ok_or(OpenError::Path)?;
        let mut key = Vec::new();
        for folder in folders {
            join(&mut key, folder);
            let entry = entries.entry(key.clone()).or_insert_with(|| Entry {
                name: folder.to_vec(),
                place: Place::Folder,
                size: 0,
            });
            if !entry.is_folder() {
                return Err(OpenError::Path);
            }
        }
        join(&mut key, name);
        let size = u32::try_from(source.size()).map_err(|_| OpenError::Read(Error::BadArgument))?;
        let entry = Entry {
            name: name.to_vec(),
            place: Place::File(file),
            size,
        };
        if entries.insert(key, entry).is_some() {
            return Err(OpenError::Path);
        }
    }
    Ok(Volume::of_entries(entries))
}
