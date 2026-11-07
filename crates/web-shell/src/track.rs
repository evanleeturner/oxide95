// SPDX-License-Identifier: MIT
//! One turn of the track's loop, on the layer's own crates: the crossing's edges
//! go to `window`'s page back, `window_input`'s front answers the key, `draw`'s
//! front lays the picture down, and its page back gives the RGBA the host shows.
//!
//! The target's part is [`Host`]; everything else here runs on a host test as it
//! runs on the page. The turn never waits: its caller sits between turns.

use core::ffi::CStr;

use draw::easel::Easel;
use draw::front::{Front as DrawFront, Header};
use draw::page::Page as DrawPage;
use draw::table::{Calls as DrawCalls, Route, Row as DrawRow};
use kernel_files::{Create, Error, Folder, Mode, Origin};
use logical_disc::{Disc, OpenError, ReadAt, Source};
use window::desk::{Desk, Window};
use window::page::Page;
use window::{Event, Rect};
use window_input::Input;
use window_input::front::Front as InputFront;
use window_input::table::{Calls as InputCalls, Row as InputRow};

use crate::crossing::{
    DISC, DISC_MOVED, DISC_READ, DISC_REFUSED, EDGES, FRAMES, LAST_KEY, MARK_LEFT, MARKER_AT,
    POLLS, POLLS_DOWN, READ_AGAIN, STILL, Words, next_edge, signed,
};
use crate::mark::Mark;

/// The screen's size in pixels.
pub const WIDTH: u32 = 640;
/// The screen's size in pixels.
pub const HEIGHT: u32 = 480;
/// The time between pictures, at most one a turn.
const FRAME_MS: u32 = 16;
/// The canvas's window, the only one on the desk.
const WINDOW: u32 = 1;
/// `VK_UP`: the key the turn polls, as a program's flight loop polls one.
pub const HELD_KEY: u8 = 0x26;
/// `GetAsyncKeyState`'s bit for a key that is down.
const DOWN: u32 = 0x8000;
/// `GROUND` a dark blue, `SQUARE` white, as `PALETTEENTRY` holds them: red in the low byte.
const COLOURS: [u32; 2] = [0x0050_0000, 0x00ff_ffff];
/// `BITMAPINFOHEADER`: 40 bytes, top-down, 8 bits a pixel, no compression.
const PICTURE: Header = Header([
    40,
    WIDTH,
    HEIGHT.wrapping_neg(),
    1 | (8 << 16),
    0,
    0,
    0,
    0,
    0,
    0,
]);

/// The one file a test disc holds, and the byte offsets of its markers within it.
pub const MARKER_FILE: &CStr = c"MARKERS.BIN";
/// Whole MiB apart across a 300 MiB file, the last one near its end.
pub const MARKER_PLACES: [i32; 5] = [0, 1 << 20, 100 << 20, 200 << 20, (300 << 20) - (1 << 20)];

static INPUT: InputRow = InputRow {
    file: "track",
    calls: &InputCalls {
        get_cursor_pos: "GetCursorPos",
        set_cursor_pos: "SetCursorPos",
        show_cursor: "ShowCursor",
        load_cursor: "LoadCursorA",
        clip_cursor: "ClipCursor",
        get_async_key_state: "GetAsyncKeyState",
        get_key_state: "GetKeyState",
        map_virtual_key: "MapVirtualKeyA",
    },
};

static DRAW: DrawRow = DrawRow {
    file: "track",
    calls: &DrawCalls {
        create_palette: "CreatePalette",
        select_palette: "SelectPalette",
        set_palette_entries: "SetPaletteEntries",
        realize_palette: "RealizePalette",
        set_dib_color_table: "SetDIBColorTable",
        get_system_palette_entries: "GetSystemPaletteEntries",
        create_dib_section: "CreateDIBSection",
        bit_blt: "BitBlt",
        set_dib_bits_to_device: "SetDIBitsToDevice",
        stretch_dib_bits: "StretchDIBits",
        stretch_blt: "StretchBlt",
        create_compatible_dc: "CreateCompatibleDC",
        delete_dc: "DeleteDC",
        delete_object: "DeleteObject",
        select_object: "SelectObject",
        get_device_caps: "GetDeviceCaps",
        get_stock_object: "GetStockObject",
        get_dc: "GetDC",
        release_dc: "ReleaseDC",
        update_window: "UpdateWindow",
        begin_paint: "BeginPaint",
        end_paint: "EndPaint",
        validate_rect: "ValidateRect",
    },
    caps: &[],
    route: Route::Bits,
};

