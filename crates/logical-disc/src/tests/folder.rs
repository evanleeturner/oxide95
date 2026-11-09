// SPDX-License-Identifier: MIT
//! The disc as a `Folder`, on the image and the iso alike: its answers;
//! nothing on it changes; a file's read takes its own sectors and no more;
//! every call one `disc.call` line.
use alloc::rc::Rc;
use alloc::string::{String, ToString};
use alloc::vec;
use alloc::vec::Vec;
use core::cell::RefCell;
use core::ffi::CStr;

use kernel_files::{Create, Error, Folder, Kind, Mode, Origin};

use super::test_log::debug_lines;
use super::{FILES, SHEET, fill, folder_source, image_source, iso};
use crate::disc::{RAW, USER};
use crate::{Disc, ReadAt, Source};

/// An open to read a file that is there.
const READ: Mode = Mode {
    read: true,
    write: false,
    create: Create::Existing,
};

/// The three discs the tree makes: the image, the iso alone, the picked
/// folder.
fn discs() -> [Disc<Vec<u8>>; 3] {
    [
        Disc::open(image_source(SHEET)).expect("the image opens"),
        Disc::open(Source::Iso(iso())).expect("the iso opens"),
        Disc::open(folder_source()).expect("the folder opens"),
    ]
}

/// A find's answer, to its NUL.
fn named(name: &[u8]) -> &[u8] {
    CStr::from_bytes_until_nul(name).expect("a NUL").to_bytes()
}

/// The ranges each source file was asked for: its name, the byte, the
/// count.
type Asked = Rc<RefCell<Vec<(String, u64, usize)>>>;

/// A source file that keeps each range asked of it.
struct Counting {
    name: String,
    bytes: Vec<u8>,
    asked: Asked,
}

impl ReadAt for Counting {
    fn size(&self) -> u64 {
        self.bytes.size()
    }

    fn read_at(&self, at: u64, into: &mut [u8]) -> Result<(), Error> {
        let range = (self.name.clone(), at, into.len());
        self.asked.borrow_mut().push(range);
        self.bytes.read_at(at, into)
    }
}

/// `source`'s files, each counting into `asked`: the sheet as `cue`, an
/// iso as `iso`, a picked file by its name.
fn counted(source: Source<Vec<u8>>, asked: &Asked) -> Source<Counting> {
    let count = |name: &str, bytes| Counting {
        name: name.to_string(),
        bytes,
        asked: Rc::clone(asked),
    };
    match source {
        Source::Iso(bytes) => Source::Iso(count("iso", bytes)),
        Source::Image { cue, files } => Source::Image {
            cue: count("cue", cue),
            files: files
                .into_iter()
                .map(|(name, bytes)| (name.clone(), count(&name, bytes)))
                .collect(),
        },
        Source::Folder(files) => Source::Folder(
            files
                .into_iter()
                .map(|(name, bytes)| (name.clone(), count(&name, bytes)))
                .collect(),
        ),
    }
}

