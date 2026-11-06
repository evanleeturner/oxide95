// SPDX-License-Identifier: MIT
//! Falsifiers 1, 2, 3, 5 and 6, on built clips.
use std::ptr;

use super::clips::{long_clip, painting_clip, track_clip};
use super::fakes::{Buffer, direct, holding, read_handle};
use super::test_log::debug_lines;
use crate::{Choices, GotoBase, Place, VoidBlocks};

/// A shell's screen: the clip somewhere inside a wider, taller buffer.
const SCREEN: Place = Place {
    x: 8,
    y: 4,
    pitch: 64,
    height: 32,
};

#[test]
fn do_then_next_paints_what_a_direct_decode_paints_at_each_frame() {
    let bytes = painting_clip();
    let pictures = direct(&bytes);
    let mut videos = holding(&bytes, 0, Choices::default());
    let mut buffer = Buffer::new(SCREEN);
    let handle = videos.smack_open(b"a.smk", 0, 0);
    buffer.name_to(&mut videos, handle);
    for picture in &pictures {
        videos.smack_do_frame(handle);
        assert_eq!(buffer.rect(16, 8), picture.pixels);
        videos.smack_next_frame(handle);
    }
    assert_eq!(read_handle(handle).2 as usize, pictures.len());
}

#[test]
fn next_then_do_paints_what_a_direct_decode_paints_even_past_a_skipped_frame() {
    let bytes = painting_clip();
    let pictures = direct(&bytes);
    let mut videos = holding(&bytes, 0, Choices::default());
    let mut buffer = Buffer::new(SCREEN);
    let handle = videos.smack_open(b"a.smk", 0, 0);
    buffer.name_to(&mut videos, handle);
    for picture in &pictures[1..] {
        videos.smack_next_frame(handle);
        videos.smack_do_frame(handle);
        assert_eq!(buffer.rect(16, 8), picture.pixels);
    }
}

#[test]
fn do_frame_twice_in_a_row_paints_once_and_keeps_the_cursor() {
    let bytes = painting_clip();
    let pictures = direct(&bytes);
    let mut videos = holding(&bytes, 0, Choices::default());
    let mut buffer = Buffer::sized(16, 8);
    let handle = videos.smack_open(b"a.smk", 0, 0);
    buffer.name_to(&mut videos, handle);
    let lines = debug_lines(|| {
        videos.smack_do_frame(handle);
        buffer.pixels[0] = 0xee;
        videos.smack_do_frame(handle);
    });
    assert_eq!(buffer.pixels[0], 0xee, "the second call wrote nothing");
    let frames = lines.iter().filter(|one| one.starts_with("smk.frame"));
    assert_eq!(frames.count(), 1);
    videos.smack_next_frame(handle);
    videos.smack_do_frame(handle);
    assert_eq!(buffer.pixels[4..16], pictures[1].pixels[4..16]);
}

#[test]
fn a_void_block_leaves_the_target_by_default() {
    let bytes = painting_clip();
    let mut videos = holding(&bytes, 0, Choices::default());
    let mut buffer = Buffer::sized(16, 8);
    let handle = videos.smack_open(b"a.smk", 0, 0);
    buffer.name_to(&mut videos, handle);
    videos.smack_do_frame(handle);
    videos.smack_next_frame(handle);
    buffer.pixels[15] = 0xee;
    videos.smack_do_frame(handle);
    assert_eq!(buffer.pixels[15], 0xee, "block 3 is void in frame 1");
    assert_eq!(buffer.pixels[0], 4, "block 0 is painted in frame 1");
}

#[test]
fn the_void_flag_writes_every_pixel() {
    let bytes = painting_clip();
    let choices = Choices {
        void_blocks: VoidBlocks::Written,
        ..Choices::default()
    };
    let mut videos = holding(&bytes, 0, choices);
    let mut buffer = Buffer::sized(16, 8);
    let handle = videos.smack_open(b"a.smk", 0, 0);
    buffer.name_to(&mut videos, handle);
    videos.smack_do_frame(handle);
    videos.smack_next_frame(handle);
    buffer.pixels[15] = 0xee;
    videos.smack_do_frame(handle);
    assert_eq!(buffer.pixels[15], 3, "written from the picture");
}

#[test]
fn goto_one_is_the_first_frame_by_default() {
    let bytes = painting_clip();
    let pictures = direct(&bytes);
    let mut videos = holding(&bytes, 0, Choices::default());
    let mut buffer = Buffer::sized(16, 8);
    let handle = videos.smack_open(b"a.smk", 0, 0);
    buffer.name_to(&mut videos, handle);
    for _ in 0..3 {
        videos.smack_do_frame(handle);
        videos.smack_next_frame(handle);
    }
    videos.smack_goto(handle, 1);
    videos.smack_do_frame(handle);
    assert_eq!(buffer.rect(16, 8), pictures[0].pixels);
    assert_eq!(read_handle(handle).3, 1, "a rewind replays the palette");
}

