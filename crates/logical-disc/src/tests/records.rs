// SPDX-License-Identifier: MIT
//! Volume records made by hand where a tree writer makes none: a name
//! holding a byte past ASCII, and a file whose extent lies past the data
//! track, in the audio after it. Each check prints one line under
//! `--nocapture`.
//!
//! `DI1`: a disc's files read back equal to the tree they were made of.
use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec;
use alloc::vec::Vec;
use core::ffi::CStr;

use kernel_files::{Create, Error, Folder, Mode};

use super::{bin, fill, iso, record};
use crate::{Disc, ReadAt, Source};

const READ: Mode = Mode {
    read: true,
    write: false,
    create: Create::Existing,
};

/// The iso with the root's last record, `README.TXT`'s, renamed `name`
/// and moved to `extent`; its 100 bytes stay at sector 20.
fn iso_with(name: &[u8], extent: u32) -> Vec<u8> {
    let mut image = iso();
    let root = [
        record(&[0], 18, 2048, true),
        record(&[1], 18, 2048, true),
        record(b"DATA", 19, 2048, true),
        record(name, extent, 100, false),
    ]
    .concat();
    let at = 18 * 2048;
    image[at..at + 2048].fill(0);
    image[at..at + root.len()].copy_from_slice(&root);
    image
}

/// A file's bytes by its name, read whole, or the refusal.
fn read_named(disc: &impl Folder, name: &[u8]) -> Result<Vec<u8>, Error> {
    let mut text = name.to_vec();
    text.push(0);
    let name = CStr::from_bytes_with_nul(&text).expect("a name");
    let file = disc.open(name, READ)?;
    let mut bytes = vec![0; 4096];
    let count = disc.read(file, &mut bytes);
    disc.close(file)?;
    bytes.truncate(usize::try_from(count?).expect("a count"));
    Ok(bytes)
}

fn shown(read: &Result<Vec<u8>, Error>, source: &[u8]) -> String {
    match read {
        Ok(bytes) if bytes == source => format!("read={}:source", bytes.len()),
        Ok(bytes) => format!("read={}:other", bytes.len()),
        Err(error) => format!("refused={error:?}"),
    }
}

fn row(kind: &str, what: &str, got: &str, want: &str) -> bool {
    let same = got == want;
    let verdict = if same { "ok" } else { "DIFF" };
    println!("{kind} {what} {got} want {want} {verdict}");
    same
}

/// The names the root's search gives back, each in hex.
fn listed<R: ReadAt>(disc: &Disc<R>) -> Vec<String> {
    use core::fmt::Write as _;
    let hex = |name: &[u8]| -> String {
        let end = name
            .iter()
            .position(|&byte| byte == 0)
            .unwrap_or(name.len());
        name[..end].iter().fold(String::new(), |mut out, byte| {
            write!(out, "{byte:02x}").expect("a string takes a write");
            out
        })
    };
    let mut name = [0; 260];
    let mut names = Vec::new();
    let Ok(find) = disc.find_first(c"*", &mut name) else {
        return names;
    };
    names.push(hex(&name));
    while disc.find_next(find, &mut name).is_ok() {
        names.push(hex(&name));
    }
    disc.find_close(find).expect("the search closes");
    names
}

/// A name past ASCII opens by its own bytes, ASCII letters folded and no
/// other byte: the reader's declared rule. ECMA-119's file names hold
/// `A`-`Z`, `0`-`9` and `_` alone, so the volume is outside the standard
/// and the rule is the reader's own.
#[test]
fn a_name_past_ascii_folds_its_ascii_alone() {
    let disc = Disc::open(Source::Iso(iso_with(b"CAF\xc9.TXT;1", 20))).expect("the iso opens");
    let source = fill(100, 1);
    let mut same = true;
    for (what, name, want) in [
        ("past_ascii_exact", &b"CAF\xc9.TXT"[..], "read=100:source"),
        ("past_ascii_ascii_folded", b"caf\xc9.txt", "read=100:source"),
        (
            "past_ascii_byte_unfolded",
            b"CAF\xe9.TXT",
            "refused=NotFound",
        ),
    ] {
        same &= row("DI1", what, &shown(&read_named(&disc, name), &source), want);
    }
    // A search's order is no declared rule: the names are sorted first.
    let mut names = listed(&disc);
    names.sort();
    let wanted = "434146c92e545854,44415441";
    same &= row("DI1", "past_ascii_listed", &names.join(","), wanted);
    assert!(same, "a row differs");
}

/// One bin of data then audio, the audio's `INDEX 01` at 25, the data
/// track's last sector 24; `README.TXT` at sector `extent`.
fn one_bin(extent: u32) -> Source<Vec<u8>> {
    let mut bytes = bin(&iso_with(b"README.TXT;1", extent));
    bytes.extend(fill(10 * 2352, 0x40));
    let cue = "FILE \"ONE.BIN\" BINARY
  TRACK 01 MODE1/2352
    INDEX 01 00:00:00
  TRACK 02 AUDIO
    INDEX 01 00:00:25
";
    Source::Image {
        cue: cue.as_bytes().to_vec(),
        files: vec![("ONE.BIN".to_string(), bytes)],
    }
}

/// The one-bin image itself: a record inside the data track reads its
/// source.
#[test]
fn one_bin_reads_its_data_track() {
    let disc = Disc::open(one_bin(20)).expect("the image opens");
    let got = shown(&read_named(&disc, b"README.TXT"), &fill(100, 1));
    assert!(row("DI1", "one_bin_inside", &got, "read=100:source"));
}

/// A data track ends where the next track begins: a record whose extent
/// lies past it, in track 2's frames, names no data sector.
#[test]
#[ignore = "the data track runs to its file's end here, so a record in the audio after it reads audio bytes"]
fn a_record_past_the_data_track_is_refused() {
    let disc = Disc::open(one_bin(30)).expect("the image opens");
    let got = match read_named(&disc, b"README.TXT") {
        Ok(bytes) => format!("read={}", bytes.len()),
        Err(_) => "refused".to_string(),
    };
    assert!(row("DI1x", "record_in_the_audio", &got, "refused"));
}
