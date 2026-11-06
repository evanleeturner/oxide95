// SPDX-License-Identifier: MIT
//! One second of one tone out of the default device, for a person to hear:
//! `cargo run -p sound-mix --features native --example native_tone`. On Linux it needs
//! ALSA's headers to build. Its lines go to stderr, so a device that refused says so.
//!
//! The crate starts no thread, so the loop below is what a program on Linux or macOS
//! owes: a pump from its own loop, often enough that the ring never runs out.

#[cfg(any(windows, target_arch = "wasm32"))]
fn main() {}

#[cfg(not(any(windows, target_arch = "wasm32")))]
fn main() {
    tone::play();
}

#[cfg(not(any(windows, target_arch = "wasm32")))]
mod tone {
    use std::thread::sleep;
    use std::time::{Duration, Instant};

    use log::{Level, LevelFilter, Log, Metadata, Record};
    use sound_mix::native::Native;
    use sound_mix::table::Row;
    use sound_mix::{Front, Host, SMP_DONE};

    static ROW: Row = Row {
        file: "native_tone",
        module: "native_tone",
    };
    static LINES: Lines = Lines;

    const MIX_HZ: u32 = 44_100;
    /// 441 Hz is exactly 100 frames a cycle.
    const FRAMES_A_CYCLE: u32 = 100;
    /// About a quarter of the full scale: a proof tone, not a loud one.
    const LOUDNESS: f64 = 8_192.0;
    /// The ring's 370 ms of frames still play after the voice has ended.
    const THE_RINGS_TAIL: Duration = Duration::from_millis(500);

    struct Lines;

    impl Log for Lines {
        fn enabled(&self, metadata: &Metadata<'_>) -> bool {
            metadata.level() <= Level::Debug
        }

        fn log(&self, record: &Record<'_>) {
            if self.enabled(record.metadata()) {
                eprintln!("{}", oxide_logline::body(record));
            }
        }

        fn flush(&self) {}
    }

    struct Clock(Instant);

    impl Host for Clock {
        #[allow(
            clippy::cast_possible_truncation,
            reason = "584,000 years of microseconds"
        )]
        fn now_us(&self) -> u64 {
            self.0.elapsed().as_micros() as u64
        }

        fn call_back(&self, _callback: u32, _handle: u32) {}

        fn read_file(&self, _name: &str) -> Option<u32> {
            None
        }

        fn free_block(&self, _block: u32) {}
    }

    /// A mono 16-bit file of the tone, one second long, at the mix's rate.
    #[allow(clippy::cast_possible_truncation)]
    fn tone_file() -> Vec<u8> {
        let bytes = MIX_HZ * 2;
        let mut file = Vec::with_capacity(44 + bytes as usize);
        file.extend_from_slice(b"RIFF");
        file.extend_from_slice(&(36 + bytes).to_le_bytes());
        file.extend_from_slice(b"WAVEfmt ");
        file.extend_from_slice(&16u32.to_le_bytes());
        file.extend_from_slice(&1u16.to_le_bytes());
        file.extend_from_slice(&1u16.to_le_bytes());
        file.extend_from_slice(&MIX_HZ.to_le_bytes());
        file.extend_from_slice(&(MIX_HZ * 2).to_le_bytes());
        file.extend_from_slice(&2u16.to_le_bytes());
        file.extend_from_slice(&16u16.to_le_bytes());
        file.extend_from_slice(b"data");
        file.extend_from_slice(&bytes.to_le_bytes());
        for frame in 0..MIX_HZ {
            let turn = f64::from(frame % FRAMES_A_CYCLE) / f64::from(FRAMES_A_CYCLE);
            let value = (turn * core::f64::consts::TAU).sin() * LOUDNESS;
            file.extend_from_slice(&(value as i16).to_le_bytes());
        }
        file
    }

    pub fn play() {
        log::set_logger(&LINES).expect("nothing else sets a logger here");
        log::set_max_level(LevelFilter::Debug);
        let front = Front::new(&ROW, Clock(Instant::now()), Native::new());
        let (_, driver) = front.wave_out_open(0, MIX_HZ, 2, 16);
        let handle = front.allocate_file_sample(driver, 0x10, &tone_file(), 0);
        front.start_sample(handle);
        while front.sample_status(handle) != SMP_DONE {
            front.pump();
            sleep(Duration::from_millis(10));
        }
        let tail = Instant::now();
        while tail.elapsed() < THE_RINGS_TAIL {
            front.pump();
            sleep(Duration::from_millis(10));
        }
    }
}
