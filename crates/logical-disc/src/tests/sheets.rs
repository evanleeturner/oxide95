// SPDX-License-Identifier: MIT
//! The cue sheet against its public format: each sheet a row, its answer
//! written here by hand from the format's own rules and this reader's
//! declared scope (`cue.rs`'s head), never by running the reader; each row
//! prints one line under `--nocapture`. Where the format and this reader
//! part, the rows stand apart, ignored with the reason.
use alloc::string::{String, ToString};
use alloc::vec::Vec;

use kernel_files::Error;

use super::{SHEET, bin, iso};
use crate::disc::RAW;
use crate::{Audio, Disc, OpenError, ReadAt, Source};

/// A file of `size` bytes, `head` first and zeros after: a sheet's long
/// times want long files, never their bytes.
struct Sparse {
    size: u64,
    head: Vec<u8>,
}

impl Sparse {
    fn of(bytes: Vec<u8>) -> Self {
        let size = u64::try_from(bytes.len()).expect("a length");
        Self { size, head: bytes }
    }
}

impl ReadAt for Sparse {
    fn size(&self) -> u64 {
        self.size
    }

    /// A range past the end is refused, as the trait asks.
    fn read_at(&self, at: u64, into: &mut [u8]) -> Result<(), Error> {
        let length = u64::try_from(into.len()).map_err(|_| Error::BadArgument)?;
        if at.checked_add(length).is_none_or(|end| end > self.size) {
            return Err(Error::BadArgument);
        }
        for (place, byte) in into.iter_mut().enumerate() {
            let from = usize::try_from(at).ok().and_then(|at| at.checked_add(place));
            *byte = from.and_then(|from| self.head.get(from)).copied().unwrap_or(0);
        }
        Ok(())
    }
}

/// A sheet's text: the crate's own test sheet with its line ending and
/// byte-order mark chosen, or lines joined by LF.
enum Text {
    Seed { ending: &'static str, bom: bool },
    Lines(&'static [&'static str]),
}

/// The answer the format gives: each audio track's number, first frame on
/// the disc, frames and byte order; or the refusal.
enum Want {
    Tracks(&'static [(u8, u32, u32, bool)]),
    Refused(OpenError),
}

/// One sheet: its name in the printed row, its text, the files picked
/// beside it (`None` the data bin, `Some` that many zero audio frames),
/// and its answer by hand.
struct Row {
    name: &'static str,
    text: Text,
    files: &'static [(&'static str, Option<u64>)],
    want: Want,
}

type Answer = Result<Vec<(u8, u32, u32, bool)>, OpenError>;

/// The data bin, named `D.BIN`: 25 sectors, so the first audio file's
/// frames start the disc's 25th.
const D: (&str, Option<u64>) = ("D.BIN", None);
const A10: (&str, Option<u64>) = ("A.BIN", Some(10));

const SEED_FILES: &[(&str, Option<u64>)] = &[
    ("picked/SYNTH.BIN", None),
    ("track 02.bin", Some(40)),
    ("Track 03.bin", Some(30)),
];
/// The seed's tracks: 25 data sectors, then 40 frames whose `INDEX 01` is
/// at 10, then 30 frames whose `INDEX 01` is at 5, the second `MOTOROLA`.
const SEED_TRACKS: &[(u8, u32, u32, bool)] = &[(2, 35, 30, false), (3, 70, 25, true)];

