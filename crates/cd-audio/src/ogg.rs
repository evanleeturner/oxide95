// SPDX-License-Identifier: MIT
//! A store's tracks through the door: each an Ogg Vorbis file in a [`Folder`], decoded by
//! `lewton`. A file not of a disc's shape, 44,100 Hz and 2 channels, is a bad one, as is one
//! that will not decode: it reads as `None`, and the door gives it one line and silence.
//!
//! A read that follows the last goes on decoding. Any other, a resume above all, seeks. A
//! page is a seek's grain, and a decoder that has just sought cannot say where it stands
//! until a page ends, so `Decoding::seek_near` lands before the place and decodes up to
//! it. Close to the file's start, or where no seek lands well, the track is decoded from
//! its start, which holds up the caller for as long as that takes.
use alloc::ffi::CString;
use alloc::vec::Vec;
use core::ops::Deref;
use std::io::{self, Read, Seek, SeekFrom};

use kernel_files::{Create, Folder, Mode, Origin};
use lewton::inside_ogg::OggStreamReader;
use logical_disc::{Audio, Encoded, Track};

use crate::door::Samples;
use crate::table::Place;

const RATE_HZ: u32 = 44_100;
const CHANNELS: u8 = 2;
/// Pairs in one of a disc's frames: `RATE_HZ` over 75.
const PAIRS_A_FRAME: u64 = 588;

/// The most bytes one Ogg page can take (RFC 3533 section 6).
const PAGE_MOST: u64 = 27 + 255 + 255 * 255;
/// A page's granule when no packet ends on it (RFC 3533 section 6).
const NO_GRANULE: u64 = u64::MAX;
/// How much further back each failed seek looks.
const WIDER: u64 = 8;

/// One open file of a folder, as `lewton` asks to read it. It closes when dropped.
struct File<D: Deref<Target: Folder>> {
    folder: D,
    file: u32,
}

impl<D: Deref<Target: Folder>> File<D> {
    fn open(folder: D, name: &[u8]) -> Option<Self> {
        let name = CString::new(name).ok()?;
        let mode = Mode {
            read: true,
            write: false,
            create: Create::Existing,
        };
        let file = folder.open(&name, mode).ok()?;
        Some(Self { folder, file })
    }
}

impl<D: Deref<Target: Folder>> Read for File<D> {
    fn read(&mut self, into: &mut [u8]) -> io::Result<usize> {
        let moved = self
            .folder
            .read(self.file, into)
            .map_err(|_| io::Error::other("the folder refused a read"))?;
        Ok(moved as usize)
    }
}

impl<D: Deref<Target: Folder>> Seek for File<D> {
    fn seek(&mut self, to: SeekFrom) -> io::Result<u64> {
        let too_far = |_| io::Error::from(io::ErrorKind::InvalidInput);
        let (distance, origin) = match to {
            SeekFrom::Start(at) => (i32::try_from(at).map_err(too_far)?, Origin::Start),
            SeekFrom::Current(by) => (i32::try_from(by).map_err(too_far)?, Origin::Here),
            SeekFrom::End(by) => (i32::try_from(by).map_err(too_far)?, Origin::End),
        };
        let at = self
            .folder
            .seek(self.file, distance, origin)
            .map_err(|_| io::Error::other("the folder refused a seek"))?;
        Ok(u64::from(at))
    }
}

impl<D: Deref<Target: Folder>> Drop for File<D> {
    fn drop(&mut self) {
        let _ = self.folder.close(self.file);
    }
}

/// Whether `page` opens an Ogg page that ends exactly where it does. A page's body may hold
/// the capture pattern by chance; it will not fit the end as well.
fn ends_exactly(page: &[u8]) -> bool {
    if !page.starts_with(b"OggS\0") {
        return false;
    }
    let Some(&segments) = page.get(26) else {
        return false;
    };
    let table_end = 27 + usize::from(segments);
    page.get(27..table_end).is_some_and(|lacing| {
        table_end
            + lacing
                .iter()
                .map(|&bytes| usize::from(bytes))
                .sum::<usize>()
            == page.len()
    })
}

/// The granule of the page that ends `tail`, if it is of the stream `serial` names. Should
/// two fit the end, the nearer it wins. Another stream's last page means a chained file,
/// whose length this is not.
pub(crate) fn last_granule(tail: &[u8], serial: &[u8]) -> Option<u64> {
    let page = (0..tail.len())
        .rev()
        .map(|at| &tail[at..])
        .find(|page| ends_exactly(page))?;
    if page[14..18] != *serial {
        return None;
    }
    let granule = u64::from_le_bytes(page[6..14].try_into().ok()?);
    (granule != NO_GRANULE).then_some(granule)
}

/// The pairs in a file, from its last page, leaving it at its start.
fn length(file: &mut impl FileLike) -> Option<u64> {
    let mut head = [0; 18];
    file.read_exact(&mut head).ok()?;
    let size = file.seek(SeekFrom::End(0)).ok()?;
    let reach = size.min(PAGE_MOST);
    file.seek(SeekFrom::Start(size - reach)).ok()?;
    let mut tail = alloc::vec![0; usize::try_from(reach).ok()?];
    file.read_exact(&mut tail).ok()?;
    file.seek(SeekFrom::Start(0)).ok()?;
    last_granule(&tail, &head[14..18])
}

trait FileLike: Read + Seek {}
impl<T: Read + Seek> FileLike for T {}

