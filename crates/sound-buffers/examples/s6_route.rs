// SPDX-License-Identifier: MIT
//! A real 32-bit program's calls into `sound-buffers`, over the real `waveOut` back, one arm
//! a run, for the bench to score. It builds for `i686-pc-windows-gnu`; any other target gets
//! an empty `main`, so the default build opens no stream.
//!
//! Every slot is read as a program reads it, `*(*this + offset)`, and called through a shim
//! that pushes the words, reads the stack after the call and puts it back, so a slot that
//! pops the wrong count is read and survived.
//!
//! Prints one `pop` line a call, one `answer` line a worked answer, and last one `route=`
//! line of counts, and exits 1 on any red. A planted arm is meant to read red; the bench
//! judges that. The plants are the program's own: it copies a table and swaps one slot in
//! its copy, as a wrong table would read.
//!
//! Hazard: the output opens at load through an empty door, and the ring sounds only after
//! stdin carries the line `go`. With no line it waits for ever; the caller owes a timeout.
//!
//! Hazard: `DirectSoundCreate`'s export is not run here, as `Com::direct_sound_create` is
//! called directly. No real `DirectSound` program is compared, and `took` and the counts
//! read the hand-off to the back: they do not show that a person heard anything.
#![cfg_attr(
    all(windows, target_arch = "x86"),
    expect(
        unsafe_code,
        reason = "the bench reads a program's raw tables and calls their slots by hand"
    )
)]

#[cfg(not(all(windows, target_arch = "x86")))]
fn main() {}

#[cfg(all(windows, target_arch = "x86"))]
#[path = "../bench/ring.rs"]
mod ring;

#[cfg(all(windows, target_arch = "x86"))]
fn main() -> std::process::ExitCode {
    route::run()
}

#[cfg(all(windows, target_arch = "x86"))]
mod route {
    use std::arch::asm;
    use std::cell::Cell;
    use std::io::BufRead as _;
    use std::process::ExitCode;
    use std::ptr::{null, with_exposed_provenance, with_exposed_provenance_mut};
    use std::sync::OnceLock;
    use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
    use std::thread::sleep;
    use std::time::{Duration, Instant};

    use log::{Level, LevelFilter, Log, Metadata, Record};
    use sound_buffers::buffers::Buffers;
    use sound_buffers::com::Com;
    use sound_buffers::mix_back::MixBack;
    use sound_buffers::sdk::{
        DS_OK, DSBCAPS_PRIMARYBUFFER, DSBLOCK_ENTIREBUFFER, DSBPLAY_LOOPING, DSERR_INVALIDPARAM,
        DSERR_UNSUPPORTED, Desc, WaveFormat,
    };
    use sound_mix::waveout::WaveOutBack;
    use sound_mix::witness::Witness;
    use sound_mix::{Front, Host};

    use crate::ring::{
        FIRST_SPAN_BYTES, MIX_HZ, Placed, RING_BYTES, WRAP_AT, ring_format, ring_image, worked_hash,
    };

    type Output = Witness<Placed<WaveOutBack>>;
    type Mix = Front<Real, Output>;
    type Ours = Com<MixBack<'static, Real, Output, fn() -> u64>>;

    const REFUSED: &str =
        "The project's `waveOut` output failed where the bench's own call worked.";

    /// Primary 1, ring 2, twin 3: ids count from 1 in order of making.
    const RING_ID: u32 = 2;
    const DESC_BYTES: u32 = 20;
    const FORMAT_BYTES: u32 = 18;
    const WINDOW: u32 = 1;
    const LEVEL: u32 = 2;
    const STATUS_PLAYING_LOOPING: u32 = 5;
    const VOLUME: u32 = 0u32.wrapping_sub(300);
    const BAD_VOLUME: u32 = 1;
    const PAN: u32 = 100;
    const BAD_PAN: u32 = 10_001;
    const FREQUENCY: u32 = 22_050;
    const POSITION: u32 = 8;
    /// 1.2 s of clock, so the 0.5 s ring crosses at least 2 seams.
    const RUN_US: u64 = 1_200_000;
    /// Past the first seam.
    const MID_US: u64 = 600_000;
    const WITNESS_AT_LEAST: u64 = 44_100;
    const PLANT_PUMP_MS: u64 = 10;
    const DRAIN: Duration = Duration::from_secs(2);
    const DRAIN_STEP: Duration = Duration::from_millis(10);
    const BLOCK_FRAMES: u64 = 1_024;
    /// What a slot writes where nothing has: a word no answer holds.
    const UNWRITTEN: u32 = 0xdead_beef;