#[test]
fn the_goto_flag_makes_one_the_second_frame() {
    let bytes = painting_clip();
    let pictures = direct(&bytes);
    let choices = Choices {
        goto_base: GotoBase::FromZero,
        ..Choices::default()
    };
    let mut videos = holding(&bytes, 0, choices);
    let mut buffer = Buffer::sized(16, 8);
    let handle = videos.smack_open(b"a.smk", 0, 0);
    buffer.name_to(&mut videos, handle);
    videos.smack_goto(handle, 1);
    videos.smack_do_frame(handle);
    assert_eq!(buffer.rect(16, 8), pictures[1].pixels);
}

#[test]
fn a_goto_naming_no_frame_is_refused_and_moves_nothing() {
    let bytes = painting_clip();
    let pictures = direct(&bytes);
    let mut videos = holding(&bytes, 0, Choices::default());
    let mut buffer = Buffer::sized(16, 8);
    let handle = videos.smack_open(b"a.smk", 0, 0);
    buffer.name_to(&mut videos, handle);
    let lines = debug_lines(|| {
        videos.smack_goto(handle, 0);
        videos.smack_goto(handle, 6);
    });
    let refused = lines.iter().filter(|one| one.contains("w=bad-frame"));
    assert_eq!(refused.count(), 2, "{lines:?}");
    videos.smack_do_frame(handle);
    assert_eq!(buffer.rect(16, 8), pictures[0].pixels);
}

#[test]
fn past_the_last_frame_do_frame_refuses_by_name_until_a_goto() {
    let bytes = painting_clip();
    let pictures = direct(&bytes);
    let mut videos = holding(&bytes, 0, Choices::default());
    let mut buffer = Buffer::sized(16, 8);
    let handle = videos.smack_open(b"a.smk", 0, 0);
    buffer.name_to(&mut videos, handle);
    for _ in 0..7 {
        videos.smack_next_frame(handle);
    }
    let lines = debug_lines(|| {
        videos.smack_do_frame(handle);
    });
    assert!(
        lines.iter().any(|one| one.contains("w=decoder")),
        "{lines:?}"
    );
    videos.smack_goto(handle, 5);
    videos.smack_do_frame(handle);
    assert_eq!(buffer.rect(16, 8), pictures[4].pixels);
}

#[test]
fn a_frame_with_no_buffer_named_is_refused_by_name_and_the_next_buffer_gets_it_whole() {
    let bytes = painting_clip();
    let pictures = direct(&bytes);
    let mut videos = holding(&bytes, 0, Choices::default());
    let handle = videos.smack_open(b"a.smk", 0, 0);
    let lines = debug_lines(|| {
        videos.smack_do_frame(handle);
    });
    assert!(
        lines.iter().any(|one| one.contains("w=no-target")),
        "{lines:?}"
    );
    videos.smack_next_frame(handle);
    let mut buffer = Buffer::sized(16, 8);
    buffer.name_to(&mut videos, handle);
    videos.smack_do_frame(handle);
    assert_eq!(buffer.rect(16, 8), pictures[1].pixels);
}

#[test]
fn a_null_buffer_is_refused_by_name() {
    let bytes = painting_clip();
    let mut videos = holding(&bytes, 0, Choices::default());
    let handle = videos.smack_open(b"a.smk", 0, 0);
    let place = Place {
        x: 0,
        y: 0,
        pitch: 16,
        height: 8,
    };
    let lines = debug_lines(|| {
        // SAFETY: a null buffer is refused before anything could write through it.
        unsafe { videos.smack_to_buffer(handle, place, ptr::null_mut(), 0) };
    });
    assert!(
        lines.iter().any(|one| one.contains("w=null-buffer")),
        "{lines:?}"
    );
}

#[test]
fn a_clip_past_the_buffer_is_cut_at_its_edges() {
    let bytes = painting_clip();
    let mut videos = holding(&bytes, 0, Choices::default());
    let mut buffer = Buffer::new(Place {
        x: 10,
        y: 4,
        pitch: 20,
        height: 6,
    });
    let handle = videos.smack_open(b"a.smk", 0, 0);
    buffer.name_to(&mut videos, handle);
    videos.smack_do_frame(handle);
    let painted: usize = buffer
        .pixels
        .iter()
        .map(|pixel| usize::from(*pixel == 3))
        .sum();
    assert_eq!(painted, 10 * 2, "10 columns of 2 rows");
    assert!(buffer.pixels[..80].iter().all(|pixel| *pixel == 0));
}

