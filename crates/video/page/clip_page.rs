// SPDX-License-Identifier: MIT
//! The thin wasm a page's script runs, one export a step: a clip straight through the
//! decoder, a clip's refusal, a clip by `Place` through `draw`'s page, and a clip's pace.
//! It holds no check and hashes nothing: frames and answers go out as bytes, for a program
//! outside the page to judge. A clip's bytes come in by [`alloc`]. Its sound door plays
//! nothing, and no logger stands but the refusal arm's.
#![cfg(target_arch = "wasm32")]
#![expect(
    unsafe_code,
    reason = "`#[unsafe(no_mangle)]` keeps an export's name, and a handle is read as a caller reads it"
)]
#![allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "the page's numbers are doubles, and these stay far under 2^53"
)]

use core::cell::{Cell, RefCell};
use core::fmt::Write as _;
use std::thread::LocalKey;

use draw::easel::Easel;
use draw::page::Page;
use log::kv::{Error, Key, Value, VisitSource};
use video::{Choices, ClipBytes, Clock, Handle, Place, SoundDoor, Track, Videos};
use video_smacker::{Clip, Picture, Refused};

/// The screen's width past the clip's: 3 past a multiple of 4, so `pitch` differs from it.
const WIDTH_PAST_CLIP: u32 = 7;
/// Where the clip lands. Its bottom touches the screen's, so `height` is tight.
const CLIP_X: u32 = 3;
const CLIP_Y: u32 = 5;
/// Room for the refusal lines, taken before a clip is read, so a line grows no memory.
const LINES_ROOM: usize = 4096;
const DUE: u32 = 0;

#[link(wasm_import_module = "page")]
unsafe extern "C" {
    /// The page's clock in microseconds.
    safe fn clock_us() -> f64;
}

/// The clock over the page's, its last read kept so the shim can shadow a due time
/// with no logger over the crossing.
#[derive(Default)]
struct PageClock {
    last_us: Cell<u64>,
    reads: Cell<u64>,
}

impl Clock for PageClock {
    fn now_us(&self) -> u64 {
        let now = clock_us() as u64;
        self.last_us.set(now);
        self.reads.set(self.reads.get() + 1);
        now
    }
}

/// The one clip, whatever name is asked for.
struct OneClip(Vec<u8>);

impl ClipBytes for OneClip {
    fn read(&mut self, _name: &[u8]) -> Option<Vec<u8>> {
        Some(self.0.clone())
    }
}

/// A door that plays nothing: `plays` stays `false`, so no sample is asked for.
struct Silent;

impl SoundDoor for Silent {
    fn stream_open(&mut self, _track: Track) -> u32 {
        0
    }

    fn stream_push(&mut self, _stream: u32, _samples: &[i16]) {}

    fn stream_close(&mut self, _stream: u32) {}
}

type PageVideos = Videos<PageClock, OneClip, Silent>;

fn videos_over_incoming() -> PageVideos {
    let bytes = INCOMING.with_borrow(Clone::clone);
    Videos::new(
        PageClock::default(),
        OneClip(bytes),
        Silent,
        Choices::default(),
    )
}

struct Decoder {
    clip: Clip,
    picture: Picture,
    rgb: Vec<u8>,
}

struct Screen {
    videos: PageVideos,
    handle: *mut Handle,
    page: &'static Page,
    easel: &'static Easel<&'static Page>,
    palette: u32,
    pixels: Vec<u8>,
    rgba: Vec<u8>,
    width: u32,
    height: u32,
    pitch: u32,
    read_pitch: u32,
}

/// A clip's pace, with `front.rs`' due time worked again beside it.
struct Pace {
    videos: PageVideos,
    handle: *mut Handle,
    period_us: u64,
    anchor_us: u64,
    anchor_frame: u32,
    open_us: u64,
    cursor: u32,
    not_due: u32,
    waits: u64,
    reanchors: u32,
    due_us: u64,
    late_us: u64,
}

thread_local! {
    static INCOMING: RefCell<Vec<u8>> = const { RefCell::new(Vec::new()) };
    static SAID: Cell<&'static str> = const { Cell::new("") };
    static LINES: RefCell<String> = const { RefCell::new(String::new()) };
    static DECODER: RefCell<Option<Decoder>> = const { RefCell::new(None) };
    static SCREEN: RefCell<Option<Screen>> = const { RefCell::new(None) };
    static PACE: RefCell<Option<Pace>> = const { RefCell::new(None) };
}