/// A file ready to decode, and its pairs; `None` if it is a bad one.
fn opened<D: Deref<Target: Folder>>(
    folder: D,
    name: &[u8],
) -> Option<(OggStreamReader<File<D>>, u64)> {
    let mut file = File::open(folder, name)?;
    let total = length(&mut file)?;
    let reader = OggStreamReader::new(file).ok()?;
    let shaped = reader.ident_hdr.audio_sample_rate == RATE_HZ
        && reader.ident_hdr.audio_channels == CHANNELS;
    shaped.then_some((reader, total))
}

/// One track being decoded, and where it stands.
struct Decoding<D: Deref<Target: Folder>> {
    track: u8,
    reader: OggStreamReader<File<D>>,
    total: u64,
    /// The pair the next delivered stands at.
    at: u64,
    /// Decoded numbers, of which `taken` are delivered or skipped.
    held: Vec<i16>,
    taken: usize,
}

impl<D: Deref<Target: Folder>> Decoding<D> {
    /// Decodes until something is held; false at the stream's end.
    fn refill(&mut self) -> Option<bool> {
        while self.taken == self.held.len() {
            let Some(packet) = self.reader.read_dec_packet_itl().ok()? else {
                return Some(false);
            };
            self.held = packet;
            self.taken = 0;
        }
        Some(true)
    }

    /// Decodes and drops up to `goal`; `None` if the file ends before it or will not decode.
    fn skip_to(&mut self, goal: u64) -> Option<()> {
        while self.at < goal {
            if !self.refill()? {
                return None;
            }
            let pairs = ((self.held.len() - self.taken) / 2) as u64;
            let dropped = pairs.min(goal - self.at);
            self.taken += usize::try_from(dropped).ok()? * 2;
            self.at += dropped;
        }
        Some(())
    }

    /// Fills `into` from where it stands, never past the file's length: an unfinished
    /// last page can decode to more.
    fn fill(&mut self, into: &mut [i16]) -> Option<usize> {
        let left = self.total.saturating_sub(self.at).saturating_mul(2);
        let room = usize::try_from(left).map_or(into.len(), |left| left.min(into.len()));
        let mut moved = 0;
        while moved < room {
            if !self.refill()? {
                break;
            }
            let count = (self.held.len() - self.taken).min(room - moved);
            into[moved..moved + count].copy_from_slice(&self.held[self.taken..self.taken + count]);
            moved += count;
            self.taken += count;
        }
        self.at += (moved / 2) as u64;
        Some(moved)
    }

    /// Seeks to the page holding `target` and learns where that left it: the place its first
    /// held pair stands. `None` on the last page, where a decoder that has just sought
    /// leaves the last packet uncut and so the sum cannot be trusted.
    fn seek(&mut self, target: u64) -> Option<u64> {
        self.reader.seek_absgp_pg(target).ok()?;
        self.held.clear();
        self.taken = 0;
        let end = loop {
            self.held.extend(self.reader.read_dec_packet_itl().ok()??);
            if let Some(end) = self.reader.get_last_absgp() {
                break end;
            }
        };
        self.at = end.checked_sub((self.held.len() / 2) as u64)?;
        (end < self.total).then_some(self.at)
    }

    /// Seeks to somewhere at or before `goal`, starting back by a whole packet, as the first
    /// packet after a seek only primes the decoder and gives nothing. False if none did.
    fn seek_near(&mut self, goal: u64) -> bool {
        let mut back = 1u64 << self.reader.ident_hdr.blocksize_1;
        while back < goal {
            if self.seek(goal - back).is_some_and(|at| at <= goal) {
                return true;
            }
            back = back.saturating_mul(WIDER);
        }
        false
    }
}

/// A store's tracks, one open at a time. It holds the folder as `D` does: a borrow, or
/// whatever lets it cross to another thread.
pub struct OggSamples<'t, D: Deref<Target: Folder>> {
    folder: D,
    tracks: &'t [Track],
    open: Option<Decoding<D>>,
}

impl<'t, D: Clone + Deref<Target: Folder>> OggSamples<'t, D> {
    pub const fn new(folder: D, tracks: &'t [Track]) -> Self {
        Self {
            folder,
            tracks,
            open: None,
        }
    }

    /// A file's whole frames, for `disc::spans` to ask; `None` if it is a bad one.
    #[must_use]
    pub fn measure(&self, encoded: &Encoded) -> Option<u32> {
        let (_, total) = opened(self.folder.clone(), &encoded.name)?;
        u32::try_from(total / PAIRS_A_FRAME).ok()
    }

    fn start(&self, track: u8) -> Option<Decoding<D>> {
        let name = self.tracks.iter().find_map(|each| match &each.audio {
            Audio::Encoded(encoded) if each.number == track => Some(&encoded.name),
            _ => None,
        })?;
        let (reader, total) = opened(self.folder.clone(), name)?;
        Some(Decoding {
            track,
            reader,
            total,
            at: 0,
            held: Vec::new(),
            taken: 0,
        })
    }

    fn decoding_at(&self, track: u8, goal: u64) -> Option<Decoding<D>> {
        let mut run = self.start(track)?;
        if goal > 0 && !run.seek_near(goal) {
            run = self.start(track)?;
        }
        run.skip_to(goal)?;
        Some(run)
    }
}

impl<D: Clone + Deref<Target: Folder>> Samples for OggSamples<'_, D> {
    fn read(&mut self, place: Place, into: &mut [i16]) -> Option<usize> {
        let goal = u64::from(place.frame) * PAIRS_A_FRAME;
        let goes_on = self
            .open
            .as_ref()
            .is_some_and(|run| run.track == place.track && run.at == goal);
        if !goes_on {
            self.open = self.decoding_at(place.track, goal);
        }
        let moved = self.open.as_mut()?.fill(into);
        if moved.is_none() {
            self.open = None;
        }
        moved
    }
}
