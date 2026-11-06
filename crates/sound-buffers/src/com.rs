// SPDX-License-Identifier: MIT
//! The shape a program calls: an interface whose first word is its table, the
//! tables in `dsound.h`'s order, each slot `extern "system"`, stdcall on a 32-bit
//! Windows, so a front pops its own arguments as the caller expects.
//!
//! A program reads `*(*object + offset)` and calls it with the object first.
//! Every slot holds a function: a served one its front, the others a stub of the
//! slot's argument count ([`crate::buffers::Buffers::absent`]), never null.
//!
//! Each front reads its pointers, asks the portable part, and writes its outs only
//! on a good answer and only where they are not null. The line is the portable
//! part's, so a front writes none.
//!
//! [`Com::direct_sound_create`] is where a target's exported `DirectSoundCreate`
//! lands. A [`Com`] lives as long as the program, hence its `'static`.
#![expect(
    unsafe_code,
    reason = "the seam a program's raw calls land on: each front reads its pointers"
)]

use alloc::boxed::Box;
use alloc::collections::BTreeMap;
use core::cell::RefCell;
use core::mem::{offset_of, size_of};

use crate::buffers::{Back, Buffers, Shape, WAVE_BYTES};
use crate::sdk::{DS_OK, Desc, WaveFormat};

const OBJECT: u8 = 0;
const BUFFER: u8 = 1;
/// Each interface's SDK name, by its kind.
const NAMES: [&str; 2] = ["IDirectSound", "IDirectSoundBuffer"];

pub(crate) type This<B> = *mut Interface<B>;
/// A slot's function by its arguments, `this` counted.
type Absent1<B> = unsafe extern "system" fn(This<B>) -> u32;
type Absent2<B> = unsafe extern "system" fn(This<B>, u32) -> u32;
type Absent3<B> = unsafe extern "system" fn(This<B>, u32, u32) -> u32;

/// The interfaces handed out, by kind and id; a box's address never moves.
type Made<B> = BTreeMap<(u8, u32), Box<Interface<B>>>;

/// The object or a buffer as a program holds it: the table first.
#[repr(C)]
pub struct Interface<B: 'static> {
    pub(crate) table: *const (),
    com: *const Com<B>,
    pub(crate) id: u32,
}

/// One program's [`Buffers`] and the interfaces handed out for them.
pub struct Com<B: 'static> {
    buffers: Buffers<B>,
    made: RefCell<Made<B>>,
}

impl<B: Back + 'static> Com<B> {
    #[must_use]
    pub fn new(buffers: Buffers<B>) -> Self {
        Self {
            buffers,
            made: RefCell::new(BTreeMap::new()),
        }
    }

    #[must_use]
    pub const fn buffers(&self) -> &Buffers<B> {
        &self.buffers
    }

    /// The one interface of `kind`'s `id`, made at its first asking and kept until
    /// this goes, so a program's stale pointer still lands on a table.
    fn interface(&'static self, kind: u8, id: u32) -> This<B> {
        let table = if kind == OBJECT {
            core::ptr::from_ref(ObjectTable::<B>::STATIC).cast()
        } else {
            core::ptr::from_ref(BufferTable::<B>::STATIC).cast()
        };
        let mut made = self.made.borrow_mut();
        let kept = made.entry((kind, id)).or_insert_with(|| {
            Box::new(Interface {
                table,
                com: self,
                id,
            })
        });
        &raw mut **kept
    }

    /// `DirectSoundCreate`: 0 and `out` the object. `guid` is shown and not read.
    ///
    /// # Safety
    /// `out` is null or points at a word the caller owns.
    pub unsafe fn direct_sound_create(
        &'static self,
        guid: u32,
        out: *mut This<B>,
        outer: u32,
    ) -> u32 {
        let answer = self
            .buffers
            .direct_sound_create(guid, !out.is_null(), outer);
        if answer == DS_OK {
            // SAFETY: a good answer means not null, and the caller's word by this
            // function's contract.
            unsafe { out.write_unaligned(self.interface(OBJECT, 0)) };
        }
        answer
    }
}

/// The program's interface behind `this`: its [`Com`] and its id.
///
/// # Safety
/// `this` came from [`Com::interface`], whose box lives as long as its `'static`
/// [`Com`].
unsafe fn parts<B: 'static>(this: This<B>) -> (&'static Com<B>, u32) {
    // SAFETY: by this function's contract `this` is a live `Interface`, and its
    // `com` is the `'static` that made it.
    unsafe { (&*(*this).com, (*this).id) }
}