fn with<T: 'static, R>(
    cell: &'static LocalKey<RefCell<Option<T>>>,
    run: impl FnOnce(&mut T) -> R,
) -> R {
    cell.with_borrow_mut(|one| run(one.as_mut().expect("the arm's open comes first")))
}

/// Keeps the refusal's name for the page; the answer that says one is waiting.
fn said(why: Refused) -> u32 {
    SAID.set(why.name());
    1
}

/// Room for a clip's bytes, which the page writes at the answer.
#[unsafe(no_mangle)]
pub extern "C" fn alloc(length: u32) -> *mut u8 {
    INCOMING.with_borrow_mut(|bytes| {
        *bytes = vec![0; length as usize];
        bytes.as_mut_ptr()
    })
}

#[unsafe(no_mangle)]
pub extern "C" fn said_ptr() -> *const u8 {
    SAID.get().as_ptr()
}

#[unsafe(no_mangle)]
pub extern "C" fn said_len() -> u32 {
    SAID.get().len() as u32
}

/// `picture` as a player shows it, each index through the palette.
fn to_rgb24(picture: &Picture, rgb: &mut Vec<u8>) {
    rgb.clear();
    for &index in &picture.pixels {
        let at = usize::from(index) * 3;
        rgb.extend_from_slice(&picture.palette[at..at + 3]);
    }
}

/// Parses the clip that came in: 0, or 1 with a name waiting.
#[unsafe(no_mangle)]
pub extern "C" fn clip_open() -> u32 {
    match INCOMING.with_borrow(|bytes| Clip::parse(bytes)) {
        Ok(clip) => {
            let decoder = Decoder {
                clip,
                picture: Picture::new(),
                rgb: Vec::new(),
            };
            DECODER.set(Some(decoder));
            0
        }
        Err(why) => said(why),
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn clip_frames() -> u32 {
    with(&DECODER, |one| one.clip.header().frames)
}

#[unsafe(no_mangle)]
pub extern "C" fn clip_cursor() -> u32 {
    with(&DECODER, |one| one.clip.cursor())
}

#[unsafe(no_mangle)]
pub extern "C" fn clip_seek(frame: u32) -> u32 {
    with(&DECODER, |one| match one.clip.seek(frame) {
        Ok(()) => 0,
        Err(why) => said(why),
    })
}

/// The next frame, its rgb24 at [`rgb_ptr`].
#[unsafe(no_mangle)]
pub extern "C" fn clip_decode() -> u32 {
    with(&DECODER, |one| {
        match one.clip.decode_next_frame(&mut one.picture) {
            Ok(_) => {
                to_rgb24(&one.picture, &mut one.rgb);
                0
            }
            Err(why) => said(why),
        }
    })
}

#[unsafe(no_mangle)]
pub extern "C" fn rgb_ptr() -> *const u8 {
    with(&DECODER, |one| one.rgb.as_ptr())
}

#[unsafe(no_mangle)]
pub extern "C" fn rgb_len() -> u32 {
    with(&DECODER, |one| one.rgb.len() as u32)
}

/// Writes one refusal line as the crate's logs say it, and nothing else.
struct RefusalLines;

/// Each of a line's values after its message.
struct Words<'line>(&'line mut String);

impl<'kvs> VisitSource<'kvs> for Words<'_> {
    fn visit_pair(&mut self, key: Key<'kvs>, value: Value<'kvs>) -> Result<(), Error> {
        write!(self.0, " {key}={value}").map_err(|_| Error::msg("a line's room"))
    }
}

impl log::Log for RefusalLines {
    fn enabled(&self, _metadata: &log::Metadata<'_>) -> bool {
        true
    }

    fn log(&self, record: &log::Record<'_>) {
        let Some(message @ ("smk.refused" | "video.refused")) = record.args().as_str() else {
            return;
        };
        LINES.with_borrow_mut(|lines| {
            lines.push_str(message);
            let _ = record.key_values().visit(&mut Words(lines));
            lines.push('\n');
        });
    }

    fn flush(&self) {}
}

static REFUSAL_LINES: RefusalLines = RefusalLines;

/// Stands the refusal arm's logger up, its room taken first.
#[unsafe(no_mangle)]
pub extern "C" fn logger_on() {
    LINES.with_borrow_mut(|lines| lines.reserve(LINES_ROOM));
    log::set_logger(&REFUSAL_LINES).expect("an instance stands one logger");
    log::set_max_level(log::LevelFilter::Debug);
}