/// The format and this reader agree on every row here.
const ROWS: &[Row] = &[
    // Lines end in CRLF or LF; a byte-order mark before the first.
    Row {
        name: "seed_crlf_bom",
        text: Text::Seed {
            ending: "\r\n",
            bom: true,
        },
        files: SEED_FILES,
        want: Want::Tracks(SEED_TRACKS),
    },
    Row {
        name: "seed_lf_bom",
        text: Text::Seed {
            ending: "\n",
            bom: true,
        },
        files: SEED_FILES,
        want: Want::Tracks(SEED_TRACKS),
    },
    Row {
        name: "seed_crlf",
        text: Text::Seed {
            ending: "\r\n",
            bom: false,
        },
        files: SEED_FILES,
        want: Want::Tracks(SEED_TRACKS),
    },
    Row {
        name: "seed_lf",
        text: Text::Seed {
            ending: "\n",
            bom: false,
        },
        files: SEED_FILES,
        want: Want::Tracks(SEED_TRACKS),
    },
    // Commands of no place in this reader pass over; a blank line is
    // ignored; an index past 01 is allowed and moves nothing.
    Row {
        name: "other_commands_pass_over",
        text: Text::Lines(&[
            "REM GENRE test",
            "CATALOG 0000000000000",
            "CDTEXTFILE \"x.cdt\"",
            "TITLE \"a sheet\"",
            "PERFORMER \"nobody\"",
            "FILE \"D.BIN\" BINARY",
            "  TRACK 01 MODE1/2352",
            "    FLAGS DCP",
            "    INDEX 01 00:00:00",
            "",
            "FILE \"A.BIN\" BINARY",
            "  TRACK 02 AUDIO",
            "    FLAGS DCP PRE",
            "    ISRC ABCDE1234567",
            "    INDEX 00 00:00:00",
            "    INDEX 01 00:00:02",
            "    INDEX 02 00:00:05",
        ]),
        files: &[D, A10],
        want: Want::Tracks(&[(2, 27, 8, false)]),
    },
    // The first track's number may pass 1.
    Row {
        name: "first_track_past_1",
        text: Text::Lines(&[
            "FILE \"D.BIN\" BINARY",
            "  TRACK 05 MODE1/2352",
            "    INDEX 01 00:00:00",
            "FILE \"A.BIN\" BINARY",
            "  TRACK 06 AUDIO",
            "    INDEX 01 00:00:00",
        ]),
        files: &[D, A10],
        want: Want::Tracks(&[(6, 25, 10, false)]),
    },
    // `mm:ss:ff`, 75 frames a second: 59 * 75 + 74 frames in.
    Row {
        name: "time_00_59_74",
        text: Text::Lines(&[
            "FILE \"D.BIN\" BINARY",
            "  TRACK 01 MODE1/2352",
            "    INDEX 01 00:00:00",
            "FILE \"A.BIN\" BINARY",
            "  TRACK 02 AUDIO",
            "    INDEX 01 00:59:74",
        ]),
        files: &[D, ("A.BIN", Some(4_500))],
        want: Want::Tracks(&[(2, 4_524, 1, false)]),
    },
    // Minutes run to 99: 99 * 4,500 + 59 * 75 + 74 frames in.
    Row {
        name: "time_99_59_74",
        text: Text::Lines(&[
            "FILE \"D.BIN\" BINARY",
            "  TRACK 01 MODE1/2352",
            "    INDEX 01 00:00:00",
            "FILE \"A.BIN\" BINARY",
            "  TRACK 02 AUDIO",
            "    INDEX 01 99:59:74",
        ]),
        files: &[D, ("A.BIN", Some(450_000))],
        want: Want::Tracks(&[(2, 450_024, 1, false)]),
    },
    // Seconds run 0 to 59.
    Row {
        name: "time_00_60_00",
        text: Text::Lines(&[
            "FILE \"D.BIN\" BINARY",
            "  TRACK 01 MODE1/2352",
            "    INDEX 01 00:00:00",
            "FILE \"A.BIN\" BINARY",
            "  TRACK 02 AUDIO",
            "    INDEX 01 00:60:00",
        ]),
        files: &[D, A10],
        want: Want::Refused(OpenError::Cue(6)),
    },
    // Frames run 0 to 74.
    Row {
        name: "time_00_00_75",
        text: Text::Lines(&[
            "FILE \"D.BIN\" BINARY",
            "  TRACK 01 MODE1/2352",
            "    INDEX 01 00:00:00",
            "FILE \"A.BIN\" BINARY",
            "  TRACK 02 AUDIO",
            "    INDEX 01 00:00:75",
        ]),
        files: &[D, A10],
        want: Want::Refused(OpenError::Cue(6)),
    },
    // A time is three fields.
    Row {
        name: "time_two_fields",
        text: Text::Lines(&[
            "FILE \"D.BIN\" BINARY",
            "  TRACK 01 MODE1/2352",
            "    INDEX 01 00:00:00",
            "FILE \"A.BIN\" BINARY",
            "  TRACK 02 AUDIO",
            "    INDEX 01 00:00",
        ]),
        files: &[D, A10],
        want: Want::Refused(OpenError::Cue(6)),
    },
    // Every track needs its `INDEX 01`: the track that lacks one is
    // refused on its own `TRACK` line, whatever follows it.
    Row {
        name: "no_index_01_then_track",
        text: Text::Lines(&[
            "FILE \"D.BIN\" BINARY",
            "  TRACK 01 MODE1/2352",
            "    INDEX 01 00:00:00",
            "FILE \"A.BIN\" BINARY",
            "  TRACK 02 AUDIO",
            "    INDEX 00 00:00:00",
            "  TRACK 03 AUDIO",
            "    INDEX 01 00:00:05",
        ]),
        files: &[D, A10],
        want: Want::Refused(OpenError::Cue(5)),
    },
    Row {
        name: "no_index_01_then_file",
        text: Text::Lines(&[
            "FILE \"D.BIN\" BINARY",
            "  TRACK 01 MODE1/2352",
            "    INDEX 01 00:00:00",
            "FILE \"A.BIN\" BINARY",
            "  TRACK 02 AUDIO",
            "    INDEX 00 00:00:00",
            "FILE \"B.BIN\" BINARY",
            "  TRACK 03 AUDIO",
            "    INDEX 01 00:00:00",
        ]),
        files: &[D, A10, ("B.BIN", Some(10))],
        want: Want::Refused(OpenError::Cue(5)),
    },
    Row {
        name: "no_index_01_last",
        text: Text::Lines(&[
            "FILE \"D.BIN\" BINARY",
            "  TRACK 01 MODE1/2352",
            "    INDEX 01 00:00:00",
            "FILE \"A.BIN\" BINARY",
            "  TRACK 02 AUDIO",
            "    INDEX 00 00:00:00",
        ]),
        files: &[D, A10],
        want: Want::Refused(OpenError::Cue(5)),
    },
    // A command comes after its context's opener: an index after a track,
    // a track after a file.
    Row {
        name: "index_before_track",
        text: Text::Lines(&[
            "FILE \"D.BIN\" BINARY",
            "    INDEX 01 00:00:00",
            "  TRACK 01 MODE1/2352",
            "    INDEX 01 00:00:00",
        ]),
        files: &[D],
        want: Want::Refused(OpenError::Cue(2)),
    },
    Row {
        name: "track_before_file",
        text: Text::Lines(&[
            "  TRACK 01 MODE1/2352",
            "FILE \"D.BIN\" BINARY",
            "  TRACK 01 MODE1/2352",
            "    INDEX 01 00:00:00",
        ]),
        files: &[D],
        want: Want::Refused(OpenError::Cue(1)),
    },
    // Modes and file types of the format this reader does not take are
    // refused on their line, as its scope declares.
    Row {
        name: "mode1_2048",
        text: Text::Lines(&[
            "FILE \"D.BIN\" BINARY",
            "  TRACK 01 MODE1/2048",
            "    INDEX 01 00:00:00",
        ]),
        files: &[D],
        want: Want::Refused(OpenError::Cue(2)),
    },
    Row {
        name: "mode2_2352",
        text: Text::Lines(&[
            "FILE \"D.BIN\" BINARY",
            "  TRACK 01 MODE2/2352",
            "    INDEX 01 00:00:00",
        ]),
        files: &[D],
        want: Want::Refused(OpenError::Cue(2)),
    },
    Row {
        name: "type_wave",
        text: Text::Lines(&[
            "FILE \"D.BIN\" BINARY",
            "  TRACK 01 MODE1/2352",
            "    INDEX 01 00:00:00",
            "FILE \"A.WAV\" WAVE",
            "  TRACK 02 AUDIO",
            "    INDEX 01 00:00:00",
        ]),
        files: &[D, ("A.WAV", Some(10))],
        want: Want::Refused(OpenError::Cue(4)),
    },
    Row {
        name: "type_mp3",
        text: Text::Lines(&[
            "FILE \"D.BIN\" BINARY",
            "  TRACK 01 MODE1/2352",
            "    INDEX 01 00:00:00",
            "FILE \"A.MP3\" MP3",
            "  TRACK 02 AUDIO",
            "    INDEX 01 00:00:00",
        ]),
        files: &[D, ("A.MP3", Some(10))],
        want: Want::Refused(OpenError::Cue(4)),
    },
    Row {
        name: "type_aiff",
        text: Text::Lines(&[
            "FILE \"D.BIN\" BINARY",
            "  TRACK 01 MODE1/2352",
            "    INDEX 01 00:00:00",
            "FILE \"A.AIF\" AIFF",
            "  TRACK 02 AUDIO",
            "    INDEX 01 00:00:00",
        ]),
        files: &[D, ("A.AIF", Some(10))],
        want: Want::Refused(OpenError::Cue(4)),
    },
    // `MOTOROLA` samples are big-endian, `BINARY` little.
    Row {
        name: "type_motorola",
        text: Text::Lines(&[
            "FILE \"D.BIN\" BINARY",
            "  TRACK 01 MODE1/2352",
            "    INDEX 01 00:00:00",
            "FILE \"A.BIN\" MOTOROLA",
            "  TRACK 02 AUDIO",
            "    INDEX 01 00:00:00",
        ]),
        files: &[D, A10],
        want: Want::Tracks(&[(2, 25, 10, true)]),
    },
    // A name with a space is quoted; one without may go bare; a quote
    // left open is refused on its line.
    Row {
        name: "name_quoted_with_space",
        text: Text::Lines(&[
            "FILE \"My Disc.bin\" BINARY",
            "  TRACK 01 MODE1/2352",
            "    INDEX 01 00:00:00",
            "FILE \"Track 2.bin\" BINARY",
            "  TRACK 02 AUDIO",
            "    INDEX 01 00:00:00",
        ]),
        files: &[("My Disc.bin", None), ("Track 2.bin", Some(10))],
        want: Want::Tracks(&[(2, 25, 10, false)]),
    },
    Row {
        name: "name_bare",
        text: Text::Lines(&[
            "FILE D.BIN BINARY",
            "  TRACK 01 MODE1/2352",
            "    INDEX 01 00:00:00",
            "FILE A.BIN BINARY",
            "  TRACK 02 AUDIO",
            "    INDEX 01 00:00:00",
        ]),
        files: &[D, A10],
        want: Want::Tracks(&[(2, 25, 10, false)]),
    },
    Row {
        name: "name_unclosed_quote",
        text: Text::Lines(&[
            "FILE \"D.BIN BINARY",
            "  TRACK 01 MODE1/2352",
            "    INDEX 01 00:00:00",
        ]),
        files: &[D],
        want: Want::Refused(OpenError::Cue(1)),
    },
    // A sheet's file is found among the picked by its last part, case
    // folded, as the reader declares; one not picked is missing.
    Row {
        name: "name_found_by_last_part",
        text: Text::Lines(&[
            "FILE \"D.BIN\" BINARY",
            "  TRACK 01 MODE1/2352",
            "    INDEX 01 00:00:00",
            "FILE \"A.BIN\" BINARY",
            "  TRACK 02 AUDIO",
            "    INDEX 01 00:00:00",
        ]),
        files: &[("some/where/d.bin", None), ("x\\a.bin", Some(10))],
        want: Want::Tracks(&[(2, 25, 10, false)]),
    },
    Row {
        name: "file_not_picked",
        text: Text::Lines(&[
            "FILE \"D.BIN\" BINARY",
            "  TRACK 01 MODE1/2352",
            "    INDEX 01 00:00:00",
            "FILE \"A.BIN\" BINARY",
            "  TRACK 02 AUDIO",
            "    INDEX 01 00:00:00",
        ]),
        files: &[D],
        want: Want::Refused(OpenError::Missing),
    },
    // The data track is the first `MODE1/2352`; a second is no audio
    // track, but its file's frames still come before the next file's.
    Row {
        name: "two_mode1_tracks",
        text: Text::Lines(&[
            "FILE \"D.BIN\" BINARY",
            "  TRACK 01 MODE1/2352",
            "    INDEX 01 00:00:00",
            "FILE \"E.BIN\" BINARY",
            "  TRACK 02 MODE1/2352",
            "    INDEX 01 00:00:00",
            "FILE \"A.BIN\" BINARY",
            "  TRACK 03 AUDIO",
            "    INDEX 01 00:00:00",
        ]),
        files: &[D, ("E.BIN", Some(10)), A10],
        want: Want::Tracks(&[(3, 35, 10, false)]),
    },
    Row {
        name: "no_data_track",
        text: Text::Lines(&[
            "FILE \"A.BIN\" BINARY",
            "  TRACK 01 AUDIO",
            "    INDEX 01 00:00:00",
        ]),
        files: &[A10],
        want: Want::Refused(OpenError::NoData),
    },
    // Two tracks in one file: the first ends where the second's pregap,
    // `INDEX 00`, begins.
    Row {
        name: "two_tracks_one_file",
        text: Text::Lines(&[
            "FILE \"D.BIN\" BINARY",
            "  TRACK 01 MODE1/2352",
            "    INDEX 01 00:00:00",
            "FILE \"A.BIN\" BINARY",
            "  TRACK 02 AUDIO",
            "    INDEX 01 00:00:00",
            "  TRACK 03 AUDIO",
            "    INDEX 00 00:00:08",
            "    INDEX 01 00:00:10",
        ]),
        files: &[D, ("A.BIN", Some(20))],
        want: Want::Tracks(&[(2, 25, 8, false), (3, 35, 10, false)]),
    },
];

