// SPDX-License-Identifier: MIT
//! The mix against the reference over [`NullBack`]: no device, no stream, so it runs in
//! the default `cargo test`. A red here is the mix, or the worked answer, and no route.

#[path = "../bench/reference.rs"]
mod reference;

use reference::{END_US, FRAMES, Outcome, PINNED_HASH, PLANTS, Plant, Script};
use sound_mix::NullBack;
use sound_mix::witness::{HASH_START, hash_samples};

/// Worked by hand from the voices' formulas, before any program computed them.
const HAND_WORKED: [(u32, i16, i16); 15] = [
    (0, -15_000, -15_064),
    (1, -14_700, -14_764),
    (2, -14_271, -14_529),
    (99, 21_021, 13_088),
    (4_409, -11_784, -12_622),
    (4_410, -12_000, -12_064),
    (8_819, -8_784, -9_622),
    (8_820, -9_000, -9_000),
    (22_050, 30_000, -30_000),
    (22_060, 32_767, -26_980),
    (22_100, 14_980, -32_768),
    (22_149, 29_660, -30_270),
    (33_073, 32_767, -23_060),
    (33_074, 7_200, 7_200),
    (88_199, 14_700, 14_700),
];

/// The steps the clock takes: 1 ms, 10 ms, 37 ms, and one leap to the end.
const PACES_US: [u64; 4] = [1_000, 10_000, 37_000, END_US];

fn run_at(plant: Plant, step_us: u64) -> Outcome {
    let script = Script::load(NullBack, plant);
    script.start();
    let mut now_us = 0;
    while now_us < END_US {
        now_us += step_us;
        script.run_to(now_us);
    }
    script.close();
    script.outcome()
}

#[test]
fn the_worked_stream_holds_the_points_worked_by_hand() {
    for (frame, left, right) in HAND_WORKED {
        assert_eq!(reference::frame_at(frame), (left, right), "frame {frame}");
    }
}

/// Prints each block's hash, which `--nocapture` shows beside the second program's list.
#[test]
fn the_worked_stream_is_the_second_programs() {
    let stream = reference::worked();
    assert_eq!(stream.len(), FRAMES as usize * 2);
    assert_eq!(hash_samples(HASH_START, &stream), PINNED_HASH);
    for (block, hash) in reference::block_hashes(&stream).iter().enumerate() {
        println!("block {block} {hash:016x}");
    }
}

#[test]
fn the_mix_makes_the_worked_stream_at_every_pace() {
    for step_us in PACES_US {
        let outcome = run_at(Plant::Clean, step_us);
        assert_eq!(outcome.first_wrong, None, "step {step_us} us");
        assert_eq!(outcome.wrong, 0, "step {step_us} us");
        assert_eq!(outcome.frames, u64::from(FRAMES), "step {step_us} us");
        assert_eq!(outcome.kept, u64::from(FRAMES), "step {step_us} us");
        assert_eq!(outcome.hash, PINNED_HASH, "step {step_us} us");
        assert_eq!(outcome.sentence(), None, "step {step_us} us");
    }
}

#[test]
fn a_planted_input_reads_wrong_at_its_block() {
    for (plant, block) in PLANTS {
        let outcome = run_at(plant, 10_000);
        assert_eq!(outcome.block(), Some(block), "{plant:?}");
        assert_ne!(outcome.hash, PINNED_HASH, "{plant:?}");
        assert_eq!(
            outcome.sentence().as_deref(),
            Some(
                format!(
                    "The sound mixer gave different sound from the worked answer at block \
                     {block}. Your computer is cleared."
                )
                .as_str()
            ),
            "{plant:?}"
        );
    }
}