/// What the track asks of its target, past the shared words.
pub trait Host: Words {
    /// Shows `rgba`, 4 bytes a pixel, `width` by `height`.
    fn present(&self, rgba: &[u8], width: u32, height: u32);
}

/// The track's whole state, on the Worker's one thread.
pub struct Track<H, R> {
    host: H,
    desk: Desk<Input<Page>>,
    easel: Easel<DrawPage>,
    context: u32,
    mark: Mark,
    bits: Vec<u8>,
    rgba: Vec<u8>,
    next_frame: u32,
    /// The picked file's bytes, cloned for each read, as `Disc::open` takes its source.
    file: Option<R>,
}

impl<H: Host, R: ReadAt + Clone> Track<H, R> {
    /// The track, its picture not yet drawn. `file` is what the player picked, if anything.
    pub fn new(host: H, file: Option<R>) -> Self {
        let width = signed(WIDTH);
        let height = signed(HEIGHT);
        let desk = Desk::new(Input::new(Page::new(width, height)));
        desk.state().windows.push(Window {
            handle: WINDOW,
            class: Vec::new(),
            procedure: 0,
            rect: Rect {
                left: 0,
                top: 0,
                right: width,
                bottom: height,
            },
            style: 0,
            ex_style: 0,
            user: 0,
            longs: Vec::new(),
            title: Vec::new(),
            parent: 0,
            shown: true,
        });
        let easel = Easel::new(DrawPage::new(), WIDTH, HEIGHT);
        let palette = easel.create_palette(&COLOURS);
        easel.show_palette(palette);
        let context = DrawFront::new(&DRAW, &easel, &desk).get_dc(WINDOW);
        Self {
            host,
            desk,
            easel,
            context,
            mark: Mark::new(WIDTH, HEIGHT),
            bits: vec![0; (WIDTH * HEIGHT) as usize],
            rgba: vec![0; (WIDTH * HEIGHT * 4) as usize],
            next_frame: 0,
            file,
        }
    }

    /// One turn at `millis` since the page began.
    pub fn turn(&mut self, millis: u32) {
        self.hand_edges(millis);
        let front = InputFront::new(&INPUT, &self.desk);
        let held = front.get_async_key_state(u32::from(HELD_KEY)) & DOWN != 0;
        self.host.count(POLLS);
        if held {
            self.host.count(POLLS_DOWN);
        }
        if self.host.load(READ_AGAIN) != 0 {
            self.read_file();
            self.host.store(READ_AGAIN, 0);
        }
        if millis >= self.next_frame {
            self.next_frame = millis.saturating_add(FRAME_MS);
            self.picture(held);
        }
    }

    /// Every waiting edge to the page back, then what the front now answers of the last.
    fn hand_edges(&self, millis: u32) {
        let back = self.desk.screen().back();
        back.frame(millis);
        let mut last = None;
        while let Some(edge) = next_edge(&self.host) {
            back.hand(Event::Key {
                window: WINDOW,
                key: edge.key,
                down: edge.down,
            });
            self.host.count(EDGES);
            last = Some(edge.key);
        }
        if let Some(key) = last {
            let answer = InputFront::new(&INPUT, &self.desk).get_async_key_state(u32::from(key));
            let down = i32::from(answer & DOWN != 0);
            self.host.store(LAST_KEY, i32::from(key) | (down << 8));
        }
        // No program reads the desk's queue on the track, so it would only grow.
        self.desk.pump();
        self.desk.state().queue.clear();
    }

    /// Moves the mark and hands the new picture to the host.
    fn picture(&mut self, held: bool) {
        self.mark.step(held, self.host.load(STILL) == 0);
        self.mark.paint(&mut self.bits);
        let front = DrawFront::new(&DRAW, &self.easel, &self.desk);
        let drawn = front.set_dib_bits_to_device(
            self.context,
            0,
            0,
            WIDTH,
            HEIGHT,
            0,
            0,
            0,
            HEIGHT,
            0,
            PICTURE,
            &self.bits,
        );
        if drawn == 0 || !self.easel.back().read(&mut self.rgba) {
            return;
        }
        self.host.store(MARK_LEFT, signed(self.mark.left()));
        self.host.present(&self.rgba, WIDTH, HEIGHT);
        self.host.count(FRAMES);
    }

