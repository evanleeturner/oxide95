// SPDX-License-Identifier: MIT
//! Wine's own sound layer, asked by a 32-bit program with no crate of ours, its calls
//! declared by hand. One file, built by `wine.sh link`; no Cargo target.
//!
//! - `query <rate> <channels> <bits>`: C8. Does Wine offer a device for that format? It
//!   asks and opens nothing, so nothing plays. Calling it with CD's format is the check.
//! - `tone <hz> <ms> <amplitude>`: plays a tone in CD's format, for C9's recording.
//!
//! Lines take the log's `imp.call` form, printed as this file links no logger. Exits 0
//! on a pass, 1 on red, 2 on a call it cannot read.
use std::process::ExitCode;
use std::time::{Duration, Instant};

const WAVE_MAPPER: u32 = 0xFFFF_FFFF;
const WAVE_FORMAT_QUERY: u32 = 1;
const WHDR_DONE: u32 = 1;
const CD_RATE: u32 = 44_100;
const C8_PASS: &str = "Wine offers a device for CD-quality sound.";
const C8_RED: &str = "Wine offers no device for CD-quality sound. The bench is at fault, and the project's code is cleared.";

/// Windows' `WAVEFORMATEX`.
#[repr(C, packed)]
struct Format {
    tag: u16,
    channels: u16,
    rate: u32,
    bytes_per_second: u32,
    block_align: u16,
    bits: u16,
    extra: u16,
}

impl Format {
    fn pcm(rate: u32, channels: u16, bits: u16) -> Self {
        let block_align = channels.wrapping_mul(bits) / 8;
        Self {
            tag: 1,
            channels,
            rate,
            bytes_per_second: rate.wrapping_mul(u32::from(block_align)),
            block_align,
            bits,
            extra: 0,
        }
    }
}

/// Windows' `WAVEHDR`.
#[repr(C)]
struct Header {
    data: *mut u8,
    length: u32,
    recorded: u32,
    user: usize,
    flags: u32,
    loops: u32,
    next: *mut Header,
    reserved: usize,
}

#[link(name = "winmm")]
unsafe extern "system" {
    fn waveOutGetNumDevs() -> u32;
    fn waveOutOpen(
        out: *mut usize,
        device: u32,
        format: *const Format,
        callback: usize,
        instance: usize,
        flags: u32,
    ) -> u32;
    fn waveOutPrepareHeader(handle: usize, header: *mut Header, size: u32) -> u32;
    fn waveOutWrite(handle: usize, header: *mut Header, size: u32) -> u32;
    fn waveOutUnprepareHeader(handle: usize, header: *mut Header, size: u32) -> u32;
    fn waveOutClose(handle: usize) -> u32;
}

fn query(rate: u32, channels: u16, bits: u16) -> ExitCode {
    let format = Format::pcm(rate, channels, bits);
    // SAFETY: the call takes no arguments and only counts devices.
    let devices = unsafe { waveOutGetNumDevs() };
    println!("imp.call c=WINMM!waveOutGetNumDevs v={devices} by=layer-check");
    // SAFETY: `format` lives across the call; with the query flag and no handle asked
    // for, the call reads the format and opens nothing.
    let answer = unsafe {
        waveOutOpen(
            std::ptr::null_mut(),
            WAVE_MAPPER,
            &raw const format,
            0,
            0,
            WAVE_FORMAT_QUERY,
        )
    };
    println!(
        "imp.call c=WINMM!waveOutOpen a={rate},{channels},{bits},query v={answer} by=layer-check"
    );
    println!(
        "pointer_bytes={} devices={devices} format_query={answer}",
        size_of::<usize>()
    );
    if devices > 0 && answer == 0 {
        println!("C8 pass {C8_PASS}");
        ExitCode::SUCCESS
    } else {
        println!("C8 red {C8_RED}");
        ExitCode::FAILURE
    }
}

/// CD's format, both channels alike.
fn samples(hz: f64, ms: u32, amplitude: i16) -> Vec<u8> {
    let step = std::f64::consts::TAU * hz / f64::from(CD_RATE);
    (0..CD_RATE / 1000 * ms)
        .flat_map(|index| {
            // A sine times an i16 stays inside i16; dropping the fraction is the rounding.
            let sample = (f64::from(amplitude) * (step * f64::from(index)).sin()) as i16;
            let [low, high] = sample.to_le_bytes();
            [low, high, low, high]
        })
        .collect()
}

fn tone(hz: f64, ms: u32, amplitude: i16) -> ExitCode {
    let format = Format::pcm(CD_RATE, 2, 16);
    let mut sound = samples(hz, ms, amplitude);
    let Ok(length) = u32::try_from(sound.len()) else {
        return ExitCode::from(2);
    };
    let mut handle = 0_usize;
    // SAFETY: `handle` and `format` live across the call; no callback is asked for.
    let opened = unsafe { waveOutOpen(&raw mut handle, WAVE_MAPPER, &raw const format, 0, 0, 0) };
    println!("imp.call c=WINMM!waveOutOpen a={CD_RATE},2,16 v={opened} by=layer-check");
    if opened != 0 {
        return ExitCode::FAILURE;
    }
    let mut header = Header {
        data: sound.as_mut_ptr(),
        length,
        recorded: 0,
        user: 0,
        flags: 0,
        loops: 0,
        next: std::ptr::null_mut(),
        reserved: 0,
    };
    // `size_of` a struct of 8 words fits a u32.
    let size = size_of::<Header>() as u32;
    // SAFETY: `handle` is open; `header` and `sound` outlive it, unprepared below.
    let prepared = unsafe { waveOutPrepareHeader(handle, &raw mut header, size) };
    // SAFETY: as above, and the header is prepared or the call refuses it.
    let wrote = unsafe { waveOutWrite(handle, &raw mut header, size) };
    println!("imp.call c=WINMM!waveOutWrite a={length} o={prepared} v={wrote} by=layer-check");
    let limit = Instant::now() + Duration::from_millis(u64::from(ms) + 3000);
    // SAFETY: Wine sets the done flag from its own thread, so it is read fresh each time.
    while wrote == 0
        && unsafe { std::ptr::read_volatile(&raw const header.flags) } & WHDR_DONE == 0
        && Instant::now() < limit
    {
        std::thread::sleep(Duration::from_millis(10));
    }
    // SAFETY: the header was prepared on this open handle, and the wait has ended.
    let done = unsafe { waveOutUnprepareHeader(handle, &raw mut header, size) };
    // SAFETY: the handle is open and nothing uses it after this.
    let closed = unsafe { waveOutClose(handle) };
    println!("imp.call c=WINMM!waveOutClose o={done} v={closed} by=layer-check");
    if wrote == 0 {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}

/// Reads the call, or says the call cannot be read.
fn run(args: &[&str]) -> Option<ExitCode> {
    Some(match *args {
        ["query", rate, channels, bits] => query(
            rate.parse().ok()?,
            channels.parse().ok()?,
            bits.parse().ok()?,
        ),
        ["tone", hz, ms, amplitude] => {
            tone(hz.parse().ok()?, ms.parse().ok()?, amplitude.parse().ok()?)
        }
        _ => return None,
    })
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    run(&args).unwrap_or_else(|| {
        eprintln!("usage: waveprobe query <rate> <channels> <bits> | tone <hz> <ms> <amplitude>");
        ExitCode::from(2)
    })
}
