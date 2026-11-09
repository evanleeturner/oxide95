// SPDX-License-Identifier: MIT
//! The disc: its source files in the cue sheet's order, where the data
//! track's sectors lie among them, and the volume's table.
use alloc::string::String;
use alloc::vec::Vec;
use kernel_files::{Error, names};

use crate::ReadAt;
use crate::cue::{self, Mode};
use crate::folder::Handles;
use crate::iso::{Entry, Place, Volume};
use crate::picked;
use crate::tracks::{self, Track};

/// A raw sector's bytes, data or audio (ECMA-130).
pub(crate) const RAW: u64 = 2352;
/// A Mode 1 sector's user bytes, and an iso's sector.
pub(crate) const USER: u64 = 2048;
/// Where a Mode 1 sector's user bytes start: past the sync and header.
const USER_AT: u64 = 16;

/// The player's files, as picked.
pub enum Source<R> {
    /// A cue sheet, and the files it may name, each by its name as picked;
    /// the sheet's `FILE` names are found among them by their last part,
    /// case folded.
    Image { cue: R, files: Vec<(String, R)> },
    /// An iso alone: the data track's user bytes, 2,048 a sector.
    Iso(R),
    /// A mounted disc's folder, picked: each file by its path from the
    /// disc's top, its parts joined by `/` or `\`. Its folders are the
    /// ones its paths pass through; it has no label and no audio track.
    Folder(Vec<(String, R)>),
}

/// Why a source would not open as a disc.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OpenError {
    /// A file the cue sheet names is not among the picked ones.
    Missing,
    /// The cue sheet's line, counted from 1, that its grammar refuses or
    /// that names a file type or track mode this reader does not take.
    Cue(usize),
    /// The sheet holds no `MODE1/2352` track.
    NoData,
    /// No ISO 9660 volume on the data track, or a record that runs past
    /// its folder or loops back to one already read.
    Volume,
    /// A source file would not read.
    Read(Error),
    /// A picked path with an empty, `.` or `..` part, or one that names,
    /// case folded, a file or folder another picked path already names.
    Path,
}

/// Where the data track's sectors lie: one source file, its first
/// sector's byte, a sector's stride, the user bytes' place in one, and the
/// sectors to the file's end.
struct Data {
    file: usize,
    first: u64,
    stride: u64,
    skip: u64,
    sectors: u64,
}

/// A disc, open: read only, and never read whole.
pub struct Disc<R> {
    /// The data track's sectors; a picked folder has none.
    data: Option<Data>,
    pub(crate) files: Vec<R>,
    pub(crate) tracks: Vec<Track>,
    volume: Volume,
    /// The open files and finds its `Folder` answers.
    pub(crate) handles: Handles,
}

impl<R: ReadAt> Disc<R> {
    /// The disc in `source`: the sheet read, its files found, the volume's
    /// folders read; no file's bytes.
    pub fn open(source: Source<R>) -> Result<Self, OpenError> {
        let (files, data, sheet) = match source {
            Source::Iso(iso) => {
                let sectors = iso.size() / USER;
                let data = Data {
                    file: 0,
                    first: 0,
                    stride: USER,
                    skip: 0,
                    sectors,
                };
                (alloc::vec![iso], data, Vec::new())
            }
            Source::Image { cue, files } => image(&cue, files)?,
            Source::Folder(picked) => return Self::of_folder(picked),
        };
        let tracks = tracks::of_sheet(&sheet, &files);
        let mut disc = Self {
            files,
            data: Some(data),
            tracks,
            volume: Volume::default(),
            handles: Handles::default(),
        };
        disc.volume = Volume::read(|sector, into| disc.user(sector, 0, into))?;
        Ok(disc)
    }

    /// A picked folder's disc: its table from the paths and each file's
    /// size; no file's bytes.
    fn of_folder(picked: Vec<(String, R)>) -> Result<Self, OpenError> {
        let volume = picked::volume(&picked)?;
        Ok(Self {
            data: None,
            files: picked.into_iter().map(|(_, file)| file).collect(),
            tracks: Vec::new(),
            volume,
            handles: Handles::default(),
        })
    }