/// The answer of a slot with no front.
///
/// # Safety
/// As [`parts`].
unsafe fn absent<B: Back + 'static>(this: This<B>, kind: u8, slot: u32) -> u32 {
    // SAFETY: passed on from this function's contract.
    let (com, _) = unsafe { parts(this) };
    com.buffers.absent(NAMES[kind as usize], slot)
}

/// A stub by its argument count, each popped by `extern "system"`.
macro_rules! stubs {
    ($($name:ident($($arg:ident),*);)*) => {$(
        unsafe extern "system" fn $name<B: Back + 'static, const KIND: u8, const SLOT: u32>(
            this: This<B>,
            $($arg: u32,)*
        ) -> u32 {
            $(let _ = $arg;)*
            // SAFETY: a program calls a slot with the interface it read it from.
            unsafe { absent(this, KIND, SLOT) }
        }
    )*};
}

stubs! {
    absent1();
    absent2(a);
    absent3(a, b);
}

/// A byte offset in a table as `dsound.h` numbers it, a slot being 4 bytes on the
/// 32-bit targets a program's calls come from.
#[expect(
    clippy::cast_possible_truncation,
    reason = "a table's offsets are below 0x100"
)]
pub(crate) const fn sdk_offset(at: usize) -> u32 {
    (at / size_of::<usize>() * 4) as u32
}

/// A stub for a table's named slot, its `SLOT` read off that slot's place.
macro_rules! stub {
    ($stub:ident, $kind:expr, $table:ident, $slot:ident) => {
        $stub::<B, $kind, { sdk_offset(offset_of!($table<()>, $slot)) }>
    };
}

/// `IDirectSound`'s table: `dsound.h`'s 11 in order.
#[repr(C)]
pub struct ObjectTable<B: 'static> {
    pub query_interface: Absent3<B>,
    pub add_ref: Absent1<B>,
    pub release: unsafe extern "system" fn(This<B>) -> u32,
    pub create_sound_buffer:
        unsafe extern "system" fn(This<B>, *const Desc, *mut This<B>, u32) -> u32,
    pub get_caps: Absent2<B>,
    pub duplicate_sound_buffer: unsafe extern "system" fn(This<B>, This<B>, *mut This<B>) -> u32,
    pub set_cooperative_level: unsafe extern "system" fn(This<B>, u32, u32) -> u32,
    pub compact: Absent1<B>,
    pub get_speaker_config: Absent2<B>,
    pub set_speaker_config: Absent2<B>,
    pub initialize: Absent2<B>,
}

impl<B: Back + 'static> ObjectTable<B> {
    /// The table a program reads, promoted to live as long as the program.
    const STATIC: &'static Self = &Self::FRONTS;
    const FRONTS: Self = Self {
        query_interface: stub!(absent3, OBJECT, ObjectTable, query_interface),
        add_ref: stub!(absent1, OBJECT, ObjectTable, add_ref),
        release: object_release::<B>,
        create_sound_buffer: object_create_sound_buffer::<B>,
        get_caps: stub!(absent2, OBJECT, ObjectTable, get_caps),
        duplicate_sound_buffer: object_duplicate_sound_buffer::<B>,
        set_cooperative_level: object_set_cooperative_level::<B>,
        compact: stub!(absent1, OBJECT, ObjectTable, compact),
        get_speaker_config: stub!(absent2, OBJECT, ObjectTable, get_speaker_config),
        set_speaker_config: stub!(absent2, OBJECT, ObjectTable, set_speaker_config),
        initialize: stub!(absent2, OBJECT, ObjectTable, initialize),
    };
}