#[test]
fn the_handle_holds_the_header_and_the_palette() {
    let bytes = painting_clip();
    let pictures = direct(&bytes);
    let mut videos = holding(&bytes, 0, Choices::default());
    let handle = videos.smack_open(b"a.smk", 0, 0);
    let (width, height, frames, changed, _) = read_handle(handle);
    assert_eq!((width, height, frames, changed), (16, 8, 5, 0));
    videos.smack_do_frame(handle);
    let (_, _, _, changed, palette) = read_handle(handle);
    assert_eq!((changed, palette), (1, pictures[0].palette));
    videos.smack_next_frame(handle);
    videos.smack_do_frame(handle);
    assert_eq!(read_handle(handle).3, 0, "frame 1 carries no palette");
}

#[test]
fn a_missing_file_returns_null_logs_it_and_keeps_no_row() {
    let mut videos = holding(&painting_clip(), 0, Choices::default());
    let mut handle = ptr::null_mut();
    let lines = debug_lines(|| handle = videos.smack_open(b"absent.smk", 0xfe000, 0));
    assert!(handle.is_null());
    assert_eq!(videos.open_clips(), 0);
    assert!(
        lines.iter().any(|one| one.contains("p=absent.smk")),
        "{lines:?}"
    );
    assert!(lines.iter().any(|one| one.contains("v=0")), "{lines:?}");
}

#[test]
fn a_clip_the_decoder_refuses_opens_as_null() {
    let mut damaged = painting_clip();
    damaged[..4].copy_from_slice(b"SMK4");
    let mut videos = holding(&damaged, 0, Choices::default());
    assert!(videos.smack_open(b"a.smk", 0, 0).is_null());
    assert_eq!(videos.open_clips(), 0);
}

#[test]
fn the_table_has_no_fixed_size_and_a_closed_handle_is_refused_by_name() {
    let mut videos = holding(&painting_clip(), 0, Choices::default());
    let handles: Vec<_> = (0..40).map(|_| videos.smack_open(b"a.smk", 0, 0)).collect();
    assert_eq!(videos.open_clips(), 40);
    let first = handles[0];
    videos.smack_close(first);
    assert_eq!(videos.open_clips(), 39);
    let lines = debug_lines(|| {
        videos.smack_do_frame(first);
    });
    assert!(
        lines.iter().any(|one| one.contains("w=unknown-handle")),
        "{lines:?}"
    );
    assert_eq!(read_handle(handles[39]).2, 5, "the others stand");
}

#[test]
fn sound_in_track_answers_0_for_every_mask_and_the_door_takes_no_call() {
    for clip in [track_clip(), long_clip(2, -1000)] {
        let mut videos = holding(&clip, 0, Choices::default());
        let handle = videos.smack_open(b"a.smk", 0xfe000, 0);
        let mut buffer = Buffer::sized(16, 8);
        buffer.name_to(&mut videos, handle);
        for bit in 0..32 {
            assert_eq!(videos.smack_sound_in_track(handle, 1 << bit), 0);
        }
        assert_eq!(videos.smack_sound_in_track(handle, 0x2000), 0);
        for _ in 0..2 {
            videos.smack_wait(handle);
            videos.smack_do_frame(handle);
            videos.smack_next_frame(handle);
        }
        videos.smack_close(handle);
        assert_eq!(videos.sound().calls, 0);
    }
}

#[test]
fn all_8_names_read_by_video() {
    let bytes = painting_clip();
    let mut videos = holding(&bytes, 0, Choices::default());
    let mut buffer = Buffer::sized(16, 8);
    let lines = debug_lines(|| {
        let handle = videos.smack_open(b"a.smk", 0, 0);
        buffer.name_to(&mut videos, handle);
        videos.smack_wait(handle);
        videos.smack_do_frame(handle);
        videos.smack_next_frame(handle);
        videos.smack_goto(handle, 1);
        videos.smack_sound_in_track(handle, 0x2000);
        videos.smack_close(handle);
    });
    let calls: Vec<_> = lines
        .iter()
        .filter(|one| one.starts_with("imp.call"))
        .collect();
    assert_eq!(calls.len(), 8, "{calls:?}");
    for name in [
        "SmackOpen",
        "SmackToBuffer",
        "SmackWait",
        "SmackDoFrame",
        "SmackNextFrame",
        "SmackGoto",
        "SmackSoundInTrack",
        "SmackClose",
    ] {
        let whole = format!("c=SMACKW32!{name} ");
        assert!(
            calls
                .iter()
                .any(|one| one.contains(&whole) && one.contains("by=video")),
            "{name} in {calls:?}"
        );
    }
}