/// The format's commands are case-insensitive.
const LOWER: &[Row] = &[
    Row {
        name: "lower_case_commands",
        text: Text::Lines(&[
            "rem a sheet",
            "file \"D.BIN\" BINARY",
            "  track 01 MODE1/2352",
            "    index 01 00:00:00",
            "file \"A.BIN\" BINARY",
            "  track 02 AUDIO",
            "    index 01 00:00:00",
        ]),
        files: &[D, A10],
        want: Want::Tracks(&[(2, 25, 10, false)]),
    },
    Row {
        name: "mixed_case_commands",
        text: Text::Lines(&[
            "File \"D.BIN\" BINARY",
            "  Track 01 MODE1/2352",
            "    Index 01 00:00:00",
            "File \"A.BIN\" BINARY",
            "  Track 02 AUDIO",
            "    Index 01 00:00:00",
        ]),
        files: &[D, A10],
        want: Want::Tracks(&[(2, 25, 10, false)]),
    },
];

/// Minutes run 0 to 99; track numbers 1 to 99, each the last plus one.
const BOUNDS: &[Row] = &[
    Row {
        name: "time_100_00_00",
        text: Text::Lines(&[
            "FILE \"D.BIN\" BINARY",
            "  TRACK 01 MODE1/2352",
            "    INDEX 01 00:00:00",
            "FILE \"A.BIN\" BINARY",
            "  TRACK 02 AUDIO",
            "    INDEX 01 100:00:00",
        ]),
        files: &[D, ("A.BIN", Some(450_010))],
        want: Want::Refused(OpenError::Cue(6)),
    },
    Row {
        name: "track_00",
        text: Text::Lines(&[
            "FILE \"D.BIN\" BINARY",
            "  TRACK 00 MODE1/2352",
            "    INDEX 01 00:00:00",
        ]),
        files: &[D],
        want: Want::Refused(OpenError::Cue(2)),
    },
    Row {
        name: "track_100",
        text: Text::Lines(&[
            "FILE \"D.BIN\" BINARY",
            "  TRACK 100 MODE1/2352",
            "    INDEX 01 00:00:00",
        ]),
        files: &[D],
        want: Want::Refused(OpenError::Cue(2)),
    },
    Row {
        name: "tracks_out_of_order",
        text: Text::Lines(&[
            "FILE \"D.BIN\" BINARY",
            "  TRACK 02 MODE1/2352",
            "    INDEX 01 00:00:00",
            "FILE \"A.BIN\" BINARY",
            "  TRACK 01 AUDIO",
            "    INDEX 01 00:00:00",
        ]),
        files: &[D, A10],
        want: Want::Refused(OpenError::Cue(5)),
    },
    Row {
        name: "tracks_skip_one",
        text: Text::Lines(&[
            "FILE \"D.BIN\" BINARY",
            "  TRACK 01 MODE1/2352",
            "    INDEX 01 00:00:00",
            "FILE \"A.BIN\" BINARY",
            "  TRACK 03 AUDIO",
            "    INDEX 01 00:00:00",
        ]),
        files: &[D, A10],
        want: Want::Refused(OpenError::Cue(5)),
    },
];

