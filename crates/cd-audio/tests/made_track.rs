// SPDX-License-Identifier: MIT
//! A made disc through the door and the mix, on a held clock and a back that plays nothing: the
//! scorer's logic, each plant read red, and a lying file's seek. The hashes are a second
//! program's, which reads no Rust: `lc6_second.py` in the bench's records. Nothing here reaches
//! a speaker.
//!
//! One test here is ignored, as it is red today: see [`a_lying_seek_hangs_the_player`].

#[path = "../bench/made_track.rs"]
mod made_track;

use std::rc::Rc;

use cd_audio::door::LEAD_FRAMES;
use made_track::{
    Arm, Held, LEAD_MS, LineCounts, Outcome, SeekLie, Time, run_arm, seek_arm, track_hash,
};
use sound_mix::NullBack;

/// The clean script's pairs on a held clock, and the second program's hash of them.
const HELD_RUNS: &str = "2:30:70560;3:0:35280;4:15:123480";
const HELD_HASH: &str = "ee76dfe351cd40ac";

fn held(arm: &Arm) -> Outcome {
    let time: Rc<dyn Time> = Rc::new(Held::default());
    let outcome = run_arm(arm, NullBack, &time, || true, &LineCounts::default);
    println!("{}", outcome.line("held", &format!("{arm:?}")));
    outcome
}

#[test]
fn each_track_hashes_as_the_second_program_worked_it() {
    assert_eq!(track_hash(2), 0xf52c_c0b3_a2f1_531e);
    assert_eq!(track_hash(3), 0xf5ac_3189_6d9b_d7a3);
    assert_eq!(track_hash(4), 0x70f1_d161_1e39_78a2);
    assert_eq!(track_hash(5), 0x6690_511d_d40b_b8f3);
}

/// The door's comment says 250; the frames it counts come to 240.
#[test]
fn the_lead_is_18_frames_or_240_ms() {
    assert_eq!(LEAD_FRAMES, 18);
    assert_eq!(LEAD_MS, 240);
}

#[test]
fn the_clean_script_reads_no_red_and_hashes_as_the_second_program_worked_it() {
    let outcome = held(&Arm::Clean);
    assert_eq!(outcome.reds, Vec::<&str>::new());
    assert_eq!(outcome.sentence, None);
    assert_eq!(outcome.first_wrong, None);
    assert_eq!(outcome.runs, HELD_RUNS);
    assert_eq!(format!("{:016x}", outcome.hash), HELD_HASH);
    assert_eq!(outcome.hash, outcome.worked);
    assert_eq!(outcome.witness, 229_320);
    assert_eq!(outcome.started, "2:30;4:15;4:60");
    assert_eq!(outcome.paused_at, "4:60");
    assert_eq!(outcome.end_short_frames, 0);
    assert_eq!(
        (outcome.gap_ms, outcome.holes, outcome.zero_pairs),
        (0, 0, 0)
    );
}

fn assert_wrong_place(arm: &Arm, place: &str) {
    let outcome = held(arm);
    assert_eq!(outcome.reds, ["mix", "hash"]);
    assert_eq!(
        outcome.sentence.as_deref(),
        Some(
            format!("CD sound came from the wrong place: {place}. The fault is in `cd-audio`.")
                .as_str()
        )
    );
}

#[test]
fn another_tracks_sound_reads_where_it_came_from() {
    assert_wrong_place(&Arm::WrongTrack, "track 3 at 30");
}

#[test]
fn sound_further_on_reads_where_it_came_from() {
    assert_wrong_place(&Arm::WrongPlace, "track 2 at 37");
}

#[test]
fn a_table_too_short_reads_where_the_sound_landed() {
    assert_wrong_place(&Arm::ShortTable, "track 3 at 0");
}

#[test]
fn a_player_never_asked_for_more_reads_its_gap() {
    let outcome = held(&Arm::NoPump);
    assert_eq!(outcome.reds, ["gap"]);
    assert_eq!(outcome.pumps, 0);
    assert_eq!(outcome.witness, 18 * 588);
    assert_eq!(outcome.gap_ms, 1_253);
    assert_eq!(
        outcome.sentence.as_deref(),
        Some("CD sound stopped since nothing asked for more. The gap was 1253 ms.")
    );
}

#[test]
fn calls_faster_than_the_lead_leave_no_gap() {
    let outcome = held(&Arm::Pace(200));
    assert_eq!(outcome.reds, Vec::<&str>::new());
    assert_eq!((outcome.gap_ms, outcome.holes), (0, 0));
    assert_eq!(outcome.hash, outcome.worked);
}

#[test]
fn calls_slower_than_the_lead_read_their_gaps() {
    let outcome = held(&Arm::Pace(400));
    assert_eq!(outcome.reds, ["gap"]);
    assert_eq!(outcome.holes, 9);
    assert_eq!(outcome.gap_ms, 160);
    assert_eq!(outcome.hash, outcome.worked);
    assert_eq!(
        outcome.sentence.as_deref(),
        Some("CD sound has a gap of 160 ms, since calls came slower than its lead.")
    );
}

#[test]
fn a_truthful_seek_ends_with_a_plain_decodes_sound() {
    let read = seek_arm(SeekLie::Truth);
    println!("{}", read.line("held", "seek-truth"));
    assert!(read.ended);
    assert_eq!(read.red(), None);
    assert!(read.right);
}

/// Red today, by reading: `ogg`'s seek asks where it stands before reading each page, and a
/// folder that lands at byte 0 hands it page 0 for ever. `--ignored` reads the sentence.
#[test]
#[ignore = "red today, by reading: a lying seek hangs the player, and no fix is made here"]
fn a_lying_seek_hangs_the_player() {
    let read = seek_arm(SeekLie::Rewinds);
    println!("{}", read.line("held", "seek-lie"));
    assert!(read.ended, "{}", read.sentence().unwrap_or("none"));
}