/// `IDirectSoundBuffer`'s table: `dsound.h`'s 21 in order.
#[repr(C)]
pub struct BufferTable<B: 'static> {
    pub query_interface: Absent3<B>,
    pub add_ref: Absent1<B>,
    pub release: unsafe extern "system" fn(This<B>) -> u32,
    pub get_caps: Absent2<B>,
    pub get_current_position: unsafe extern "system" fn(This<B>, *mut u32, *mut u32) -> u32,
    pub get_format: unsafe extern "system" fn(This<B>, *mut WaveFormat, u32, *mut u32) -> u32,
    pub get_volume: unsafe extern "system" fn(This<B>, *mut i32) -> u32,
    pub get_pan: unsafe extern "system" fn(This<B>, *mut i32) -> u32,
    pub get_frequency: Absent2<B>,
    pub get_status: unsafe extern "system" fn(This<B>, *mut u32) -> u32,
    pub initialize: Absent3<B>,
    pub lock: unsafe extern "system" fn(
        This<B>,
        u32,
        u32,
        *mut u32,
        *mut u32,
        *mut u32,
        *mut u32,
        u32,
    ) -> u32,
    pub play: unsafe extern "system" fn(This<B>, u32, u32, u32) -> u32,
    pub set_current_position: unsafe extern "system" fn(This<B>, u32) -> u32,
    pub set_format: unsafe extern "system" fn(This<B>, *const WaveFormat) -> u32,
    pub set_volume: unsafe extern "system" fn(This<B>, i32) -> u32,
    pub set_pan: unsafe extern "system" fn(This<B>, i32) -> u32,
    pub set_frequency: unsafe extern "system" fn(This<B>, u32) -> u32,
    pub stop: unsafe extern "system" fn(This<B>) -> u32,
    pub unlock: unsafe extern "system" fn(This<B>, u32, u32, u32, u32) -> u32,
    pub restore: unsafe extern "system" fn(This<B>) -> u32,
}

impl<B: Back + 'static> BufferTable<B> {
    /// The table a program reads, promoted to live as long as the program.
    const STATIC: &'static Self = &Self::FRONTS;
    const FRONTS: Self = Self {
        query_interface: stub!(absent3, BUFFER, BufferTable, query_interface),
        add_ref: stub!(absent1, BUFFER, BufferTable, add_ref),
        release: buffer_release::<B>,
        get_caps: stub!(absent2, BUFFER, BufferTable, get_caps),
        get_current_position: buffer_get_current_position::<B>,
        get_format: buffer_get_format::<B>,
        get_volume: buffer_get_volume::<B>,
        get_pan: buffer_get_pan::<B>,
        get_frequency: stub!(absent2, BUFFER, BufferTable, get_frequency),
        get_status: buffer_get_status::<B>,
        initialize: stub!(absent3, BUFFER, BufferTable, initialize),
        lock: buffer_lock::<B>,
        play: buffer_play::<B>,
        set_current_position: buffer_set_current_position::<B>,
        set_format: buffer_set_format::<B>,
        set_volume: buffer_set_volume::<B>,
        set_pan: buffer_set_pan::<B>,
        set_frequency: buffer_set_frequency::<B>,
        stop: buffer_stop::<B>,
        unlock: buffer_unlock::<B>,
        restore: buffer_restore::<B>,
    };
}

/// Writes `value` through `out`, unless it is null.
///
/// # Safety
/// `out` is null or points at a word the caller owns.
unsafe fn put<T>(out: *mut T, value: T) {
    if !out.is_null() {
        // SAFETY: not null, and the caller's by this function's contract.
        unsafe { out.write_unaligned(value) };
    }
}

/// A read's return code, the value written through `out` when it is good.
///
/// # Safety
/// As [`put`].
unsafe fn answered<T>(answer: Result<T, u32>, out: *mut T) -> u32 {
    match answer {
        Ok(value) => {
            // SAFETY: passed on from this function's contract.
            unsafe { put(out, value) };
            DS_OK
        }
        Err(code) => code,
    }
}

/// The format at `wave`, `None` for null.
///
/// # Safety
/// `wave` is null or holds a whole `WAVEFORMATEX`.
unsafe fn wave_at(wave: *const WaveFormat) -> Option<WaveFormat> {
    if wave.is_null() {
        return None;
    }
    // SAFETY: not null and whole by this function's contract; it is packed, so read
    // unaligned.
    Some(unsafe { wave.read_unaligned() })
}

/// What `desc` asks, `None` for null.
///
/// # Safety
/// `desc` is null or holds a description whose format is as [`wave_at`] asks.
unsafe fn shape_at(desc: *const Desc) -> Option<Shape> {
    if desc.is_null() {
        return None;
    }
    // SAFETY: not null and whole by this function's contract.
    let asked = unsafe { desc.read_unaligned() };
    Some(Shape {
        flags: asked.flags,
        bytes: asked.bytes,
        // SAFETY: passed on from this function's contract.
        format: unsafe { wave_at(asked.format) },
    })
}