/// The disc's answers as a folder: a file opened by any spelling, read,
/// sought and closed; the refusals by name; handles and finds from 4 by
/// 4; finds over files and folders in key order.
#[test]
fn the_disc_answers_as_a_folder() {
    for disc in discs() {
        let readme = disc.open(c"readme.txt", READ).expect("folded");
        assert_eq!(readme, 4);
        assert_eq!(disc.kind(readme), Ok(Kind::Disk));
        let mut got = [0; 60];
        assert_eq!(Folder::read(&disc, readme, &mut got), Ok(60));
        assert_eq!(got[..], fill(100, 1)[..60]);
        assert_eq!(Folder::read(&disc, readme, &mut got), Ok(40));
        assert_eq!(got[..40], fill(100, 1)[60..]);
        assert_eq!(Folder::read(&disc, readme, &mut got), Ok(0));
        assert_eq!(disc.seek(readme, -10, Origin::End), Ok(90));
        assert_eq!(disc.seek(readme, 5, Origin::Here), Ok(95));
        assert_eq!(disc.seek(readme, -1, Origin::Start), Err(Error::BadSeek));
        assert_eq!(disc.seek(readme, 200, Origin::Start), Ok(200));
        assert_eq!(Folder::read(&disc, readme, &mut got), Ok(0));
        assert_eq!(disc.close(readme), Ok(()));
        assert_eq!(disc.close(readme), Err(Error::BadHandle));
        assert_eq!(disc.kind(readme), Err(Error::BadHandle));
        assert_eq!(Folder::read(&disc, readme, &mut got), Err(Error::BadHandle));

        assert_eq!(disc.open(c"NONE\\X.DAT", READ), Err(Error::NoPath));
        assert_eq!(disc.open(c"DATA", READ), Err(Error::Denied));
        assert_eq!(disc.open(c"DATA\\NONE.DAT", READ), Err(Error::NotFound));
        assert_eq!(disc.open(c"\\data\\level.dat", READ), Ok(8));

        let mut name = [0; 16];
        let find = disc.find_first(c"DATA\\*", &mut name).expect("DATA's");
        assert_eq!(find, 12);
        assert_eq!(named(&name), b"LEVEL.DAT");
        assert_eq!(disc.find_next(find, &mut name), Ok(()));
        assert_eq!(named(&name), b"SOUND.RAW");
        assert_eq!(disc.find_next(find, &mut name), Err(Error::NoMore));
        assert_eq!(disc.find_close(find), Ok(()));
        assert_eq!(disc.find_next(find, &mut name), Err(Error::BadHandle));
        assert_eq!(disc.find_close(find), Err(Error::BadHandle));
        let find = disc.find_first(c"*", &mut name).expect("the top's");
        assert_eq!(named(&name), b"DATA");
        assert_eq!(disc.find_next(find, &mut name), Ok(()));
        assert_eq!(named(&name), b"README.TXT");
        assert_eq!(disc.find_next(find, &mut name), Err(Error::NoMore));
        assert!(disc.find_first(c"data\\?OUND.*", &mut name).is_ok());
        assert_eq!(named(&name), b"SOUND.RAW");
        assert_eq!(
            disc.find_first(c"DATA\\*.TXT", &mut name),
            Err(Error::NotFound)
        );
        assert_eq!(disc.find_first(c"NONE\\*", &mut name), Err(Error::NoPath));
        let mut short = [0xff; 4];
        assert!(disc.find_first(c"README.*", &mut short).is_ok());
        assert_eq!(&short, b"REA\0");

        disc.fail(87);
        assert_eq!(disc.last_error(), 87);
    }
}

/// A write, a delete and a move answer access denied, and
/// so does any open that would write, make or empty a file; the file
/// reads equal after.
#[test]
fn nothing_on_the_disc_changes() {
    let mode = |read, write, create| Mode {
        read,
        write,
        create,
    };
    // A mode, its answer on a file there, its answer on one not there.
    let opens = [
        (
            mode(true, true, Create::Existing),
            Err(Error::Denied),
            Err(Error::NotFound),
        ),
        (
            mode(false, true, Create::Always),
            Err(Error::Denied),
            Err(Error::Denied),
        ),
        (
            mode(true, false, Create::New),
            Err(Error::Exists),
            Err(Error::Denied),
        ),
        (
            mode(false, true, Create::OpenAlways),
            Err(Error::Denied),
            Err(Error::Denied),
        ),
        (
            mode(true, false, Create::OpenAlways),
            Ok(()),
            Err(Error::Denied),
        ),
        (
            mode(false, true, Create::Truncate),
            Err(Error::Denied),
            Err(Error::NotFound),
        ),
        (
            mode(true, false, Create::Truncate),
            Err(Error::Denied),
            Err(Error::NotFound),
        ),
    ];
    for disc in discs() {
        let readme = disc.open(c"README.TXT", READ).expect("opens");
        assert_eq!(disc.write(readme, b"changed"), Err(Error::Denied));
        assert_eq!(disc.write(99, b"changed"), Err(Error::BadHandle));
        assert_eq!(disc.delete(c"README.TXT"), Err(Error::Denied));
        assert_eq!(disc.delete(c"NONE\\GONE.TXT"), Err(Error::Denied));
        assert_eq!(disc.rename(c"README.TXT", c"OTHER.TXT"), Err(Error::Denied));
        for (mode, there, gone) in opens {
            let answer = disc.open(c"README.TXT", mode).map(|_| ());
            assert_eq!(answer, there, "{mode:?}");
            let answer = disc.open(c"DATA\\NEW.DAT", mode).map(|_| ());
            assert_eq!(answer, gone, "{mode:?}");
        }
        let mut got = [0; 200];
        assert_eq!(Folder::read(&disc, readme, &mut got), Ok(100));
        assert_eq!(got[..100], fill(100, 1));
        for (name, _, length, _) in FILES {
            assert!(
                disc.entry(name.as_bytes())
                    .is_some_and(|entry| entry.size as usize == length)
            );
        }
    }
}

