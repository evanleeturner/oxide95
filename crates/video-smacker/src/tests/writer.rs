// SPDX-License-Identifier: MIT
//! A test-only writer of Smacker clips, the reader's inverse: balanced trees,
//! the three chunks and the header. It holds no judgement about what a clip
//! should say, so a test can write a clip the reader must refuse.
use std::collections::{BTreeMap, BTreeSet};

/// A block's kind, in a kind word's low 2 bits.
pub(super) const MONO: u16 = 0;
pub(super) const FULL: u16 = 1;
pub(super) const VOID: u16 = 2;
pub(super) const SOLID: u16 = 3;

/// The header word holding the trees' stored length.
const TREES_SIZE_WORD: usize = 13;
/// The first of the four words a reader may size its trees by; this writer's
/// trees all fit the same generous size.
const TREE_ROOM_WORD: usize = 14;
const TREE_ROOM: u32 = 0x1000;
/// The header word of track 0's form.
pub(super) const TRACK_WORD: usize = 18;
/// The header word of track 0's largest unpacked sound.
pub(super) const LARGEST_WORD: usize = 6;
pub(super) const HEADER_BYTES: usize = 104;

/// A kind word: the kind, a 6-bit chain code, the byte a solid block fills with.
pub(super) const fn kind(block: u16, chain: u16, extra: u16) -> u16 {
    block | (chain << 2) | (extra << 8)
}

/// The format's bit stream going out: lowest bit of a byte first.
#[derive(Default)]
pub(super) struct BitWriter {
    bytes: Vec<u8>,
    used: u8,
}

impl BitWriter {
    pub(super) fn bit(&mut self, set: bool) {
        if self.used == 0 {
            self.bytes.push(0);
        }
        if set {
            *self.bytes.last_mut().expect("a byte was just pushed") |= 1 << self.used;
        }
        self.used = (self.used + 1) % 8;
    }

    pub(super) fn byte(&mut self, value: u8) {
        for place in 0..8 {
            self.bit((value >> place) & 1 == 1);
        }
    }

    pub(super) fn word(&mut self, value: u16) {
        for byte in value.to_le_bytes() {
            self.byte(byte);
        }
    }

    pub(super) fn path(&mut self, path: &[bool]) {
        for &step in path {
            self.bit(step);
        }
    }
}

/// Each value's way down a tree.
type Paths<T> = BTreeMap<T, Vec<bool>>;

/// Writes a balanced shape over `values` and gathers each value's path.
fn shape<T: Copy + Ord>(
    out: &mut BitWriter,
    values: &[T],
    path: &mut Vec<bool>,
    paths: &mut Paths<T>,
    leaf: &mut impl FnMut(&mut BitWriter, T),
) {
    if let [value] = values {
        out.bit(false);
        leaf(out, *value);
        paths.insert(*value, path.clone());
        return;
    }
    out.bit(true);
    let (zero, one) = values.split_at(values.len() / 2);
    path.push(false);
    shape(out, zero, path, paths, leaf);
    path.pop();
    path.push(true);
    shape(out, one, path, paths, leaf);
    path.pop();
}

/// Writes a byte tree over `values`; none makes an absent tree.
fn byte_tree(out: &mut BitWriter, values: &BTreeSet<u8>) -> Paths<u8> {
    let mut paths = Paths::new();
    if values.is_empty() {
        out.bit(false);
        return paths;
    }
    out.bit(true);
    let values: Vec<u8> = values.iter().copied().collect();
    shape(
        out,
        &values,
        &mut Vec::new(),
        &mut paths,
        &mut BitWriter::byte,
    );
    out.bit(false);
    paths
}

/// A word tree's values and its three recent-value markers, which become
/// leaves of their own.
pub(super) type WordSpec<'a> = Option<(&'a [u16], [u16; 3])>;