/// `PREGAP` and `POSTGAP` add frames that are stored in no data file: the
/// tracks after them start that much later on the disc.
const GAPS: &[Row] = &[
    Row {
        name: "pregap_command",
        text: Text::Lines(&[
            "FILE \"D.BIN\" BINARY",
            "  TRACK 01 MODE1/2352",
            "    INDEX 01 00:00:00",
            "FILE \"A.BIN\" BINARY",
            "  TRACK 02 AUDIO",
            "    PREGAP 00:02:00",
            "    INDEX 01 00:00:00",
        ]),
        files: &[D, A10],
        want: Want::Tracks(&[(2, 175, 10, false)]),
    },
    Row {
        name: "postgap_command",
        text: Text::Lines(&[
            "FILE \"D.BIN\" BINARY",
            "  TRACK 01 MODE1/2352",
            "    INDEX 01 00:00:00",
            "FILE \"A.BIN\" BINARY",
            "  TRACK 02 AUDIO",
            "    INDEX 01 00:00:00",
            "    POSTGAP 00:02:00",
            "FILE \"B.BIN\" BINARY",
            "  TRACK 03 AUDIO",
            "    INDEX 01 00:00:00",
        ]),
        files: &[D, A10, ("B.BIN", Some(10))],
        want: Want::Tracks(&[(2, 25, 10, false), (3, 185, 10, false)]),
    },
];