#[unsafe(no_mangle)]
pub extern "C" fn lines_ptr() -> *const u8 {
    LINES.with_borrow(|lines| lines.as_ptr())
}

#[unsafe(no_mangle)]
pub extern "C" fn lines_len() -> u32 {
    LINES.with_borrow(|lines| lines.len() as u32)
}

/// Every frame's sound and picture past [`clip_open`]; the first refusal ends it.
#[unsafe(no_mangle)]
pub extern "C" fn read_rest() -> u32 {
    with(&DECODER, |one| {
        for _ in 0..one.clip.header().frames {
            if let Err(why) = one.clip.frame_samples() {
                return said(why);
            }
            if let Err(why) = one.clip.decode_next_frame(&mut one.picture) {
                return said(why);
            }
        }
        0
    })
}

/// Opens the clip by `smack_open` and names a buffer of the shim's own, copied to the
/// easel's screen each frame, so no pointer outlives a lend.
#[unsafe(no_mangle)]
pub extern "C" fn screen_open() -> u32 {
    let mut videos = videos_over_incoming();
    let handle = videos.smack_open(b"clip", 0, 0);
    if handle.is_null() {
        return 1;
    }
    // SAFETY: a handle `smack_open` answered stands until `smack_close`, which no export calls.
    let (clip_width, clip_height) = unsafe { ((*handle).width, (*handle).height) };
    let width = clip_width + WIDTH_PAST_CLIP;
    let height = clip_height + CLIP_Y;
    let pitch = (width + 3) & !3;
    let mut pixels = vec![0_u8; (pitch * height) as usize];
    let place = Place {
        x: CLIP_X,
        y: CLIP_Y,
        pitch,
        height,
    };
    // SAFETY: `pixels` holds `pitch * height` bytes and moves into the `Screen` with its
    // heap in place, and that `Screen` is never dropped while the clip stands.
    unsafe { videos.smack_to_buffer(handle, place, pixels.as_mut_ptr(), 0) };
    let page: &'static Page = Box::leak(Box::new(Page::new()));
    let easel: &'static Easel<&'static Page> = Box::leak(Box::new(Easel::new(page, width, height)));
    let palette = easel.create_palette(&[0; 256]);
    let screen = Screen {
        videos,
        handle,
        page,
        easel,
        palette,
        pixels,
        rgba: vec![0; (width * height * 4) as usize],
        width,
        height,
        pitch,
        read_pitch: easel.with_screen(|buffer| buffer.pitch),
    };
    SCREEN.set(Some(screen));
    0
}

/// One frame to the page: paint, copy in, the palette, present, one read, step on. 1 for
/// a read that held a new picture.
#[unsafe(no_mangle)]
pub extern "C" fn screen_frame() -> u32 {
    with(&SCREEN, |one| {
        one.videos.smack_do_frame(one.handle);
        one.easel
            .with_screen(|buffer| buffer.pixels.copy_from_slice(&one.pixels));
        // SAFETY: as in `screen_open`: the handle stands, and nothing writes it during this read.
        let entries = unsafe { &(*one.handle).palette };
        let words: Vec<u32> = entries
            .chunks_exact(3)
            .map(|entry| u32::from_le_bytes([entry[0], entry[1], entry[2], 0]))
            .collect();
        one.easel.set_palette_entries(one.palette, 0, &words);
        one.easel.show_palette(one.palette);
        one.easel.present();
        let read = one.page.read(&mut one.rgba);
        one.videos.smack_next_frame(one.handle);
        u32::from(read)
    })
}

#[unsafe(no_mangle)]
pub extern "C" fn screen_frames() -> u32 {
    // SAFETY: as in `screen_open`.
    with(&SCREEN, |one| unsafe { (*one.handle).frames })
}

#[unsafe(no_mangle)]
pub extern "C" fn screen_clip_width() -> u32 {
    // SAFETY: as in `screen_open`.
    with(&SCREEN, |one| unsafe { (*one.handle).width })
}

#[unsafe(no_mangle)]
pub extern "C" fn screen_clip_height() -> u32 {
    // SAFETY: as in `screen_open`.
    with(&SCREEN, |one| unsafe { (*one.handle).height })
}

#[unsafe(no_mangle)]
pub extern "C" fn screen_width() -> u32 {
    with(&SCREEN, |one| one.width)
}

#[unsafe(no_mangle)]
pub extern "C" fn screen_height() -> u32 {
    with(&SCREEN, |one| one.height)
}

#[unsafe(no_mangle)]
pub extern "C" fn screen_pitch_mine() -> u32 {
    with(&SCREEN, |one| one.pitch)
}

