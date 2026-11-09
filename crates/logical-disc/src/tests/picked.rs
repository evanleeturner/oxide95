// SPDX-License-Identifier: MIT
//! A mounted disc's folder, picked: its table from the paths alone, its
//! folders the ones they pass through; the paths it refuses.
use alloc::string::ToString;
use alloc::vec;
use alloc::vec::Vec;

use super::{fill, folder_source};
use crate::iso::Entry;
use crate::{Disc, OpenError, Source};

/// A picked folder of `paths`, each file 10 bytes.
fn picked(paths: &[&str]) -> Source<Vec<u8>> {
    Source::Folder(
        paths
            .iter()
            .map(|path| (path.to_string(), fill(10, 1)))
            .collect(),
    )
}

/// The folder has no label and no audio track; its folders are the ones
/// its paths pass through, by `/` or `\`, each named as the first path
/// through it spells it; an empty pick opens empty.
#[test]
fn the_folder_is_its_paths() {
    let disc = Disc::open(folder_source()).expect("the folder opens");
    assert_eq!(disc.label(), b"");
    assert!(disc.tracks.is_empty());
    assert!(disc.entry(b"data").is_some_and(Entry::is_folder));
    let names: Vec<&[u8]> = disc
        .within(b"data")
        .map(|(_, entry)| entry.name.as_slice())
        .collect();
    assert_eq!(names, [&b"LEVEL.DAT"[..], &b"SOUND.RAW"[..]]);

    let deep = Disc::open(picked(&["Top/Mid\\Low/End.Txt", "TOP/mid/Two.txt"])).expect("opens");
    for folder in [&b"TOP"[..], b"top\\MID", b"Top\\Mid\\Low"] {
        assert!(deep.entry(folder).is_some_and(Entry::is_folder));
    }
    let spelled = |name: &[u8]| deep.entry(name).map(|entry| entry.name.clone());
    assert_eq!(spelled(b"top\\mid"), Some(b"Mid".to_vec()));
    assert_eq!(spelled(b"TOP\\MID\\TWO.TXT"), Some(b"Two.txt".to_vec()));
    let within: Vec<&[u8]> = deep
        .within(b"top\\mid")
        .map(|(_, entry)| entry.name.as_slice())
        .collect();
    assert_eq!(within, [&b"Low"[..], &b"Two.txt"[..]]);

    let empty = Disc::open(picked(&[])).expect("an empty pick opens");
    assert!(empty.within(b"").next().is_none());
}

/// A path with an empty, `.` or `..` part, two that name one file case
/// folded, and a file that another path passes through, in either order,
/// each refuse the folder `Path`; two files in one folder spelled two ways
/// do not.
#[test]
fn a_clash_or_an_empty_part_refuses() {
    let refused = [
        vec!["/TOP.TXT"],
        vec!["A//B.TXT"],
        vec!["A/"],
        vec![""],
        vec!["A/./B.TXT"],
        vec!["A/../B.TXT"],
        vec!["A.TXT", "a.txt"],
        vec!["A", "A/B.TXT"],
        vec!["A/B.TXT", "a"],
    ];
    for paths in refused {
        let answer = Disc::open(picked(&paths)).err();
        assert_eq!(answer, Some(OpenError::Path), "{paths:?}");
    }
    assert!(Disc::open(picked(&["A/B.TXT", "a\\C.TXT"])).is_ok());
}