    /// The volume identifier, its padding cut; a picked folder's is empty.
    #[must_use]
    pub fn label(&self) -> &[u8] {
        self.volume.label()
    }

    /// The file or folder `name` names, from the volume's top.
    pub(crate) fn entry(&self, name: &[u8]) -> Option<&Entry> {
        self.volume.entry(name)
    }

    /// The entries in the folder whose key is `folder`, in key order, each
    /// with its key.
    pub(crate) fn within<'a>(
        &'a self,
        folder: &'a [u8],
    ) -> impl Iterator<Item = (&'a [u8], &'a Entry)> {
        self.volume.within(folder)
    }

    /// `entry`'s bytes from `at` into `into`: on the data track one range a
    /// sector, from a picked file one range; the count moved, short at the
    /// file's end.
    pub(crate) fn read(&self, entry: &Entry, at: u64, into: &mut [u8]) -> Result<usize, Error> {
        let left = u64::from(entry.size).saturating_sub(at);
        let count = usize::try_from(left).map_or(into.len(), |left| left.min(into.len()));
        let extent = match entry.place {
            Place::Sector(extent) => extent,
            Place::File(file) => {
                if count > 0 {
                    self.files[file].read_at(at, &mut into[..count])?;
                }
                return Ok(count);
            }
            Place::Folder => return Err(Error::BadArgument),
        };
        let mut done = 0;
        while done < count {
            let here = at + done as u64;
            let sector = u32::try_from(here / USER)
                .ok()
                .and_then(|step| extent.checked_add(step))
                .ok_or(Error::BadArgument)?;
            let within = here % USER;
            let room = usize::try_from(USER - within).unwrap_or(usize::MAX);
            let take = (count - done).min(room);
            self.user(sector, within, &mut into[done..done + take])?;
            done += take;
        }
        Ok(done)
    }

    /// `into.len()` user bytes of the data track's sector `sector`, from
    /// its byte `at`: one range of one source file. A picked folder has no
    /// data track.
    fn user(&self, sector: u32, at: u64, into: &mut [u8]) -> Result<(), Error> {
        let data = self.data.as_ref().ok_or(Error::BadArgument)?;
        if u64::from(sector) >= data.sectors || at + into.len() as u64 > USER {
            return Err(Error::BadArgument);
        }
        let from = data.first + u64::from(sector) * data.stride + data.skip + at;
        self.files[data.file].read_at(from, into)
    }
}

/// An image's files in the sheet's order, its data track's place, and the
/// sheet. The data track is the sheet's first `MODE1/2352` track; its
/// sectors run to its file's end.
fn image<R: ReadAt>(
    cue: &R,
    picked: Vec<(String, R)>,
) -> Result<(Vec<R>, Data, Vec<cue::File>), OpenError> {
    let size = usize::try_from(cue.size()).map_err(|_| OpenError::Read(Error::BadArgument))?;
    let mut text = alloc::vec![0; size];
    cue.read_at(0, &mut text).map_err(OpenError::Read)?;
    let sheet = cue::parse(&text)?;
    let mut picked: Vec<Option<(String, R)>> = picked.into_iter().map(Some).collect();
    let mut files = Vec::with_capacity(sheet.len());
    for file in &sheet {
        let wanted = names::last(&file.name);
        let found = picked
            .iter_mut()
            .find(|each| {
                each.as_ref()
                    .is_some_and(|(name, _)| names::same(names::last(name.as_bytes()), wanted))
            })
            .and_then(Option::take)
            .ok_or(OpenError::Missing)?;
        files.push(found.1);
    }
    let (file, track) = sheet
        .iter()
        .enumerate()
        .find_map(|(at, file)| {
            let track = file.tracks.iter().find(|track| track.mode == Mode::Mode1)?;
            Some((at, track))
        })
        .ok_or(OpenError::NoData)?;
    let first = u64::from(track.start) * RAW;
    let sectors = files[file].size().saturating_sub(first) / RAW;
    let data = Data {
        file,
        first,
        stride: RAW,
        skip: USER_AT,
        sectors,
    };
    Ok((files, data, sheet))
}