unsafe extern "system" fn object_release<B: Back + 'static>(this: This<B>) -> u32 {
    // SAFETY: a program calls a slot with the interface it read it from.
    let (com, _) = unsafe { parts(this) };
    com.buffers.object_release()
}

unsafe extern "system" fn object_create_sound_buffer<B: Back + 'static>(
    this: This<B>,
    desc: *const Desc,
    out: *mut This<B>,
    outer: u32,
) -> u32 {
    // SAFETY: a program calls a slot with the interface it read it from.
    let (com, _) = unsafe { parts(this) };
    // SAFETY: a program hands a description of its own, or null.
    let shape = unsafe { shape_at(desc) };
    match com
        .buffers
        .create_sound_buffer(shape, !out.is_null(), outer)
    {
        Ok(id) => {
            // SAFETY: a good answer means not null; the program's own word.
            unsafe { out.write_unaligned(com.interface(BUFFER, id)) };
            DS_OK
        }
        Err(code) => code,
    }
}

unsafe extern "system" fn object_duplicate_sound_buffer<B: Back + 'static>(
    this: This<B>,
    original: This<B>,
    out: *mut This<B>,
) -> u32 {
    // SAFETY: a program calls a slot with the interface it read it from.
    let (com, _) = unsafe { parts(this) };
    // SAFETY: a program hands an interface this crate gave it, or null.
    let id = unsafe { original.as_ref() }.map(|kept| kept.id);
    match com.buffers.duplicate(id, !out.is_null()) {
        Ok(twin) => {
            // SAFETY: a good answer means not null; the program's own word.
            unsafe { out.write_unaligned(com.interface(BUFFER, twin)) };
            DS_OK
        }
        Err(code) => code,
    }
}

unsafe extern "system" fn object_set_cooperative_level<B: Back + 'static>(
    this: This<B>,
    window: u32,
    level: u32,
) -> u32 {
    // SAFETY: a program calls a slot with the interface it read it from.
    let (com, _) = unsafe { parts(this) };
    com.buffers.set_cooperative_level(window, level)
}

unsafe extern "system" fn buffer_release<B: Back + 'static>(this: This<B>) -> u32 {
    // SAFETY: a program calls a slot with the interface it read it from.
    let (com, id) = unsafe { parts(this) };
    com.buffers.buffer_release(id)
}

unsafe extern "system" fn buffer_get_current_position<B: Back + 'static>(
    this: This<B>,
    play_byte: *mut u32,
    write_byte: *mut u32,
) -> u32 {
    // SAFETY: a program calls a slot with the interface it read it from.
    let (com, id) = unsafe { parts(this) };
    match com.buffers.get_current_position(id) {
        Ok((play, write)) => {
            // SAFETY: each is the program's own word or null.
            unsafe {
                put(play_byte, play);
                put(write_byte, write);
            }
            DS_OK
        }
        Err(code) => code,
    }
}

unsafe extern "system" fn buffer_get_format<B: Back + 'static>(
    this: This<B>,
    wave: *mut WaveFormat,
    room: u32,
    written: *mut u32,
) -> u32 {
    // SAFETY: a program calls a slot with the interface it read it from.
    let (com, id) = unsafe { parts(this) };
    match com
        .buffers
        .get_format(id, !wave.is_null(), room, !written.is_null())
    {
        Ok(format) => {
            // SAFETY: each is the program's own or null, and the portable part has
            // judged `room`.
            unsafe {
                put(wave, format);
                put(written, WAVE_BYTES);
            }
            DS_OK
        }
        Err(code) => code,
    }
}

unsafe extern "system" fn buffer_get_volume<B: Back + 'static>(
    this: This<B>,
    out: *mut i32,
) -> u32 {
    // SAFETY: a program calls a slot with the interface it read it from.
    let (com, id) = unsafe { parts(this) };
    // SAFETY: the program's own word or null.
    unsafe { answered(com.buffers.get_volume(id, !out.is_null()), out) }
}

unsafe extern "system" fn buffer_get_pan<B: Back + 'static>(this: This<B>, out: *mut i32) -> u32 {
    // SAFETY: a program calls a slot with the interface it read it from.
    let (com, id) = unsafe { parts(this) };
    // SAFETY: the program's own word or null.
    unsafe { answered(com.buffers.get_pan(id, !out.is_null()), out) }
}