/// Writes a word tree, or an absent one, and gives back each value's path.
fn word_tree(out: &mut BitWriter, spec: WordSpec<'_>) -> Paths<u16> {
    let mut paths = Paths::new();
    let Some((values, markers)) = spec else {
        out.bit(false);
        return paths;
    };
    out.bit(true);
    let leaves: BTreeSet<u16> = values.iter().chain(&markers).copied().collect();
    let low = byte_tree(
        out,
        &leaves.iter().map(|value| value.to_le_bytes()[0]).collect(),
    );
    let high = byte_tree(
        out,
        &leaves.iter().map(|value| value.to_le_bytes()[1]).collect(),
    );
    for marker in markers {
        out.word(marker);
    }
    let leaves: Vec<u16> = leaves.into_iter().collect();
    shape(
        out,
        &leaves,
        &mut Vec::new(),
        &mut paths,
        &mut |out, value| {
            let [low_byte, high_byte] = value.to_le_bytes();
            out.path(&low[&low_byte]);
            out.path(&high[&high_byte]);
        },
    );
    out.bit(false);
    paths
}

/// The four stored trees and how to write a value through each.
pub(super) struct Trees {
    pub(super) bytes: Vec<u8>,
    map: Paths<u16>,
    colours: Paths<u16>,
    full: Paths<u16>,
    kinds: Paths<u16>,
}

impl Trees {
    pub(super) fn new(
        map: WordSpec<'_>,
        colours: WordSpec<'_>,
        full: WordSpec<'_>,
        kinds: WordSpec<'_>,
    ) -> Self {
        let mut out = BitWriter::default();
        let map = word_tree(&mut out, map);
        let colours = word_tree(&mut out, colours);
        let full = word_tree(&mut out, full);
        let kinds = word_tree(&mut out, kinds);
        Self {
            bytes: out.bytes,
            map,
            colours,
            full,
            kinds,
        }
    }

    pub(super) fn video(&self) -> Video<'_> {
        Video {
            trees: self,
            out: BitWriter::default(),
        }
    }
}

/// A frame's video chunk, written block by block.
pub(super) struct Video<'a> {
    trees: &'a Trees,
    out: BitWriter,
}

impl Video<'_> {
    pub(super) fn kind(mut self, word: u16) -> Self {
        self.out.path(&self.trees.kinds[&word]);
        self
    }

    pub(super) fn colours(mut self, value: u16) -> Self {
        self.out.path(&self.trees.colours[&value]);
        self
    }

    pub(super) fn map(mut self, value: u16) -> Self {
        self.out.path(&self.trees.map[&value]);
        self
    }

    pub(super) fn mono(self, colours: u16, map: u16) -> Self {
        self.colours(colours).map(map)
    }

    pub(super) fn full(mut self, values: [u16; 8]) -> Self {
        for value in values {
            self.out.path(&self.trees.full[&value]);
        }
        self
    }

    pub(super) fn bytes(self) -> Vec<u8> {
        self.out.bytes
    }
}

/// A palette chunk: its length byte, its runs, and 0s up to a multiple of 4.
pub(super) fn palette_chunk(runs: &[u8]) -> Vec<u8> {
    let mut chunk = vec![0];
    chunk.extend_from_slice(runs);
    while chunk.len() % 4 != 0 {
        chunk.push(0);
    }
    chunk[0] = u8::try_from(chunk.len() / 4).expect("a palette chunk fits its length byte");
    chunk
}