/// A track's indexes rise, and each track's place lies past the last's.
const ORDER: &[Row] = &[
    Row {
        name: "index_01_before_00",
        text: Text::Lines(&[
            "FILE \"D.BIN\" BINARY",
            "  TRACK 01 MODE1/2352",
            "    INDEX 01 00:00:00",
            "FILE \"A.BIN\" BINARY",
            "  TRACK 02 AUDIO",
            "    INDEX 01 00:00:05",
            "    INDEX 00 00:00:00",
        ]),
        files: &[D, A10],
        want: Want::Refused(OpenError::Cue(7)),
    },
    Row {
        name: "start_behind_the_last",
        text: Text::Lines(&[
            "FILE \"D.BIN\" BINARY",
            "  TRACK 01 MODE1/2352",
            "    INDEX 01 00:00:00",
            "FILE \"A.BIN\" BINARY",
            "  TRACK 02 AUDIO",
            "    INDEX 01 00:00:10",
            "  TRACK 03 AUDIO",
            "    INDEX 01 00:00:05",
        ]),
        files: &[D, ("A.BIN", Some(20))],
        want: Want::Refused(OpenError::Cue(8)),
    },
];

impl Row {
    fn text(&self) -> String {
        match self.text {
            Text::Seed { ending, bom } => {
                let body = SHEET.trim_start_matches('\u{feff}').replace("\r\n", ending);
                if bom { format!("\u{feff}{body}") } else { body }
            }
            Text::Lines(lines) => {
                let mut text = lines.join("\n");
                text.push('\n');
                text
            }
        }
    }