    /// The markers of the picked file, into the crossing, or why not, into `DISC`.
    fn read_file(&self) {
        let Some(file) = self.file.clone() else {
            return;
        };
        // `Disc::open` answers a failed read of an iso as a bad volume, which would tell
        // the player a moved file is not a disc, so one byte is tried first.
        if file.size() > 0 && file.read_at(0, &mut [0]).is_err() {
            self.host.store(DISC, DISC_MOVED);
            return;
        }
        let status = match Disc::open(Source::Iso(file)) {
            Ok(disc) => match read_markers(&disc) {
                Ok(markers) => {
                    for (word, byte) in (MARKER_AT..).zip(markers) {
                        self.host.store(word, i32::from(byte));
                    }
                    log::debug!(markers:? = markers; "track.markers");
                    DISC_READ
                }
                Err(_) => DISC_MOVED,
            },
            Err(OpenError::Read(_)) => DISC_MOVED,
            Err(_) => DISC_REFUSED,
        };
        self.host.store(DISC, status);
    }
}

/// The byte at each of [`MARKER_PLACES`] of [`MARKER_FILE`].
fn read_markers(folder: &impl Folder) -> Result<[u8; 5], Error> {
    let file = folder.open(
        MARKER_FILE,
        Mode {
            read: true,
            write: false,
            create: Create::Existing,
        },
    )?;
    let mut markers = [0; 5];
    for (marker, place) in markers.iter_mut().zip(MARKER_PLACES) {
        folder.seek(file, place, Origin::Start)?;
        let mut byte = [0];
        folder.read(file, &mut byte)?;
        *marker = byte[0];
    }
    folder.close(file)?;
    Ok(markers)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crossing::tests::Plain;
    use crate::crossing::{DISC_NONE, Edge, unsigned};
    use crate::mark::SIDE;
    use std::cell::{Cell, RefCell};
    use std::rc::Rc;

    const BLOCK: usize = 2048;
    /// The bytes before the planted iso's one file: the descriptors and the root.
    const HEAD_BYTES: usize = 19 * BLOCK;
    /// Where that file starts.
    const FILE_AT: u64 = HEAD_BYTES as u64;
    const MARKER_BYTES: [u8; 5] = [0xa5, 0x5a, 0x3c, 0xc3, 0x7e];

    /// The words, and the last picture the host was handed.
    struct Shown {
        words: Plain,
        rgba: RefCell<Vec<u8>>,
    }

    impl Shown {
        fn new() -> Self {
            Self {
                words: Plain::new(),
                rgba: RefCell::new(Vec::new()),
            }
        }

        fn pixel(&self, left: u32, top: u32) -> [u8; 4] {
            let at = ((top * WIDTH + left) * 4) as usize;
            let rgba = self.rgba.borrow();
            [rgba[at], rgba[at + 1], rgba[at + 2], rgba[at + 3]]
        }
    }

    impl Words for &Shown {
        fn load(&self, word: u32) -> i32 {
            self.words.load(word)
        }

        fn store(&self, word: u32, value: i32) {
            self.words.store(word, value);
        }
    }

    impl Host for &Shown {
        fn present(&self, rgba: &[u8], width: u32, height: u32) {
            assert_eq!((width, height), (WIDTH, HEIGHT));
            *self.rgba.borrow_mut() = rgba.to_vec();
        }
    }

    /// A directory record, as `logical_disc` reads one.
    fn record(name: &[u8], extent: u32, size: u32, folder: bool) -> Vec<u8> {
        let length = 33 + name.len() + (name.len() + 1) % 2;
        let mut bytes = vec![0; length];
        bytes[0] = u8::try_from(length).expect("a record's length");
        bytes[2..6].copy_from_slice(&extent.to_le_bytes());
        bytes[6..10].copy_from_slice(&extent.to_be_bytes());
        bytes[10..14].copy_from_slice(&size.to_le_bytes());
        bytes[14..18].copy_from_slice(&size.to_be_bytes());
        bytes[25] = if folder { 2 } else { 0 };
        bytes[32] = u8::try_from(name.len()).expect("a name's length");
        bytes[33..33 + name.len()].copy_from_slice(name);
        bytes
    }

    /// The first 19 sectors of an iso holding one file of 300 MiB.
    fn head() -> Vec<u8> {
        let mut image = vec![0; HEAD_BYTES];
        let block = BLOCK;
        let mut put = |sector: usize, at: usize, bytes: &[u8]| {
            let from = sector * block + at;
            image[from..from + bytes.len()].copy_from_slice(bytes);
        };
        let root = record(&[0], 18, 2048, true);
        for (at, bytes) in [
            (0, &[1][..]),
            (1, b"CD001"),
            (6, &[1]),
            (40, &[b' '; 32]),
            (128, &2048u16.to_le_bytes()),
            (130, &2048u16.to_be_bytes()),
            (156, &root),
        ] {
            put(16, at, bytes);
        }
        put(17, 0, &[255]);
        put(17, 1, b"CD001");
        put(17, 6, &[1]);
        let rows = [
            root.clone(),
            record(&[1], 18, 2048, true),
            record(b"MARKERS.BIN;1", 19, 300 << 20, false),
        ];
        put(18, 0, &rows.concat());
        image
    }

    /// A planted file: its head, its markers, zeros elsewhere, and a switch that takes
    /// it away the way moving it on disk does.
    #[derive(Clone)]
    struct Planted {
        head: Rc<Vec<u8>>,
        gone: Rc<Cell<bool>>,
    }

    impl Planted {
        fn new(head: Vec<u8>) -> Self {
            Self {
                head: Rc::new(head),
                gone: Rc::new(Cell::new(false)),
            }
        }
    }

    impl ReadAt for Planted {
        fn size(&self) -> u64 {
            FILE_AT + (300 << 20)
        }

        fn read_at(&self, at: u64, into: &mut [u8]) -> Result<(), Error> {
            if self.gone.get() {
                return Err(Error::Other(1));
            }
            for (place, byte) in (at..).zip(into.iter_mut()) {
                let head = usize::try_from(place).ok().and_then(|i| self.head.get(i));
                let marker = MARKER_PLACES
                    .iter()
                    .zip(MARKER_BYTES)
                    .find(|(p, _)| FILE_AT + u64::from(unsigned(**p)) == place);
                *byte = marker.map_or(head.copied().unwrap_or(0), |(_, value)| value);
            }
            Ok(())
        }
    }

    fn track(shown: &Shown) -> Track<&Shown, Planted> {
        Track::new(shown, None)
    }

    fn press(shown: &Shown, key: u8, down: bool) {
        shown.words.push(Edge { key, down });
    }

    #[test]
    fn a_key_reads_back_at_the_front_as_its_virtual_key() {
        let shown = Shown::new();
        let mut track = track(&shown);
        press(&shown, 0x41, true);
        track.turn(0);
        assert_eq!(shown.words.load(LAST_KEY), 0x141);
        press(&shown, 0x41, false);
        track.turn(1);
        assert_eq!(shown.words.load(LAST_KEY), 0x041);
        assert_eq!(shown.words.load(EDGES), 2);
    }

    #[test]
    fn a_held_key_reads_down_on_every_turn_until_it_is_let_go() {
        let shown = Shown::new();
        let mut track = track(&shown);
        press(&shown, HELD_KEY, true);
        for millis in 0..3 {
            track.turn(millis);
        }
        assert_eq!(shown.words.load(POLLS_DOWN), 3);
        press(&shown, HELD_KEY, false);
        track.turn(3);
        assert_eq!(shown.words.load(POLLS), 4);
        assert_eq!(shown.words.load(POLLS_DOWN), 3);
    }

    #[test]
    fn a_tap_inside_one_turn_hands_both_edges_and_reads_up() {
        let shown = Shown::new();
        let mut track = track(&shown);
        press(&shown, HELD_KEY, true);
        press(&shown, HELD_KEY, false);
        track.turn(0);
        assert_eq!(shown.words.load(EDGES), 2);
        assert_eq!(shown.words.load(POLLS_DOWN), 0);
        assert_eq!(shown.words.load(LAST_KEY), i32::from(HELD_KEY));
    }

    #[test]
    fn a_picture_comes_at_most_every_frame_time() {
        let shown = Shown::new();
        let mut track = track(&shown);
        for (millis, frames) in [(0, 1), (5, 1), (15, 1), (16, 2), (20, 2), (32, 3)] {
            track.turn(millis);
            assert_eq!(shown.words.load(FRAMES), frames, "at {millis} ms");
        }
    }

    #[test]
    fn a_still_mark_holds_while_the_pictures_go_on_and_moves_when_let_go() {
        let shown = Shown::new();
        let mut track = track(&shown);
        shown.words.store(STILL, 1);
        for millis in [0, 16, 32] {
            track.turn(millis);
        }
        assert_eq!(shown.words.load(FRAMES), 3);
        assert_eq!(shown.words.load(MARK_LEFT), 0);
        shown.words.store(STILL, 0);
        track.turn(48);
        assert_eq!(shown.words.load(MARK_LEFT), 4);
    }

    #[test]
    fn the_only_white_in_the_picture_is_the_whole_mark() {
        let shown = Shown::new();
        let mut track = track(&shown);
        track.turn(0);
        let rgba = shown.rgba.borrow();
        let white = rgba
            .chunks_exact(4)
            .filter(|pixel| pixel[..3] == [255, 255, 255])
            .count();
        assert_eq!(white, (SIDE * SIDE) as usize);
    }

    #[test]
    fn the_picture_holds_the_mark_on_the_ground_in_its_colours() {
        let shown = Shown::new();
        let mut track = track(&shown);
        track.turn(0);
        let (left, top) = (4, (HEIGHT - SIDE) / 2);
        assert_eq!(shown.words.load(MARK_LEFT), 4);
        assert_eq!(shown.pixel(left, top), [255, 255, 255, 255]);
        assert_eq!(
            shown.pixel(left + SIDE - 1, top + SIDE - 1),
            [255, 255, 255, 255]
        );
        assert_eq!(shown.pixel(left - 1, top), [0, 0, 0x50, 255]);
        assert_eq!(shown.pixel(0, 0), [0, 0, 0x50, 255]);
    }

    #[test]
    fn the_markers_read_back_from_a_300_mib_file() {
        let shown = Shown::new();
        let mut track = Track::new(&shown, Some(Planted::new(head())));
        shown.words.store(READ_AGAIN, 1);
        track.turn(0);
        assert_eq!(shown.words.load(DISC), DISC_READ);
        assert_eq!(shown.words.load(READ_AGAIN), 0);
        for (word, byte) in (MARKER_AT..).zip(MARKER_BYTES) {
            assert_eq!(shown.words.load(word), i32::from(byte));
        }
    }

    #[test]
    fn a_file_moved_after_a_good_read_says_so_and_a_return_reads_again() {
        let shown = Shown::new();
        let file = Planted::new(head());
        let gone = Rc::clone(&file.gone);
        let mut track = Track::new(&shown, Some(file));
        shown.words.store(READ_AGAIN, 1);
        track.turn(0);
        assert_eq!(shown.words.load(DISC), DISC_READ);
        gone.set(true);
        shown.words.store(READ_AGAIN, 1);
        track.turn(1);
        assert_eq!(shown.words.load(DISC), DISC_MOVED);
        gone.set(false);
        shown.words.store(READ_AGAIN, 1);
        track.turn(2);
        assert_eq!(shown.words.load(DISC), DISC_READ);
    }

    #[test]
    fn a_file_that_is_no_iso_is_refused() {
        let shown = Shown::new();
        let mut track = Track::new(&shown, Some(Planted::new(vec![0; HEAD_BYTES])));
        shown.words.store(READ_AGAIN, 1);
        track.turn(0);
        assert_eq!(shown.words.load(DISC), DISC_REFUSED);
    }

    #[test]
    fn asking_with_no_file_changes_nothing_but_clears_the_ask() {
        let shown = Shown::new();
        let mut track = track(&shown);
        shown.words.store(READ_AGAIN, 1);
        track.turn(0);
        assert_eq!(shown.words.load(DISC), DISC_NONE);
        assert_eq!(shown.words.load(READ_AGAIN), 0);
    }
}