    static MIX_ROW: sound_mix::table::Row = sound_mix::table::Row {
        file: "a bench",
        module: "MSS32",
    };
    static ROW: sound_buffers::table::Row = sound_buffers::table::Row {
        file: "a bench",
        levels: &[LEVEL],
    };
    static LINES: Lines = Lines;
    static DROPPED: AtomicU64 = AtomicU64::new(0);
    static PASSED_OVER: AtomicU64 = AtomicU64::new(0);
    static START: OnceLock<Instant> = OnceLock::new();
    /// The real `Lock`, for the plant that forwards to it.
    static REAL_LOCK: AtomicU32 = AtomicU32::new(0);

    /// One interface's slots in `dsound.h`'s order, each with its words, `this` counted.
    /// These are the program's; the scorer takes its own from the header.
    struct Table {
        name: &'static str,
        slots: &'static [(&'static str, u32)],
    }

    const OBJECT: Table = Table {
        name: "IDirectSound",
        slots: &[
            ("QueryInterface", 3),
            ("AddRef", 1),
            ("Release", 1),
            ("CreateSoundBuffer", 4),
            ("GetCaps", 2),
            ("DuplicateSoundBuffer", 3),
            ("SetCooperativeLevel", 3),
            ("Compact", 1),
            ("GetSpeakerConfig", 2),
            ("SetSpeakerConfig", 2),
            ("Initialize", 2),
        ],
    };

    const BUFFER: Table = Table {
        name: "IDirectSoundBuffer",
        slots: &[
            ("QueryInterface", 3),
            ("AddRef", 1),
            ("Release", 1),
            ("GetCaps", 2),
            ("GetCurrentPosition", 3),
            ("GetFormat", 4),
            ("GetVolume", 2),
            ("GetPan", 2),
            ("GetFrequency", 2),
            ("GetStatus", 2),
            ("Initialize", 3),
            ("Lock", 8),
            ("Play", 4),
            ("SetCurrentPosition", 2),
            ("SetFormat", 2),
            ("SetVolume", 2),
            ("SetPan", 2),
            ("SetFrequency", 2),
            ("Stop", 1),
            ("Unlock", 5),
            ("Restore", 1),
        ],
    };

    /// One run's pump, or its wrong table or wrong sample.
    #[derive(Clone, Copy, PartialEq, Eq)]
    enum Arm {
        Clean(u64),
        PopShort,
        PopLong,
        Wrap,
        Flip,
    }

    fn arm_of(word: &str) -> Option<Arm> {
        Some(match word {
            "pop-short" => Arm::PopShort,
            "pop-long" => Arm::PopLong,
            "wrap" => Arm::Wrap,
            "flip" => Arm::Flip,
            _ => Arm::Clean(word.strip_prefix("clean:")?.parse().ok()?),
        })
    }

    /// Prints each line, and sums what the mix dropped and the buffers passed over.
    struct Lines;

    impl Log for Lines {
        fn enabled(&self, metadata: &Metadata<'_>) -> bool {
            metadata.level() <= Level::Debug
        }

        fn log(&self, record: &Record<'_>) {
            if !self.enabled(record.metadata()) {
                return;
            }
            let event = record.args().to_string();
            if event == "sound.frames.dropped" {
                let frames = record.key_values().get("frames".into());
                let frames = frames.and_then(|value| parsed_count(&value.to_string()));
                DROPPED.fetch_add(frames.unwrap_or(0), Ordering::Relaxed);
            } else if event == "sound.buffers.passed.over" {
                // `a` is the id, then the frames passed over.
                let numbers = record.key_values().get("a".into());
                let frames = numbers.and_then(|value| {
                    let text = value.to_string();
                    parsed_count(text.split(',').nth(1)?)
                });
                PASSED_OVER.fetch_add(frames.unwrap_or(0), Ordering::Relaxed);
            }
            eprintln!("{}", oxide_logline::body(record));
        }

        fn flush(&self) {}
    }

    /// `oxide_trace::__shown!` writes a count in hex, with no prefix.
    fn parsed_count(text: &str) -> Option<u64> {
        u64::from_str_radix(text, 16).ok()
    }

    #[allow(
        clippy::cast_possible_truncation,
        reason = "584,000 years of microseconds"
    )]
    fn micros(span: Duration) -> u64 {
        span.as_micros() as u64
    }

    /// The one clock, read by the mix and by the buffers.
    fn now_us() -> u64 {
        micros(START.get_or_init(Instant::now).elapsed())
    }

    struct Real;

    impl Host for Real {
        fn now_us(&self) -> u64 {
            now_us()
        }

        fn call_back(&self, _callback: u32, _handle: u32) {}

        fn read_file(&self, _name: &str) -> Option<u32> {
            None
        }

        fn free_block(&self, _block: u32) {}
    }

    /// A pointer as the 32-bit word a program passes. The address is exposed, so a call
    /// that writes through the word is one the compiler must allow for.
    #[allow(
        clippy::cast_possible_truncation,
        reason = "this module builds for 32-bit x86 alone"
    )]
    fn word<T>(pointer: *const T) -> u32 {
        pointer.expose_provenance() as u32
    }

    /// The word at `address`.
    ///
    /// # Safety
    /// `address` holds a live word.
    unsafe fn read_word(address: u32) -> u32 {
        // SAFETY: by this function's contract.
        unsafe { with_exposed_provenance::<u32>(address as usize).read_unaligned() }
    }

    /// Calls `slot` with `words` pushed right to left; answers its `eax` and the bytes it
    /// popped. The stack goes back to where it was whatever the slot did, so a wrong pop
    /// is read and survived.
    ///
    /// # Safety
    /// `slot` is a function that returns, and `words` are what it may read.
    unsafe fn call_words(slot: u32, words: &[u32]) -> (u32, u32) {
        let answer: u32;
        let after: usize;
        let at_call: usize;
        // SAFETY: `edi` and `ebx` are saved by the callee under both conventions, so `edi`
        // still holds the stack as it was, and `esp` goes back before the block ends. The
        // loop reads `words` within its length. The rest is this function's contract.
        unsafe {
            asm!(
                "mov edi, esp",
                "2:",
                "test ecx, ecx",
                "jz 3f",
                "push dword ptr [edx + ecx*4 - 4]",
                "dec ecx",
                "jmp 2b",
                "3:",
                "mov ebx, esp",
                "call eax",
                "mov ecx, esp",
                "mov esp, edi",
                inlateout("eax") slot => answer,
                inlateout("ecx") words.len() => after,
                inlateout("edx") words.as_ptr() => _,
                out("ebx") at_call,
                out("edi") _,
                clobber_abi("system"),
            );
        }
        let popped = after.wrapping_sub(at_call);
        (
            answer,
            u32::try_from(popped).expect("a word is 32 bits here"),
        )
    }

    extern "system" fn three_system(hundreds: u32, tens: u32, ones: u32) -> u32 {
        hundreds * 100 + tens * 10 + ones
    }

    extern "C" fn three_c(hundreds: u32, tens: u32, ones: u32) -> u32 {
        hundreds * 100 + tens * 10 + ones
    }

    /// The shim's own controls: bytes popped by a 3-word stdcall and a 3-word cdecl, and
    /// whether both read their words in order.
    fn controls() -> (u32, u32, bool) {
        // SAFETY: both are functions of 3 words, and this hands them 3.
        let (system, c) = unsafe {
            (
                call_words(word(three_system as *const ()), &[1, 2, 3]),
                call_words(word(three_c as *const ()), &[1, 2, 3]),
            )
        };
        (system.1, c.1, system.0 == 123 && c.0 == 123)
    }

    /// `Unlock`'s slot on the `pop-short` plant: 16 bytes popped, not 20.
    extern "system" fn pop_short(_this: u32, _first: u32, _first_bytes: u32, _second: u32) -> u32 {
        DS_OK
    }

    /// `Unlock`'s slot on the `pop-long` plant: 24 bytes popped, not 20.
    extern "system" fn pop_long(
        _this: u32,
        _first: u32,
        _first_bytes: u32,
        _second: u32,
        _second_bytes: u32,
        _past: u32,
    ) -> u32 {
        DS_OK
    }

    /// `Lock`'s slot on the `wrap` plant: the real answer, its second word 4 bytes on.
    #[allow(
        clippy::too_many_arguments,
        reason = "`Lock`'s own 8 words, as a program pushes them"
    )]
    extern "system" fn wrap_lock(
        this: u32,
        offset: u32,
        bytes: u32,
        first: u32,
        first_bytes: u32,
        second: u32,
        second_bytes: u32,
        flags: u32,
    ) -> u32 {
        let words = [
            this,
            offset,
            bytes,
            first,
            first_bytes,
            second,
            second_bytes,
            flags,
        ];
        // SAFETY: `REAL_LOCK` is the slot this replaced, which takes these 8 words.
        let (answer, _) = unsafe { call_words(REAL_LOCK.load(Ordering::Relaxed), &words) };
        if answer == DS_OK {
            // SAFETY: a good answer wrote the caller's word at `second`, which is live.
            unsafe {
                let at = with_exposed_provenance_mut::<u32>(second as usize);
                at.write_unaligned(at.read_unaligned() + 4);
            }
        }
        answer
    }

    /// Copies `this`'s table, swaps the one slot in the copy, and points `this` at it.
    fn swap_slot(this: u32, place: u32, function: u32) {
        let slots = u32::try_from(BUFFER.slots.len()).expect("21 slots");
        // SAFETY: `this` is a live interface whose first word is a table of `slots` words.
        let mut copy: Vec<u32> = unsafe {
            let table = read_word(this);
            (0..slots)
                .map(|at| read_word(table + at * 4))
                .collect::<Vec<u32>>()
        };
        copy[place as usize] = function;
        let leaked: &'static [u32] = Box::leak(copy.into_boxed_slice());
        // SAFETY: `this`'s first word is its table, and a program's own to read; nothing
        // here reads it but the calls below, through this same word.
        unsafe {
            with_exposed_provenance_mut::<u32>(this as usize)
                .write_unaligned(word(leaked.as_ptr()));
        };
    }

    fn slot_place(table: &Table, name: &str) -> u32 {
        let place = table
            .slots
            .iter()
            .position(|slot| slot.0 == name)
            .expect("a name from the program's own table");
        u32::try_from(place).expect("under 32 slots")
    }

    /// The plant, if the arm has one, on the ring's table alone.
    fn planted(arm: Arm, ring: u32) {
        let unlock = slot_place(&BUFFER, "Unlock");
        let lock = slot_place(&BUFFER, "Lock");
        match arm {
            Arm::PopShort => swap_slot(ring, unlock, word(pop_short as *const ())),
            Arm::PopLong => swap_slot(ring, unlock, word(pop_long as *const ())),
            Arm::Wrap => {
                // SAFETY: `ring` is live and its table holds `Lock` at this place.
                let real = unsafe { read_word(read_word(ring) + lock * 4) };
                REAL_LOCK.store(real, Ordering::Relaxed);
                swap_slot(ring, lock, word(wrap_lock as *const ()));
            }
            Arm::Clean(_) | Arm::Flip => {}
        }
    }

    /// What the calls have shown wrong: the first name under each red.
    #[derive(Default)]
    struct Book {
        pop: Option<String>,
        answer: Option<String>,
        wrap: Option<String>,
    }

    impl Book {
        /// Reads the slot as a program does, calls it, and writes its `pop` line.
        fn call(&mut self, table: &Table, this: u32, name: &str, rest: &[u32]) -> u32 {
            let place = slot_place(table, name);
            let args = table.slots[place as usize].1;
            let mut words = vec![this];
            words.extend_from_slice(rest);
            assert_eq!(words.len(), args as usize, "the program's own count");
            let offset = place * 4;
            // SAFETY: `this` is a live interface, the first word of which is its table, and
            // `words` is the slot's count by the program's table, the pointers among them
            // the caller's live words.
            let (answer, popped) = unsafe {
                let slot = read_word(read_word(this) + offset);
                call_words(slot, &words)
            };
            println!(
                "pop interface={interface} slot={name} offset={offset} popped={popped} \
                 answer={answer:#x}",
                interface = table.name,
            );
            if popped != args * 4 {
                self.pop
                    .get_or_insert_with(|| format!("{}::{name}", table.name));
            }
            answer
        }

        /// Writes one worked answer's line, and notes the first that differs.
        fn said(&mut self, name: &str, got: u32, want: u32) {
            println!("answer name={name} got={got:#x} want={want:#x}");
            if got != want {
                self.answer.get_or_insert_with(|| name.to_owned());
            }
        }

        /// A call and its worked answer.
        fn ask(&mut self, table: &Table, this: u32, name: &str, rest: &[u32], want: u32) {
            let got = self.call(table, this, name, rest);
            self.said(&format!("{}::{name}", table.name), got, want);
        }

        /// A slot with no front: zeros for its words, and the worked refusal.
        fn absent(&mut self, table: &Table, this: u32, name: &str) {
            let args = table.slots[slot_place(table, name) as usize].1;
            let rest = vec![0; args as usize - 1];
            self.ask(table, this, name, &rest, DSERR_UNSUPPORTED);
        }

        /// A getter of one word, and the word it must write.
        fn reads(&mut self, this: u32, name: &str, want: u32) {
            let out = Cell::new(UNWRITTEN);
            self.ask(&BUFFER, this, name, &[word(out.as_ptr())], DS_OK);
            self.said(&format!("IDirectSoundBuffer::{name}"), out.get(), want);
        }

        /// `GetCurrentPosition`'s two bytes.
        fn position(&mut self, this: u32) -> (u32, u32) {
            let (play, write) = (Cell::new(UNWRITTEN), Cell::new(UNWRITTEN));
            let rest = [word(play.as_ptr()), word(write.as_ptr())];
            self.ask(&BUFFER, this, "GetCurrentPosition", &rest, DS_OK);
            (play.get(), write.get())
        }

        /// `GetFormat`'s rate, channels, bits and bytes written, each with its worked one.
        fn format_is(&mut self, this: u32, rate_hz: u32, bits: u16) {
            const NAME: &str = "IDirectSoundBuffer::GetFormat";
            let format = Cell::new(WaveFormat {
                tag: 0,
                channels: 0,
                rate_hz: 0,
                bytes_per_second: 0,
                block_align: 0,
                bits: 0,
                extra: 0,
            });
            let written = Cell::new(UNWRITTEN);
            let rest = [word(format.as_ptr()), FORMAT_BYTES, word(written.as_ptr())];
            self.ask(&BUFFER, this, "GetFormat", &rest, DS_OK);
            let read = format.get();
            self.said(NAME, read.rate_hz, rate_hz);
            self.said(NAME, u32::from(read.channels), 2);
            self.said(NAME, u32::from(read.bits), u32::from(bits));
            self.said(NAME, written.get(), FORMAT_BYTES);
        }

        fn wrapped(&mut self) {
            self.wrap
                .get_or_insert_with(|| "IDirectSoundBuffer::Lock".to_owned());
        }
    }

    /// The object through `Com::direct_sound_create`, a target's export's landing place,
    /// and each of its slots with no front, plus the level.
    fn object_made(book: &mut Book, com: &'static Ours) -> u32 {
        let out = Cell::new(0_u32);
        // SAFETY: `out` is this function's own word.
        let got = unsafe { com.direct_sound_create(0, out.as_ptr().cast(), 0) };
        book.said("DSOUND!DirectSoundCreate", got, DS_OK);
        let object = out.get();
        for name in [
            "QueryInterface",
            "AddRef",
            "GetCaps",
            "Compact",
            "GetSpeakerConfig",
            "SetSpeakerConfig",
            "Initialize",
        ] {
            book.absent(&OBJECT, object, name);
        }
        book.ask(
            &OBJECT,
            object,
            "SetCooperativeLevel",
            &[WINDOW, LEVEL],
            DS_OK,
        );
        object
    }

    fn buffer_made(book: &mut Book, object: u32, desc: &Desc) -> u32 {
        let out = Cell::new(0_u32);
        let rest = [word(core::ptr::from_ref(desc)), word(out.as_ptr()), 0];
        book.ask(&OBJECT, object, "CreateSoundBuffer", &rest, DS_OK);
        out.get()
    }

    /// A primary, by its worked answers.
    fn primary_checked(book: &mut Book, object: u32) -> u32 {
        let desc = Desc {
            size: DESC_BYTES,
            flags: DSBCAPS_PRIMARYBUFFER,
            bytes: 0,
            reserved: 0,
            format: null(),
        };
        let primary = buffer_made(book, object, &desc);
        book.format_is(primary, 22_050, 8);
        let format = ring_format();
        let rest = [word(core::ptr::from_ref(&format))];
        book.ask(&BUFFER, primary, "SetFormat", &rest, DS_OK);
        book.format_is(primary, MIX_HZ, 16);
        book.ask(&BUFFER, primary, "Play", &[0, 0, DSBPLAY_LOOPING], DS_OK);
        book.reads(primary, "GetStatus", STATUS_PLAYING_LOOPING);
        book.ask(&BUFFER, primary, "Stop", &[], DS_OK);
        book.reads(primary, "GetStatus", 0);
        primary
    }

    /// A ring's worked answers, before anything is written to it. Each setting goes back,
    /// so the ring sounds unchanged.
    fn ring_settings_checked(book: &mut Book, ring: u32) {
        for name in [
            "QueryInterface",
            "AddRef",
            "GetCaps",
            "GetFrequency",
            "Initialize",
        ] {
            book.absent(&BUFFER, ring, name);
        }
        book.reads(ring, "GetVolume", 0);
        book.ask(&BUFFER, ring, "SetVolume", &[VOLUME], DS_OK);
        book.reads(ring, "GetVolume", VOLUME);
        book.ask(
            &BUFFER,
            ring,
            "SetVolume",
            &[BAD_VOLUME],
            DSERR_INVALIDPARAM,
        );
        book.reads(ring, "GetVolume", VOLUME);
        book.ask(&BUFFER, ring, "SetVolume", &[0], DS_OK);
        book.reads(ring, "GetPan", 0);
        book.ask(&BUFFER, ring, "SetPan", &[PAN], DS_OK);
        book.reads(ring, "GetPan", PAN);
        book.ask(&BUFFER, ring, "SetPan", &[BAD_PAN], DSERR_INVALIDPARAM);
        book.reads(ring, "GetPan", PAN);
        book.ask(&BUFFER, ring, "SetPan", &[0], DS_OK);
        book.ask(&BUFFER, ring, "SetFrequency", &[FREQUENCY], DS_OK);
        book.ask(&BUFFER, ring, "SetFrequency", &[0], DS_OK);
        book.ask(&BUFFER, ring, "SetCurrentPosition", &[POSITION], DS_OK);
        let (play, write) = book.position(ring);
        book.said("IDirectSoundBuffer::GetCurrentPosition", play, POSITION);
        book.said("IDirectSoundBuffer::GetCurrentPosition", write, POSITION);
        book.ask(&BUFFER, ring, "SetCurrentPosition", &[0], DS_OK);
        book.ask(&BUFFER, ring, "Restore", &[], DS_OK);
    }

    /// The whole ring written through `Lock`'s 2 spans, so a wrong span shows in the sound.
    /// `base` comes from `with_bytes`, never from `Lock`.
    fn ring_written(book: &mut Book, com: &'static Ours, ring: u32, flipped: bool) -> u32 {
        let (first, first_bytes) = (Cell::new(UNWRITTEN), Cell::new(UNWRITTEN));
        let (second, second_bytes) = (Cell::new(UNWRITTEN), Cell::new(UNWRITTEN));
        let rest = [
            WRAP_AT,
            RING_BYTES,
            word(first.as_ptr()),
            word(first_bytes.as_ptr()),
            word(second.as_ptr()),
            word(second_bytes.as_ptr()),
            0,
        ];
        book.ask(&BUFFER, ring, "Lock", &rest, DS_OK);
        let base = com
            .buffers()
            .with_bytes(RING_ID, |cells| word(cells.as_ptr()))
            .expect("the ring is live");
        let spans = (
            first.get(),
            first_bytes.get(),
            second.get(),
            second_bytes.get(),
        );
        println!(
            "wrap base={base:#x} first={:#x} first_bytes={} second={:#x} second_bytes={}",
            spans.0, spans.1, spans.2, spans.3,
        );
        if spans != (base + WRAP_AT, FIRST_SPAN_BYTES, base, WRAP_AT) {
            book.wrapped();
        }
        let image = ring_image(flipped);
        let (head, tail) = image.split_at(WRAP_AT as usize);
        if (spans.1, spans.3) == (FIRST_SPAN_BYTES, WRAP_AT) {
            // SAFETY: `Lock` answered these spans of this many bytes in the live ring; a
            // planted word stays in the ring, as its 4 bytes are under the first span's 4,000.
            unsafe {
                let first_at = with_exposed_provenance_mut::<u8>(spans.0 as usize);
                core::ptr::copy_nonoverlapping(tail.as_ptr(), first_at, tail.len());
                let second_at = with_exposed_provenance_mut::<u8>(spans.2 as usize);
                core::ptr::copy_nonoverlapping(head.as_ptr(), second_at, head.len());
            }
        }
        let rest = [spans.0, spans.1, spans.2, spans.3];
        book.ask(&BUFFER, ring, "Unlock", &rest, DS_OK);
        let same = com
            .buffers()
            .with_bytes(RING_ID, |cells| {
                cells.iter().map(Cell::get).eq(image.iter().copied())
            })
            .expect("the ring is live");
        println!("wrap ring_read_back_same={same}");
        if !same {
            book.wrapped();
        }
        base
    }

    /// A duplicate: a new word, whose `Lock(ENTIREBUFFER)` answers the ring's own bytes.
    fn twin_checked(book: &mut Book, object: u32, ring: u32, base: u32) {
        let out = Cell::new(0_u32);
        let rest = [ring, word(out.as_ptr())];
        book.ask(&OBJECT, object, "DuplicateSoundBuffer", &rest, DS_OK);
        let twin = out.get();
        book.said(
            "IDirectSound::DuplicateSoundBuffer",
            u32::from(twin != 0 && twin != ring),
            1,
        );
        let (first, first_bytes) = (Cell::new(UNWRITTEN), Cell::new(UNWRITTEN));
        let (second, second_bytes) = (Cell::new(0_u32), Cell::new(0_u32));
        let rest = [
            0,
            0,
            word(first.as_ptr()),
            word(first_bytes.as_ptr()),
            word(second.as_ptr()),
            word(second_bytes.as_ptr()),
            DSBLOCK_ENTIREBUFFER,
        ];
        book.ask(&BUFFER, twin, "Lock", &rest, DS_OK);
        book.said("IDirectSoundBuffer::Lock", first.get(), base);
        let rest = [
            first.get(),
            first_bytes.get(),
            second.get(),
            second_bytes.get(),
        ];
        book.ask(&BUFFER, twin, "Unlock", &rest, DS_OK);
        book.ask(&BUFFER, twin, "Release", &[], DS_OK);
    }

    /// `Play`, then both pumps by real time, with the playing answers read past the first
    /// seam; `Stop` last, which drops what the door held.
    fn ring_played(
        book: &mut Book,
        com: &'static Ours,
        front: &'static Mix,
        ring: u32,
        pump: Duration,
    ) {
        book.ask(&BUFFER, ring, "Play", &[0, 0, DSBPLAY_LOOPING], DS_OK);
        let from = Instant::now();
        let mut mid_owed = true;
        loop {
            let clock = micros(from.elapsed());
            com.buffers().pump();
            front.pump();
            if mid_owed && clock >= MID_US {
                mid_owed = false;
                book.reads(ring, "GetStatus", STATUS_PLAYING_LOOPING);
                let (play, write) = book.position(ring);
                println!("position play={play} write={write}");
                let on_frame = |byte: u32| u32::from(byte < RING_BYTES && byte % 4 == 0);
                book.said("IDirectSoundBuffer::GetCurrentPosition", on_frame(play), 1);
                book.said("IDirectSoundBuffer::GetCurrentPosition", on_frame(write), 1);
            }
            if clock >= RUN_US {
                break;
            }
            sleep(pump);
        }
        book.ask(&BUFFER, ring, "Stop", &[], DS_OK);
        book.reads(ring, "GetStatus", 0);
    }

    /// Everything a program does, in order, once the gate has opened.
    fn calls(
        book: &mut Book,
        com: &'static Ours,
        front: &'static Mix,
        placed: &Placed<WaveOutBack>,
        arm: Arm,
    ) {
        let room_at_open = placed.room_now();
        let object = object_made(book, com);
        let primary = primary_checked(book, object);
        let format = ring_format();
        let desc = Desc {
            size: DESC_BYTES,
            flags: 0,
            bytes: RING_BYTES,
            reserved: 0,
            format: &raw const format,
        };
        let ring = buffer_made(book, object, &desc);
        planted(arm, ring);
        ring_settings_checked(book, ring);
        let base = ring_written(book, com, ring, arm == Arm::Flip);
        twin_checked(book, object, ring, base);
        let pump_ms = if let Arm::Clean(ms) = arm {
            ms
        } else {
            PLANT_PUMP_MS
        };
        ring_played(book, com, front, ring, Duration::from_millis(pump_ms));

        let tail = Instant::now();
        while placed.room_now() < room_at_open && tail.elapsed() < DRAIN {
            front.pump();
            sleep(DRAIN_STEP);
        }
        book.ask(&BUFFER, ring, "Release", &[], DS_OK);
        book.ask(&BUFFER, primary, "Release", &[], DS_OK);
        book.ask(&OBJECT, object, "Release", &[], DS_OK);
    }

    fn gate_opened() -> bool {
        let mut line = String::new();
        let read = std::io::stdin().lock().read_line(&mut line);
        matches!(read, Ok(count) if count > 0) && line.trim() == "go"
    }

    /// S6's sentence, a fault of `sound-buffers` seen through a 32-bit call.
    fn s6(name: &str) -> String {
        format!(
            "A 32-bit call into the sound buffers came back wrong at {name}. The fault is in \
             `sound-buffers`."
        )
    }

    /// What the run read, for the red words.
    struct Read {
        took: u32,
        gated: bool,
        shim_holds: bool,
        witness: u64,
        hash: u64,
        worked: u64,
        first_wrong: Option<u64>,
        dropped: u64,
        passed_over: u64,
    }

    /// The red words, each a rule the line breaks, and the sentence for the first that has
    /// one. Frames are compared by place, so a passed-over or dropped frame makes all after
    /// it read wrong, and comes first: `late` is this box's stall and takes no sentence, and
    /// a drop takes S3's, as the mix and the back hold that fault.
    fn judged(book: &Book, read: &Read) -> (Vec<&'static str>, Option<String>) {
        if read.took != MIX_HZ {
            return (vec!["open"], Some(REFUSED.to_owned()));
        }
        if !read.gated {
            return (vec!["gate"], None);
        }
        let mut reds = Vec::new();
        let mut sentence = None;
        if !read.shim_holds {
            reds.push("shim");
        }
        for (red, name) in [
            ("pop", &book.pop),
            ("answer", &book.answer),
            ("wrap", &book.wrap),
        ] {
            if let Some(name) = name {
                reds.push(red);
                sentence = sentence.or_else(|| Some(s6(name)));
            }
        }
        let late = read.passed_over > 0;
        if late {
            reds.push("late");
        }
        if read.dropped > 0 || read.witness < WITNESS_AT_LEAST {
            reds.push("counts");
        }
        if read.dropped > 0 && !late {
            sentence = sentence.or_else(|| {
                Some(format!(
                    "{} blocks of sound went missing between the mixer and the output. Your \
                     computer is cleared.",
                    read.dropped.div_ceil(BLOCK_FRAMES)
                ))
            });
        }
        if let Some(frame) = read.first_wrong {
            reds.push("mix");
            if !late {
                sentence = sentence.or_else(|| Some(s6(&format!("sound frame {frame}"))));
            }
        }
        if read.hash != read.worked {
            reds.push("hash");
        }
        (reds, sentence)
    }

    pub fn run() -> ExitCode {
        let word_given = std::env::args().nth(1).unwrap_or_default();
        let Some(arm) = arm_of(&word_given) else {
            eprintln!("usage: s6_route <clean:MS|pop-short|pop-long|wrap|flip>");
            return ExitCode::from(2);
        };
        log::set_logger(&LINES).expect("nothing else sets a logger here");
        log::set_max_level(LevelFilter::Debug);
        let (shim_system, shim_c, shim_order) = controls();

        let placed = Placed::new(WaveOutBack::new());
        let witness = Witness::new(placed.clone());
        let seen = witness.seen();
        let front: &'static Mix = Box::leak(Box::new(Front::new(&MIX_ROW, Real, witness)));
        let clock: fn() -> u64 = now_us;
        let com: &'static Ours = Box::leak(Box::new(Com::new(Buffers::new(
            &ROW,
            MixBack::new(front, clock),
        ))));

        // Empty, it adds no sound and holds the output open across the gate and the drain.
        let spare = front.stream_open(MIX_HZ, 2);
        let tally = placed.places();
        let took = tally.took.get();
        let mut book = Book::default();
        let mut gated = false;
        if took == MIX_HZ {
            gated = gate_opened();
            if gated {
                calls(&mut book, com, front, &placed, arm);
            }
        }
        front.stream_close(spare);

        let summary = seen.summary();
        let read = Read {
            took,
            gated,
            shim_holds: shim_system == 12 && shim_c == 0 && shim_order,
            witness: summary.frames,
            hash: summary.hash,
            worked: worked_hash(summary.frames),
            first_wrong: tally.first_wrong.get(),
            dropped: DROPPED.load(Ordering::Relaxed),
            passed_over: PASSED_OVER.load(Ordering::Relaxed),
        };
        let (reds, sentence) = judged(&book, &read);
        println!(
            "route=wine arm={word_given} took={took} witness={witness} blocks={blocks} \
             hash={hash:016x} worked={worked:016x} first_wrong={first_wrong} wrong={wrong} \
             dropped={dropped} passed_over={passed_over} room_low={room_low} \
             shim_system={shim_system} shim_c={shim_c} shim_order={shim_order} reds={red_words} \
             sentence=\"{said}\"",
            witness = read.witness,
            blocks = summary.blocks,
            hash = read.hash,
            worked = read.worked,
            first_wrong = read
                .first_wrong
                .map_or_else(|| "none".to_owned(), |frame| frame.to_string()),
            wrong = tally.wrong.get(),
            dropped = read.dropped,
            passed_over = read.passed_over,
            room_low = tally.room_low.get(),
            red_words = if reds.is_empty() {
                "none".to_owned()
            } else {
                reds.join(",")
            },
            said = sentence.as_deref().unwrap_or("none"),
        );
        if reds.is_empty() {
            ExitCode::SUCCESS
        } else {
            ExitCode::FAILURE
        }
    }
}