/// An open reads nothing; a file read whole takes its own
/// three sectors, one range each, and no other file's bytes; a read
/// across a sector's edge takes the two ranges it spans.
#[test]
fn a_read_takes_its_own_sectors() {
    let asked = Asked::default();
    let image = Disc::open(counted(image_source(SHEET), &asked)).expect("the image opens");
    let iso = Disc::open(counted(Source::Iso(iso()), &asked)).expect("the iso opens");
    let in_bin: fn(u64) -> u64 = |sector| sector * RAW + 16;
    let in_iso: fn(u64) -> u64 = |sector| sector * USER;
    let sound = fill(5000, 3);
    for (disc, file, at) in [(&image, "picked/SYNTH.BIN", in_bin), (&iso, "iso", in_iso)] {
        asked.borrow_mut().clear();
        let handle = disc.open(c"DATA\\SOUND.RAW", READ).expect("opens");
        assert!(asked.borrow().is_empty(), "an open reads nothing");
        let mut got = vec![0; 6000];
        assert_eq!(Folder::read(disc, handle, &mut got), Ok(5000));
        assert_eq!(got[..5000], sound);
        let range = |sector, from, count| (file.to_string(), at(sector) + from, count);
        let whole = [range(22, 0, 2048), range(23, 0, 2048), range(24, 0, 904)];
        assert_eq!(asked.borrow_mut().split_off(0), whole);

        assert_eq!(disc.seek(handle, 2000, Origin::Start), Ok(2000));
        let mut edge = [0; 100];
        assert_eq!(Folder::read(disc, handle, &mut edge), Ok(100));
        assert_eq!(edge[..], sound[2000..2100]);
        let spans = [range(22, 2000, 48), range(23, 0, 52)];
        assert_eq!(asked.borrow_mut().split_off(0), spans);
    }
}

/// A picked folder's open reads nothing; a read takes one range of the
/// file's own bytes and no other file's; a read at its end asks nothing.
#[test]
fn a_picked_read_takes_one_range() {
    let asked = Asked::default();
    let folder = Disc::open(counted(folder_source(), &asked)).expect("the folder opens");
    let handle = folder.open(c"data\\sound.raw", READ).expect("folded");
    assert!(asked.borrow().is_empty(), "an open reads nothing");
    let sound = fill(5000, 3);
    let mut got = vec![0; 6000];
    assert_eq!(Folder::read(&folder, handle, &mut got), Ok(5000));
    assert_eq!(got[..5000], sound);
    let file = "DATA/SOUND.RAW".to_string();
    assert_eq!(asked.borrow_mut().split_off(0), [(file.clone(), 0, 5000)]);

    assert_eq!(folder.seek(handle, 2000, Origin::Start), Ok(2000));
    let mut edge = [0; 100];
    assert_eq!(Folder::read(&folder, handle, &mut edge), Ok(100));
    assert_eq!(edge[..], sound[2000..2100]);
    assert_eq!(asked.borrow_mut().split_off(0), [(file, 2000, 100)]);
    assert_eq!(folder.seek(handle, 0, Origin::End), Ok(5000));
    assert_eq!(Folder::read(&folder, handle, &mut edge), Ok(0));
    assert!(asked.borrow().is_empty(), "a read at the end asks nothing");
}

/// Every call of the 11, answered or refused, writes one
/// `disc.call` line, its `c` the call; `fail`, the front's bookkeeping,
/// writes none.
#[test]
fn every_call_writes_one_line() {
    let disc = Disc::open(image_source(SHEET)).expect("the image opens");
    let mut name = [0; 16];
    let mut got = [0; 200];
    let lines = debug_lines(|| {
        let file = disc.open(c"README.TXT", READ).expect("opens");
        _ = Folder::read(&disc, file, &mut got);
        _ = disc.write(file, b"x");
        _ = disc.seek(file, 0, Origin::Start);
        _ = disc.kind(file);
        _ = disc.close(file);
        _ = disc.delete(c"README.TXT");
        _ = disc.rename(c"README.TXT", c"OTHER.TXT");
        let find = disc.find_first(c"*", &mut name).expect("the top's");
        _ = disc.find_next(find, &mut name);
        _ = disc.find_close(find);
        _ = disc.open(c"NONE\\X.DAT", READ);
        _ = Folder::read(&disc, 99, &mut got);
        _ = disc.find_next(99, &mut name);
        disc.fail(5);
    });
    let calls = [
        "open",
        "read",
        "write",
        "seek",
        "kind",
        "close",
        "delete",
        "rename",
        "find_first",
        "find_next",
        "find_close",
        "open",
        "read",
        "find_next",
    ];
    let called: Vec<&str> = lines
        .iter()
        .map(|line| {
            let rest = line.strip_prefix("disc.call c=").expect(line);
            rest.split(' ').next().expect(line)
        })
        .collect();
    assert_eq!(called, calls);
    assert_eq!(lines[1], "disc.call c=read a=4 p= v=64 n=64 e=ok");
    assert_eq!(
        lines[11],
        "disc.call c=open a=0 p=NONE\\X.DAT v=0 n=0 e=NoPath"
    );
}
