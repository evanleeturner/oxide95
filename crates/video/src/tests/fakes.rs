// SPDX-License-Identifier: MIT
//! The fakes for the three backs, and what a caller does with a handle.
use std::collections::BTreeMap;

use video_smacker::{Clip, Picture};

use crate::{Choices, ClipBytes, Clock, Handle, PALETTE_BYTES, Place, SoundDoor, Track, Videos};

/// A clock its test moves by hand.
#[derive(Default)]
pub(super) struct FakeClock(pub(super) u64);

impl Clock for FakeClock {
    fn now_us(&self) -> u64 {
        self.0
    }
}

/// Files by name.
#[derive(Default)]
pub(super) struct FakeBytes(pub(super) BTreeMap<Vec<u8>, Vec<u8>>);

impl ClipBytes for FakeBytes {
    fn read(&mut self, name: &[u8]) -> Option<Vec<u8>> {
        self.0.get(name).cloned()
    }
}

/// A door that counts every call it takes.
#[derive(Default)]
pub(super) struct FakeDoor {
    pub(super) calls: u32,
}

impl SoundDoor for FakeDoor {
    fn stream_open(&mut self, _: Track) -> u32 {
        self.calls += 1;
        0
    }

    fn stream_push(&mut self, _: u32, _: &[i16]) {
        self.calls += 1;
    }

    fn stream_close(&mut self, _: u32) {
        self.calls += 1;
    }
}

pub(super) type Fakes = Videos<FakeClock, FakeBytes, FakeDoor>;

/// The fronts holding `clip` as `a.smk`, the clock at `now_us`.
pub(super) fn holding(clip: &[u8], now_us: u64, choices: Choices) -> Fakes {
    let mut bytes = FakeBytes::default();
    bytes.0.insert(b"a.smk".to_vec(), clip.to_vec());
    Videos::new(FakeClock(now_us), bytes, FakeDoor::default(), choices)
}

/// A caller's buffer.
pub(super) struct Buffer {
    pub(super) pixels: Vec<u8>,
    pub(super) place: Place,
}

impl Buffer {
    pub(super) fn new(place: Place) -> Self {
        Self {
            pixels: vec![0; place.pitch as usize * place.height as usize],
            place,
        }
    }

    /// A clip-sized buffer at the corner.
    pub(super) fn sized(width: u32, height: u32) -> Self {
        Self::new(Place {
            x: 0,
            y: 0,
            pitch: width,
            height,
        })
    }

    /// The clip's rect, row by row.
    pub(super) fn rect(&self, width: usize, height: usize) -> Vec<u8> {
        let mut rect = Vec::with_capacity(width * height);
        for row in 0..height {
            let at = (self.place.y as usize + row) * self.place.pitch as usize;
            let at = at + self.place.x as usize;
            rect.extend_from_slice(&self.pixels[at..at + width]);
        }
        rect
    }

    /// Names this buffer to the clip; it outlives the clip's use of it in every test.
    pub(super) fn name_to(&mut self, videos: &mut Fakes, handle: *const Handle) {
        // SAFETY: `pixels` holds pitch * height bytes and neither it nor the
        // allocation moves while the test uses the clip.
        unsafe {
            videos.smack_to_buffer(handle, self.place, self.pixels.as_mut_ptr(), 0);
        }
    }
}

/// The 5 fields, read through the pointer as a caller reads them.
pub(super) fn read_handle(handle: *const Handle) -> (u32, u32, u32, u32, [u8; PALETTE_BYTES]) {
    // SAFETY: a handle `smack_open` answered and no close has taken.
    unsafe {
        (
            (*handle).width,
            (*handle).height,
            (*handle).frames,
            (*handle).palette_changed,
            (*handle).palette,
        )
    }
}

/// Every frame by a direct decode.
pub(super) fn direct(clip: &[u8]) -> Vec<Picture> {
    let mut clip = Clip::parse(clip).expect("a built clip parses");
    (0..clip.header().frames)
        .map(|_| {
            let mut picture = Picture::new();
            clip.decode_next_frame(&mut picture)
                .expect("a built clip decodes");
            picture
        })
        .collect()
}
