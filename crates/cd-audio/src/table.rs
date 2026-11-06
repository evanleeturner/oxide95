// SPDX-License-Identifier: MIT
//! The CD-audio rows: each call's name as the counter labels it, and what a
//! file's calls read back. A file the front serves is a row, kept in its
//! game's rows crate, and nothing else.

/// Each call's name, the line's `c`: the import as the counter spells it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Calls {
    pub send_command: &'static str,
    pub aux_devices: &'static str,
    pub error_string: &'static str,
    pub aux_caps: &'static str,
    pub aux_set_volume: &'static str,
    pub aux_get_volume: &'static str,
}

/// One file's CD-audio calls.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Row {
    pub file: &'static str,
    pub calls: Calls,
    /// How many CD-audio devices the file is told there are.
    pub letters: u32,
    /// How many tracks the disc is said to hold.
    pub tracks: u32,
    /// The name the device goes by; the file hands it back to open it.
    pub device_name: &'static str,
    /// The number an open hands out, and the only one a later call may name.
    pub device_id: u32,
}

/// One audio track on a disc: its number and how long it runs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Span {
    pub number: u8,
    pub frames: u32,
}

/// A place on a disc: a track, and how many frames into it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Place {
    pub track: u8,
    pub frame: u32,
}

/// A disc's audio tracks in order, with the frames between them closed up, so a run that
/// crosses tracks is one count of frames.
#[derive(Clone, Copy, Debug)]
pub struct Tracks<'t>(pub &'t [Span]);

impl Tracks<'_> {
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// The highest number on the disc, so a data track before the audio counts.
    #[must_use]
    pub fn count(&self) -> u32 {
        self.0
            .iter()
            .map(|span| u32::from(span.number))
            .max()
            .unwrap_or(0)
    }

    /// Every frame on the disc.
    #[must_use]
    pub fn end(&self) -> u32 {
        self.0.iter().map(|span| span.frames).sum()
    }

    /// `place` as one count, for a place a run can start at; `None` if it is not on the disc.
    #[must_use]
    pub fn start_of(&self, place: Place) -> Option<u32> {
        self.count_to(place).filter(|_| {
            self.0
                .iter()
                .any(|span| span.number == place.track && place.frame < span.frames)
        })
    }

    /// `place` as one count, for a place a run can end at: a track's own end is on the disc.
    #[must_use]
    pub fn end_of(&self, place: Place) -> Option<u32> {
        self.count_to(place)
    }

    fn count_to(&self, place: Place) -> Option<u32> {
        let mut before = 0;
        for span in self.0 {
            if span.number == place.track {
                return (place.frame <= span.frames).then_some(before + place.frame);
            }
            before += span.frames;
        }
        None
    }

    /// Where the track holding `count` ends, as one count; past the disc, the disc's end.
    #[must_use]
    pub fn track_end(&self, count: u32) -> u32 {
        let mut end = 0;
        for span in self.0 {
            end += span.frames;
            if count < end {
                return end;
            }
        }
        end
    }

    /// The place `count` frames into the disc; past the end, the last track's end.
    #[must_use]
    pub fn place(&self, count: u32) -> Place {
        let mut before = 0;
        for span in self.0 {
            if count < before + span.frames {
                return Place {
                    track: span.number,
                    frame: count - before,
                };
            }
            before += span.frames;
        }
        self.0
            .last()
            .map_or(Place { track: 0, frame: 0 }, |span| Place {
                track: span.number,
                frame: span.frames,
            })
    }
}