unsafe extern "system" fn buffer_get_status<B: Back + 'static>(
    this: This<B>,
    out: *mut u32,
) -> u32 {
    // SAFETY: a program calls a slot with the interface it read it from.
    let (com, id) = unsafe { parts(this) };
    // SAFETY: the program's own word or null.
    unsafe { answered(com.buffers.get_status(id, !out.is_null()), out) }
}

unsafe extern "system" fn buffer_lock<B: Back + 'static>(
    this: This<B>,
    offset: u32,
    bytes: u32,
    first: *mut u32,
    first_bytes: *mut u32,
    second: *mut u32,
    second_bytes: *mut u32,
    flags: u32,
) -> u32 {
    // SAFETY: a program calls a slot with the interface it read it from.
    let (com, id) = unsafe { parts(this) };
    let has_first = !first.is_null() && !first_bytes.is_null();
    let can_wrap = !second.is_null() && !second_bytes.is_null();
    match com
        .buffers
        .lock(id, offset, bytes, flags, has_first, can_wrap)
    {
        Ok(spans) => {
            let second_word = if spans.second_bytes == 0 {
                0
            } else {
                spans.base
            };
            // SAFETY: each is the program's own word or null, and a good answer
            // means the first two are not null.
            unsafe {
                put(first, spans.base.wrapping_add(spans.first_at));
                put(first_bytes, spans.first_bytes);
                put(second, second_word);
                put(second_bytes, spans.second_bytes);
            }
            DS_OK
        }
        Err(code) => code,
    }
}

unsafe extern "system" fn buffer_play<B: Back + 'static>(
    this: This<B>,
    _reserved: u32,
    _priority: u32,
    flags: u32,
) -> u32 {
    // SAFETY: a program calls a slot with the interface it read it from.
    let (com, id) = unsafe { parts(this) };
    com.buffers.play(id, flags)
}

unsafe extern "system" fn buffer_set_current_position<B: Back + 'static>(
    this: This<B>,
    byte: u32,
) -> u32 {
    // SAFETY: a program calls a slot with the interface it read it from.
    let (com, id) = unsafe { parts(this) };
    com.buffers.set_current_position(id, byte)
}

unsafe extern "system" fn buffer_set_format<B: Back + 'static>(
    this: This<B>,
    wave: *const WaveFormat,
) -> u32 {
    // SAFETY: a program calls a slot with the interface it read it from.
    let (com, id) = unsafe { parts(this) };
    // SAFETY: a program hands a format of its own, or null.
    let asked = unsafe { wave_at(wave) };
    com.buffers.set_format(id, asked)
}

unsafe extern "system" fn buffer_set_volume<B: Back + 'static>(this: This<B>, volume: i32) -> u32 {
    // SAFETY: a program calls a slot with the interface it read it from.
    let (com, id) = unsafe { parts(this) };
    com.buffers.set_volume(id, volume)
}

unsafe extern "system" fn buffer_set_pan<B: Back + 'static>(this: This<B>, pan: i32) -> u32 {
    // SAFETY: a program calls a slot with the interface it read it from.
    let (com, id) = unsafe { parts(this) };
    com.buffers.set_pan(id, pan)
}

unsafe extern "system" fn buffer_set_frequency<B: Back + 'static>(
    this: This<B>,
    rate_hz: u32,
) -> u32 {
    // SAFETY: a program calls a slot with the interface it read it from.
    let (com, id) = unsafe { parts(this) };
    com.buffers.set_frequency(id, rate_hz)
}

unsafe extern "system" fn buffer_stop<B: Back + 'static>(this: This<B>) -> u32 {
    // SAFETY: a program calls a slot with the interface it read it from.
    let (com, id) = unsafe { parts(this) };
    com.buffers.stop(id)
}

unsafe extern "system" fn buffer_unlock<B: Back + 'static>(
    this: This<B>,
    _first: u32,
    first_bytes: u32,
    _second: u32,
    second_bytes: u32,
) -> u32 {
    // SAFETY: a program calls a slot with the interface it read it from.
    let (com, id) = unsafe { parts(this) };
    com.buffers.unlock(id, first_bytes, second_bytes)
}

unsafe extern "system" fn buffer_restore<B: Back + 'static>(this: This<B>) -> u32 {
    // SAFETY: a program calls a slot with the interface it read it from.
    let (com, id) = unsafe { parts(this) };
    com.buffers.restore(id)
}