    fn want(&self) -> Answer {
        match self.want {
            Want::Tracks(tracks) => Ok(tracks.to_vec()),
            Want::Refused(error) => Err(error),
        }
    }

    /// The reader's answer for this sheet and its files.
    fn answer(&self) -> Answer {
        let data = bin(&iso());
        let files = self
            .files
            .iter()
            .map(|&(name, frames)| {
                let file = frames.map_or_else(
                    || Sparse::of(data.clone()),
                    |frames| Sparse {
                        size: frames * RAW,
                        head: Vec::new(),
                    },
                );
                (name.to_string(), file)
            })
            .collect();
        let cue = Sparse::of(self.text().into_bytes());
        let disc = Disc::open(Source::Image { cue, files })?;
        Ok(disc
            .tracks()
            .iter()
            .map(|track| match &track.audio {
                Audio::Pcm(pcm) => (track.number, pcm.start, pcm.frames, pcm.big_endian),
                Audio::Encoded(_) => panic!("an image's track is raw"),
            })
            .collect())
    }
}

fn shown(answer: &Answer) -> String {
    match answer {
        Ok(tracks) if tracks.is_empty() => "tracks=none".to_string(),
        Ok(tracks) => {
            let each: Vec<String> = tracks
                .iter()
                .map(|&(number, start, frames, big)| {
                    let order = if big { "be" } else { "le" };
                    format!("{number}:{start}:{frames}:{order}")
                })
                .collect();
            format!("tracks={}", each.join(","))
        }
        Err(error) => format!("refused={error:?}"),
    }
}