/// `Buffer::pitch` of the easel's screen.
#[unsafe(no_mangle)]
pub extern "C" fn screen_pitch_read() -> u32 {
    with(&SCREEN, |one| one.read_pitch)
}

#[unsafe(no_mangle)]
pub extern "C" fn screen_clip_x() -> u32 {
    CLIP_X
}

#[unsafe(no_mangle)]
pub extern "C" fn screen_clip_y() -> u32 {
    CLIP_Y
}

#[unsafe(no_mangle)]
pub extern "C" fn screen_rgba_ptr() -> *const u8 {
    with(&SCREEN, |one| one.rgba.as_ptr())
}

#[unsafe(no_mangle)]
pub extern "C" fn screen_rgba_len() -> u32 {
    with(&SCREEN, |one| one.rgba.len() as u32)
}

#[unsafe(no_mangle)]
pub extern "C" fn screen_pixels_ptr() -> *const u8 {
    with(&SCREEN, |one| one.pixels.as_ptr())
}

#[unsafe(no_mangle)]
pub extern "C" fn screen_pixels_len() -> u32 {
    with(&SCREEN, |one| one.pixels.len() as u32)
}

/// Opens the clip for its pace: its instant is the clock's read inside `smack_open`.
#[unsafe(no_mangle)]
pub extern "C" fn pace_open() -> u32 {
    let period_us = match INCOMING.with_borrow(|bytes| Clip::parse(bytes)) {
        Ok(clip) => u64::from(clip.header().frame_us),
        Err(why) => return said(why),
    };
    let mut videos = videos_over_incoming();
    let handle = videos.smack_open(b"clip", 0, 0);
    if handle.is_null() {
        return 1;
    }
    let open_us = videos.clock_mut().last_us.get();
    let pace = Pace {
        videos,
        handle,
        period_us,
        anchor_us: open_us,
        anchor_frame: 0,
        open_us,
        cursor: 0,
        not_due: 0,
        waits: 0,
        reanchors: 0,
        due_us: open_us,
        late_us: 0,
    };
    PACE.set(Some(pace));
    0
}

/// `smack_wait` once, its due time and re-anchor worked beside it from the same read.
#[unsafe(no_mangle)]
pub extern "C" fn pace_wait() -> u32 {
    with(&PACE, |one| {
        let answer = one.videos.smack_wait(one.handle);
        one.waits += 1;
        let now_us = one.videos.clock_mut().last_us.get();
        one.due_us = one.anchor_us + u64::from(one.cursor - one.anchor_frame) * one.period_us;
        if answer == DUE {
            one.late_us = now_us.saturating_sub(one.due_us);
            if one.late_us > one.period_us {
                one.reanchors += 1;
                one.anchor_us = now_us;
                one.anchor_frame = one.cursor;
            }
        } else {
            one.not_due += 1;
        }
        answer
    })
}

/// Paints the due frame and steps on.
#[unsafe(no_mangle)]
pub extern "C" fn pace_step() {
    with(&PACE, |one| {
        one.videos.smack_do_frame(one.handle);
        one.videos.smack_next_frame(one.handle);
        one.cursor += 1;
        one.not_due = 0;
    });
}

#[unsafe(no_mangle)]
pub extern "C" fn pace_frames() -> u32 {
    // SAFETY: as in `screen_open`.
    with(&PACE, |one| unsafe { (*one.handle).frames })
}

#[unsafe(no_mangle)]
pub extern "C" fn pace_open_us() -> f64 {
    with(&PACE, |one| one.open_us as f64)
}

#[unsafe(no_mangle)]
pub extern "C" fn pace_due_us() -> f64 {
    with(&PACE, |one| one.due_us as f64)
}

#[unsafe(no_mangle)]
pub extern "C" fn pace_late_us() -> f64 {
    with(&PACE, |one| one.late_us as f64)
}

#[unsafe(no_mangle)]
pub extern "C" fn pace_not_due() -> u32 {
    with(&PACE, |one| one.not_due)
}

#[unsafe(no_mangle)]
pub extern "C" fn pace_waits() -> f64 {
    with(&PACE, |one| one.waits as f64)
}

#[unsafe(no_mangle)]
pub extern "C" fn pace_reanchors() -> u32 {
    with(&PACE, |one| one.reanchors)
}

#[unsafe(no_mangle)]
pub extern "C" fn pace_clock_reads() -> f64 {
    with(&PACE, |one| one.videos.clock_mut().reads.get() as f64)
}