/// A sound chunk of whole 16-bit samples, interleaved left then right.
pub(super) fn sound_chunk(channels: usize, samples: &[i16]) -> Vec<u8> {
    let track: Vec<Vec<i16>> = (0..channels)
        .map(|channel| {
            samples
                .iter()
                .skip(channel)
                .step_by(channels)
                .copied()
                .collect()
        })
        .collect();
    let deltas: Vec<Vec<i16>> = track
        .iter()
        .map(|one| {
            one.windows(2)
                .map(|pair| pair[1].wrapping_sub(pair[0]))
                .collect()
        })
        .collect();
    let mut out = BitWriter::default();
    out.bit(true);
    out.bit(channels == 2);
    out.bit(true);
    let mut paths = Vec::new();
    for one in &deltas {
        let low = byte_tree(
            &mut out,
            &one.iter().map(|delta| delta.to_le_bytes()[0]).collect(),
        );
        let high = byte_tree(
            &mut out,
            &one.iter().map(|delta| delta.to_le_bytes()[1]).collect(),
        );
        paths.push((low, high));
    }
    for one in track.iter().rev() {
        let [low, high] = one[0].to_le_bytes();
        out.byte(high);
        out.byte(low);
    }
    for at in 0..deltas[0].len() {
        for (one, (low, high)) in deltas.iter().zip(&paths) {
            let [low_byte, high_byte] = one[at].to_le_bytes();
            out.path(&low[&low_byte]);
            out.path(&high[&high_byte]);
        }
    }
    let unpacked = u32::try_from(samples.len() * 2).expect("a test's sound fits");
    sound_from_bits(unpacked, &out.bytes)
}

/// A sound chunk from its unpacked length and its already packed bits.
pub(super) fn sound_from_bits(unpacked: u32, bits: &[u8]) -> Vec<u8> {
    let whole = u32::try_from(bits.len() + 8).expect("a test's sound fits");
    let mut chunk = Vec::new();
    chunk.extend_from_slice(&whole.to_le_bytes());
    chunk.extend_from_slice(&unpacked.to_le_bytes());
    chunk.extend_from_slice(bits);
    chunk
}

/// One frame's three chunks; the clip pads it to a multiple of 4.
#[derive(Default)]
pub(super) struct Frame {
    pub(super) palette: Option<Vec<u8>>,
    pub(super) sound: Option<Vec<u8>>,
    pub(super) video: Vec<u8>,
}

/// Writes a `u32` into a clip's header or tables.
pub(super) fn set_word(bytes: &mut [u8], word: usize, value: u32) {
    bytes[word * 4..word * 4 + 4].copy_from_slice(&value.to_le_bytes());
}

/// A whole clip's bytes; `track` is track 0's form word and largest sound.
pub(super) fn clip(
    width: u32,
    height: u32,
    trees: &Trees,
    frames: &[Frame],
    track: Option<(u32, u32)>,
) -> Vec<u8> {
    let mut bytes = vec![0; HEADER_BYTES];
    bytes[..4].copy_from_slice(b"SMK2");
    set_word(&mut bytes, 1, width);
    set_word(&mut bytes, 2, height);
    set_word(
        &mut bytes,
        3,
        u32::try_from(frames.len()).expect("a test's frames fit"),
    );
    set_word(&mut bytes, 4, (-1000i32).cast_unsigned());
    set_word(
        &mut bytes,
        TREES_SIZE_WORD,
        u32::try_from(trees.bytes.len()).expect("a test's trees fit"),
    );
    for word in TREE_ROOM_WORD..TREE_ROOM_WORD + 4 {
        set_word(&mut bytes, word, TREE_ROOM);
    }
    if let Some((form, largest)) = track {
        set_word(&mut bytes, TRACK_WORD, form);
        set_word(&mut bytes, LARGEST_WORD, largest);
    }
    let stored: Vec<Vec<u8>> = frames
        .iter()
        .map(|frame| {
            let mut one = frame.palette.clone().unwrap_or_default();
            one.extend_from_slice(frame.sound.as_deref().unwrap_or_default());
            one.extend_from_slice(&frame.video);
            while one.len() % 4 != 0 {
                one.push(0);
            }
            one
        })
        .collect();
    for one in &stored {
        let size = u32::try_from(one.len()).expect("a test's frame fits");
        bytes.extend_from_slice(&size.to_le_bytes());
    }
    for frame in frames {
        bytes.push(u8::from(frame.palette.is_some()) | (u8::from(frame.sound.is_some()) << 1));
    }
    bytes.extend_from_slice(&trees.bytes);
    for one in &stored {
        bytes.extend_from_slice(one);
    }
    bytes
}