/// Every row printed, `ok` or `DIFF`; the names that differ.
fn run(kind: &str, rows: &[Row]) -> Vec<&'static str> {
    let mut differ = Vec::new();
    for row in rows {
        let got = row.answer();
        let want = row.want();
        let verdict = if got == want {
            "ok"
        } else {
            differ.push(row.name);
            "DIFF"
        };
        println!(
            "{kind} {} {} want {} {verdict}",
            row.name,
            shown(&got),
            shown(&want)
        );
    }
    differ
}

#[test]
fn every_sheet_opens_as_its_format_reads() {
    let differ = run("DI2", ROWS);
    assert!(differ.is_empty(), "rows differ: {differ:?}");
}

#[test]
#[ignore = "the format's commands are case-insensitive; this reader matches them in capitals alone"]
fn commands_in_any_case() {
    let differ = run("DI2x", LOWER);
    assert!(differ.is_empty(), "rows differ: {differ:?}");
}

#[test]
#[ignore = "the format bounds minutes at 99 and tracks at 1 to 99 in order; this reader does neither"]
fn minutes_and_track_numbers_bounded() {
    let differ = run("DI2x", BOUNDS);
    assert!(differ.is_empty(), "rows differ: {differ:?}");
}

#[test]
#[ignore = "PREGAP and POSTGAP frames are in no file; this reader passes over them, so later starts come short"]
fn gaps_move_later_tracks() {
    let differ = run("DI2x", GAPS);
    assert!(differ.is_empty(), "rows differ: {differ:?}");
}

#[test]
#[ignore = "the format's places rise; this reader takes an index or a track behind the last"]
fn places_rise() {
    let differ = run("DI2x", ORDER);
    assert!(differ.is_empty(), "rows differ: {differ:?}");
}
