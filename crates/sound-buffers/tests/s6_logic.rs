// SPDX-License-Identifier: MIT
//! S6's logic with no device: the worked ring through the real mixer and a witness, on a
//! clock this test holds. The route runs the same ring through 32-bit calls on a real back;
//! this one writes through `with_bytes`, as a 64-bit host's word from `Lock` is not a pointer.

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use sound_buffers::buffers::{Buffers, Shape};
use sound_buffers::mix_back::MixBack;
use sound_buffers::sdk::{DS_OK, DSBPLAY_LOOPING};
use sound_mix::output::NullBack;
use sound_mix::witness::{Summary, Witness};
use sound_mix::{Front, Host};

#[path = "../bench/ring.rs"]
mod ring;

use ring::{
    FIRST_SPAN_BYTES, FLIP_FRAME, Placed, RING_BYTES, WRAP_AT, ring_format, ring_image, worked_hash,
};

/// 1.2 s at 44,100 Hz.
const FRAMES: u64 = 52_920;
/// `lc5_second.py`'s FNV-1a of those frames, worked in Python from the formula alone.
const PINNED: u64 = 0x56a9_0760_3e34_5e89;
const STEP_US: u64 = 10_000;
const END_US: u64 = 1_200_000;

static MIX_ROW: sound_mix::table::Row = sound_mix::table::Row {
    file: "a bench",
    module: "MSS32",
};
static ROW: sound_buffers::table::Row = sound_buffers::table::Row {
    file: "a bench",
    levels: &[2],
};

/// The clock both the mix and the buffers read, moved by the test.
#[derive(Clone, Default)]
struct Held(Arc<AtomicU64>);

impl Host for Held {
    fn now_us(&self) -> u64 {
        self.0.load(Ordering::Relaxed)
    }

    fn call_back(&self, _callback: u32, _handle: u32) {}

    fn read_file(&self, _name: &str) -> Option<u32> {
        None
    }

    fn free_block(&self, _block: u32) {}
}

#[derive(Clone, Copy)]
enum Plant {
    Clean,
    Flip,
    /// The second span written 4 bytes on, as a wrong second word from `Lock` would put it.
    Wrap,
}

struct Played {
    summary: Summary,
    first_wrong: Option<u64>,
    wrong: u64,
}

fn played(plant: Plant) -> Played {
    let held = Held::default();
    let placed = Placed::new(NullBack);
    let witness = Witness::new(placed.clone());
    let seen = witness.seen();
    let mix = Front::new(&MIX_ROW, held.clone(), witness);
    let clock = held.clone();
    let buffers = Buffers::new(&ROW, MixBack::new(&mix, move || clock.now_us()));
    assert_eq!(buffers.direct_sound_create(0, true, 0), DS_OK);
    let id = buffers
        .create_sound_buffer(
            Some(Shape {
                flags: 0,
                bytes: RING_BYTES,
                format: Some(ring_format()),
            }),
            true,
            0,
        )
        .expect("a good shape");

    let spans = buffers
        .lock(id, WRAP_AT, RING_BYTES, 0, true, true)
        .expect("a good lock");
    assert_eq!(
        (spans.first_at, spans.first_bytes, spans.second_bytes),
        (WRAP_AT, FIRST_SPAN_BYTES, WRAP_AT),
    );
    let image = ring_image(matches!(plant, Plant::Flip));
    let (start, tail) = image.split_at(WRAP_AT as usize);
    let second_at = if matches!(plant, Plant::Wrap) { 4 } else { 0 };
    buffers
        .with_bytes(id, |cells| {
            for (cell, byte) in cells[WRAP_AT as usize..].iter().zip(tail) {
                cell.set(*byte);
            }
            for (cell, byte) in cells[second_at..].iter().zip(start) {
                cell.set(*byte);
            }
        })
        .expect("it is live");

    assert_eq!(buffers.play(id, DSBPLAY_LOOPING), DS_OK);
    let mut now_us = 0;
    while now_us < END_US {
        now_us += STEP_US;
        held.0.store(now_us, Ordering::Relaxed);
        buffers.pump();
        mix.pump();
    }
    assert_eq!(buffers.stop(id), DS_OK);
    let tally = placed.places();
    Played {
        summary: seen.summary(),
        first_wrong: tally.first_wrong.get(),
        wrong: tally.wrong.get(),
    }
}

/// Python's formula and this file's agree, so a wrong formula here cannot hide below.
#[test]
fn the_ring_s_arithmetic_is_the_second_answer_s() {
    assert_eq!(worked_hash(FRAMES), PINNED);
}

#[test]
fn a_clean_ring_gives_the_worked_frames_across_its_seams() {
    let clean = played(Plant::Clean);
    assert_eq!(clean.summary.frames, FRAMES);
    assert_eq!(clean.summary.hash, PINNED);
    assert_eq!(clean.first_wrong, None);
    assert_eq!(clean.wrong, 0);
}

/// Frame 15,000 sounds twice in 52,920 frames: at its place and one ring later.
#[test]
fn a_flipped_sample_reads_red_at_its_frame() {
    let flip = played(Plant::Flip);
    assert_eq!(flip.first_wrong, Some(u64::from(FLIP_FRAME)));
    assert_eq!(flip.wrong, 2);
    assert_ne!(flip.summary.hash, PINNED);
}

#[test]
fn a_span_written_4_bytes_on_reads_red() {
    let wrap = played(Plant::Wrap);
    assert_eq!(wrap.first_wrong, Some(0));
    assert_ne!(wrap.summary.hash, PINNED);
}
