// SPDX-License-Identifier: MIT
//! The computer under the kernel stand-ins, asked with no crate of ours, so a red here clears
//! them and a red in their own checks can name one.
//!
//! Four rows: a clock moves, a file comes back, two threads exclude, a heap holds. Natively
//! they go through `std`, and on a Windows target through `kernel32` calls declared by hand.
//! The page's rows are `page/basics-page.mjs`. Built by `rustc` or `wine.sh link`; its one Cargo
//! target is `tests/computer_basics.rs`, which takes this file by `#[path]`.
//!
//! Call: `computer_basics [<file>]`. The file row writes there, `cc1-bytes.bin` in the working
//! folder if unset, and leaves it, so a reader outside this program can check its `sum`.
//!
//! Prints each row's readings, then `CC1 pass`, or `CC1 red <row> <sentence>` for each red
//! row. Exits 0 on a pass, 1 on red.
//!
//! It does not say a clock's rate is right: two clocks wrong at one steady rate agree here. A
//! step of the wall clock during the reads gives a false red.
//!
//! `CC1`: the four rows as one check; its pass clears the computer for the kernel crates' checks.
use std::process::ExitCode;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::time::{Duration, Instant};

pub const CLOCK_RED: &str = "A clock on this computer stood still or disagreed with a second clock, with none of this project's code in the way.";
pub const FILE_RED: &str = "A file on this computer did not come back as it was written, with none of this project's code in the way.";
pub const THREAD_RED: &str = "Two threads on this computer were not kept apart by a lock, with none of this project's code in the way.";
pub const HEAP_RED: &str = "A block of memory on this computer did not hold what was put in it, with none of this project's code in the way.";

const WAIT_MS: u32 = 250;
const FILE_BYTES: usize = 65_536;
const BLOCK_BYTES: usize = 1_048_576;
/// How many refusals the held-out thread must take before its lock is let go.
pub const REFUSALS: u32 = 1_000;
const REFUSALS_LIMIT: Duration = Duration::from_secs(10);
/// One thread's turns at the shared count.
pub const ROUNDS: u64 = 200_000;

/// A clock to compare, in microseconds from any start.
pub struct Clock {
    name: &'static str,
    now_us: fn() -> u64,
    /// How far apart two reads of it can be for one instant, from each one's documentation.
    grain_us: u64,
}

/// One clock's interval, read inside another's: `inner` was read after `outer` began and
/// before `outer` ended, so it cannot have moved further than `outer` and their grains.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Nest {
    pub outer: &'static str,
    pub inner: &'static str,
    pub outer_us: u64,
    pub inner_us: u64,
    pub grain_us: u64,
}

impl Nest {
    #[must_use]
    pub fn holds(&self) -> bool {
        self.outer_us > 0 && self.inner_us > 0 && self.inner_us <= self.outer_us + self.grain_us
    }
}

/// What came back of a file written and opened again.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileRow {
    pub wrote: usize,
    pub back: usize,
    pub same: bool,
    /// The FNV-1a of what came back, for a reader outside this program.
    pub sum: u64,
    pub refused: Option<String>,
}

/// A second thread against a lock held, then two threads against one count.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ThreadRow {
    pub refused: u32,
    pub in_while_held: bool,
    pub in_on_release: bool,
    pub counted: u64,
}

/// A block written, a second one written and freed, the first read again.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[expect(
    clippy::struct_excessive_bools,
    reason = "each is its own answer, read by name"
)]
pub struct HeapRow {
    pub gave: bool,
    pub sized: bool,
    pub apart: bool,
    pub held: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rows {
    pub clock: Vec<Nest>,
    pub file: FileRow,
    pub thread: ThreadRow,
    pub heap: HeapRow,
}

/// Each red row's name and sentence; empty when all four hold.
#[must_use]
pub fn reds(rows: &Rows) -> Vec<(&'static str, &'static str)> {
    let mut found = Vec::new();
    if rows.clock.is_empty() || !rows.clock.iter().all(Nest::holds) {
        found.push(("clock", CLOCK_RED));
    }
    let file = &rows.file;
    if file.refused.is_some() || !file.same || file.back != file.wrote {
        found.push(("file", FILE_RED));
    }
    let thread = &rows.thread;
    if thread.refused < REFUSALS
        || thread.in_while_held
        || !thread.in_on_release
        || thread.counted != 2 * ROUNDS
    {
        found.push(("thread", THREAD_RED));
    }
    let heap = &rows.heap;
    if !(heap.gave && heap.sized && heap.apart && heap.held) {
        found.push(("heap", HEAP_RED));
    }
    found
}

/// Bytes that differ between seeds, from a small xorshift.
fn pattern(count: usize, seed: u32) -> Vec<u8> {
    let mut state = seed.wrapping_mul(2_654_435_761) | 1;
    (0..count)
        .map(|_| {
            state ^= state << 13;
            state ^= state >> 17;
            state ^= state << 5;
            state.to_le_bytes()[0]
        })
        .collect()
}

fn fnv1a(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf2_9ce4_8422_2325, |sum, byte| {
        (sum ^ u64::from(*byte)).wrapping_mul(0x0000_0100_0000_01b3)
    })
}

fn read(clock: &Clock) -> u64 {
    (clock.now_us)()
}

/// What each clock moved over one wait, read in `order` and then in reverse, so each clock's
/// interval lies inside those of the clocks before it.
fn moved_over_a_wait(order: &[&Clock]) -> Vec<u64> {
    let started: Vec<u64> = order.iter().map(|clock| read(clock)).collect();
    host::wait_ms(WAIT_MS);
    let mut ended = vec![0; order.len()];
    for (index, clock) in order.iter().enumerate().rev() {
        ended[index] = read(clock);
    }
    ended
        .iter()
        .zip(&started)
        .map(|(end, start)| end.saturating_sub(*start))
        .collect()
}

fn clock_row() -> Vec<Nest> {
    let clocks = host::clocks();
    let mut order: Vec<&Clock> = clocks.iter().collect();
    let first = &clocks[0];
    let first_outside = moved_over_a_wait(&order);
    order.rotate_left(1);
    let first_inside = moved_over_a_wait(&order);
    let last = clocks.len() - 1;
    let mut nests = Vec::new();
    for (index, other) in clocks.iter().enumerate().skip(1) {
        let grain_us = first.grain_us + other.grain_us;
        nests.push(Nest {
            outer: first.name,
            inner: other.name,
            outer_us: first_outside[0],
            inner_us: first_outside[index],
            grain_us,
        });
        nests.push(Nest {
            outer: other.name,
            inner: first.name,
            outer_us: first_inside[index - 1],
            inner_us: first_inside[last],
            grain_us,
        });
    }
    nests
}

fn file_row(name: &str) -> FileRow {
    let made = pattern(FILE_BYTES, 1);
    let answer = host::write_file(name, &made).and_then(|()| {
        let back = host::read_file(name)?;
        Ok(back)
    });
    match answer {
        Ok(back) => FileRow {
            wrote: made.len(),
            back: back.len(),
            same: back == made,
            sum: fnv1a(&back),
            refused: None,
        },
        Err(why) => FileRow {
            wrote: made.len(),
            back: 0,
            same: false,
            sum: 0,
            refused: Some(why),
        },
    }
}

fn other_sides_lock(lock: &Arc<host::Lock>) -> Arc<host::Lock> {
    Arc::clone(lock)
}

/// A second thread knocks at a lock this one holds: how often it was refused, whether it got
/// in during the hold, and whether it got in once the lock was let go.
fn held_out() -> (u32, bool, bool) {
    let lock = Arc::new(host::Lock::new());
    let refused = Arc::new(AtomicU32::new(0));
    let entered = Arc::new(AtomicBool::new(false));
    let held = lock.hold();
    let knock = {
        let lock = other_sides_lock(&lock);
        let refused = Arc::clone(&refused);
        let entered = Arc::clone(&entered);
        host::spawn(move || {
            loop {
                if let Some(_inside) = lock.try_hold() {
                    entered.store(true, Ordering::SeqCst);
                    return;
                }
                refused.fetch_add(1, Ordering::SeqCst);
            }
        })
    };
    let started = Instant::now();
    while refused.load(Ordering::SeqCst) < REFUSALS
        && !entered.load(Ordering::SeqCst)
        && started.elapsed() < REFUSALS_LIMIT
    {
        std::thread::yield_now();
    }
    let in_while_held = entered.load(Ordering::SeqCst);
    let refused_then = refused.load(Ordering::SeqCst);
    drop(held);
    knock.join();
    (refused_then, in_while_held, entered.load(Ordering::SeqCst))
}

/// Two threads take turns at one count, by a load and a store rather than an add, so with no
/// lock between them turns are lost.
fn counted_by_two() -> u64 {
    let lock = Arc::new(host::Lock::new());
    let count = Arc::new(AtomicU64::new(0));
    let go = Arc::new(AtomicBool::new(false));
    let side = |lock: Arc<host::Lock>| {
        let count = Arc::clone(&count);
        let go = Arc::clone(&go);
        host::spawn(move || {
            while !go.load(Ordering::SeqCst) {
                std::hint::spin_loop();
            }
            for _ in 0..ROUNDS {
                let _inside = lock.hold();
                let seen = count.load(Ordering::Relaxed);
                count.store(seen + 1, Ordering::Relaxed);
            }
        })
    };
    let sides = [side(Arc::clone(&lock)), side(other_sides_lock(&lock))];
    go.store(true, Ordering::SeqCst);
    for one in sides {
        one.join();
    }
    count.load(Ordering::SeqCst)
}

fn thread_row() -> ThreadRow {
    let (refused, in_while_held, in_on_release) = held_out();
    ThreadRow {
        refused,
        in_while_held,
        in_on_release,
        counted: counted_by_two(),
    }
}

fn apart(first: &host::Block, second: &host::Block) -> bool {
    let (one, two) = (first.bytes().as_ptr_range(), second.bytes().as_ptr_range());
    one.end <= two.start || two.end <= one.start
}

fn heap_row() -> HeapRow {
    let wanted = pattern(BLOCK_BYTES, 2);
    let refused = HeapRow {
        gave: false,
        sized: false,
        apart: false,
        held: false,
    };
    let Some(mut first) = host::Block::new(BLOCK_BYTES) else {
        return refused;
    };
    first.bytes_mut().copy_from_slice(&wanted);
    let Some(mut second) = host::Block::new(BLOCK_BYTES) else {
        return refused;
    };
    second.bytes_mut().copy_from_slice(&pattern(BLOCK_BYTES, 3));
    let sized = first.size() >= BLOCK_BYTES && second.size() >= BLOCK_BYTES;
    let apart = apart(&first, &second);
    drop(second);
    let Some(mut third) = host::Block::new(BLOCK_BYTES / 2) else {
        return refused;
    };
    third
        .bytes_mut()
        .copy_from_slice(&pattern(BLOCK_BYTES / 2, 4));
    HeapRow {
        gave: true,
        sized,
        apart,
        held: first.bytes() == wanted.as_slice(),
    }
}

/// Asks the computer all four rows; the file row writes `file` and leaves it there.
#[must_use]
pub fn measure(file: &str) -> Rows {
    Rows {
        clock: clock_row(),
        file: file_row(file),
        thread: thread_row(),
        heap: heap_row(),
    }
}

fn print_rows(rows: &Rows) {
    for nest in &rows.clock {
        println!(
            "CC1 clock outer={} inner={} outer_us={} inner_us={} grain_us={}",
            nest.outer, nest.inner, nest.outer_us, nest.inner_us, nest.grain_us
        );
    }
    let file = &rows.file;
    println!(
        "CC1 file wrote={} back={} same={} sum={:016x} refused={}",
        file.wrote,
        file.back,
        file.same,
        file.sum,
        file.refused.as_deref().unwrap_or("none")
    );
    let thread = &rows.thread;
    println!(
        "CC1 thread refused={} in_while_held={} in_on_release={} counted={} of={}",
        thread.refused,
        thread.in_while_held,
        thread.in_on_release,
        thread.counted,
        2 * ROUNDS
    );
    let heap = &rows.heap;
    println!(
        "CC1 heap gave={} sized={} apart={} held={}",
        heap.gave, heap.sized, heap.apart, heap.held
    );
}

fn main() -> ExitCode {
    let file = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "cc1-bytes.bin".to_owned());
    println!("CC1 host pointer_bytes={}", size_of::<usize>());
    let rows = measure(&file);
    print_rows(&rows);
    let found = reds(&rows);
    if found.is_empty() {
        println!("CC1 pass");
        return ExitCode::SUCCESS;
    }
    for (row, sentence) in found {
        println!("CC1 red {row} {sentence}");
    }
    ExitCode::FAILURE
}

#[cfg(not(windows))]
mod host {
    use std::sync::{Mutex, MutexGuard, OnceLock, PoisonError, TryLockError};
    use std::time::{Instant, SystemTime, UNIX_EPOCH};

    use super::Clock;

    fn instant_us() -> u64 {
        static START: OnceLock<Instant> = OnceLock::new();
        u64::try_from(START.get_or_init(Instant::now).elapsed().as_micros()).unwrap_or(u64::MAX)
    }

    fn wall_us() -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |since| {
                u64::try_from(since.as_micros()).unwrap_or(u64::MAX)
            })
    }

    /// `Instant` first, which reads finer than any grain here; `SystemTime` to 1 ms: a slew of
    /// 500 ppm over the wait is 125 us (`adjtimex`), and the read's own time sits on top.
    pub fn clocks() -> Vec<Clock> {
        vec![
            Clock {
                name: "instant",
                now_us: instant_us,
                grain_us: 0,
            },
            Clock {
                name: "wall",
                now_us: wall_us,
                grain_us: 1_000,
            },
        ]
    }

    pub fn wait_ms(ms: u32) {
        std::thread::sleep(std::time::Duration::from_millis(u64::from(ms)));
    }

    pub fn write_file(name: &str, bytes: &[u8]) -> Result<(), String> {
        std::fs::write(name, bytes).map_err(|why| why.to_string())
    }

    pub fn read_file(name: &str) -> Result<Vec<u8>, String> {
        std::fs::read(name).map_err(|why| why.to_string())
    }

    pub struct Lock(Mutex<()>);

    pub type Held<'a> = MutexGuard<'a, ()>;

    impl Lock {
        pub fn new() -> Self {
            Self(Mutex::new(()))
        }

        pub fn hold(&self) -> Held<'_> {
            self.0.lock().unwrap_or_else(PoisonError::into_inner)
        }

        pub fn try_hold(&self) -> Option<Held<'_>> {
            match self.0.try_lock() {
                Ok(held) => Some(held),
                Err(TryLockError::Poisoned(held)) => Some(held.into_inner()),
                Err(TryLockError::WouldBlock) => None,
            }
        }
    }

    pub struct Thread(std::thread::JoinHandle<()>);

    impl Thread {
        pub fn join(self) {
            self.0.join().expect("a thread of this check panicked");
        }
    }

    pub fn spawn(work: impl FnOnce() + Send + 'static) -> Thread {
        Thread(std::thread::spawn(work))
    }

    pub struct Block(Vec<u8>);

    impl Block {
        pub fn new(size: usize) -> Option<Self> {
            let mut bytes = Vec::new();
            bytes.try_reserve_exact(size).ok()?;
            bytes.resize(size, 0);
            Some(Self(bytes))
        }

        pub fn size(&self) -> usize {
            self.0.capacity()
        }

        pub fn bytes(&self) -> &[u8] {
            &self.0
        }

        pub fn bytes_mut(&mut self) -> &mut [u8] {
            &mut self.0
        }
    }
}

#[cfg(windows)]
#[allow(unsafe_code, reason = "this module is the calls declared by hand")]
mod host {
    use std::cell::UnsafeCell;
    use std::ffi::{CString, c_void};
    use std::ptr::{null, null_mut};

    use super::Clock;

    type Handle = *mut c_void;
    type Start = unsafe extern "system" fn(*mut c_void) -> u32;

    const BAD_HANDLE: Handle = usize::MAX as Handle;
    const WRITE: u32 = 0x4000_0000;
    const READ: u32 = 0x8000_0000;
    const CREATE_ALWAYS: u32 = 2;
    const OPEN_EXISTING: u32 = 3;
    const NORMAL: u32 = 0x80;
    const FOREVER: u32 = 0xffff_ffff;
    const READ_CHUNK: u32 = 4_096;
    /// Microsoft's pages put the system timer behind both at "10 to 16 ms", from memory.
    const TIMER_GRAIN_US: u64 = 16_000;

    #[repr(C)]
    #[derive(Default)]
    struct CritSec {
        debug: usize,
        lock_count: i32,
        recursion_count: i32,
        owning_thread: usize,
        lock_semaphore: usize,
        spin_count: usize,
    }

    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn QueryPerformanceCounter(count: *mut i64) -> i32;
        fn QueryPerformanceFrequency(rate: *mut i64) -> i32;
        fn GetSystemTimeAsFileTime(time: *mut [u32; 2]);
        fn GetTickCount() -> u32;
        fn Sleep(ms: u32);
        fn GetLastError() -> u32;
        fn CreateFileA(
            name: *const std::ffi::c_char,
            access: u32,
            share: u32,
            security: *const c_void,
            disposition: u32,
            flags: u32,
            template: Handle,
        ) -> Handle;
        fn ReadFile(
            file: Handle,
            buffer: *mut u8,
            count: u32,
            done: *mut u32,
            overlapped: *mut c_void,
        ) -> i32;
        fn WriteFile(
            file: Handle,
            buffer: *const u8,
            count: u32,
            done: *mut u32,
            overlapped: *mut c_void,
        ) -> i32;
        fn CloseHandle(handle: Handle) -> i32;
        fn CreateThread(
            security: *const c_void,
            stack: usize,
            start: Start,
            argument: *mut c_void,
            flags: u32,
            id: *mut u32,
        ) -> Handle;
        fn WaitForSingleObject(handle: Handle, ms: u32) -> u32;
        fn InitializeCriticalSection(section: *mut CritSec);
        fn EnterCriticalSection(section: *mut CritSec);
        fn TryEnterCriticalSection(section: *mut CritSec) -> i32;
        fn LeaveCriticalSection(section: *mut CritSec);
        fn DeleteCriticalSection(section: *mut CritSec);
        fn GetProcessHeap() -> Handle;
        fn HeapAlloc(heap: Handle, flags: u32, size: usize) -> *mut c_void;
        fn HeapSize(heap: Handle, flags: u32, block: *const c_void) -> usize;
        fn HeapFree(heap: Handle, flags: u32, block: *mut c_void) -> i32;
    }

    fn counter_us() -> u64 {
        let (mut count, mut rate) = (0_i64, 0_i64);
        // SAFETY: each call writes one `i64` we own.
        unsafe {
            QueryPerformanceCounter(&raw mut count);
            QueryPerformanceFrequency(&raw mut rate);
        }
        let micros = i128::from(count) * 1_000_000 / i128::from(rate.max(1));
        u64::try_from(micros).unwrap_or(u64::MAX)
    }

    fn wall_us() -> u64 {
        let mut time = [0_u32; 2];
        // SAFETY: the call writes one `FILETIME`, 2 `u32`s, into memory we own.
        unsafe { GetSystemTimeAsFileTime(&raw mut time) };
        ((u64::from(time[1]) << 32) | u64::from(time[0])) / 10
    }

    fn tick_us() -> u64 {
        // SAFETY: the call takes nothing and touches no memory of ours.
        u64::from(unsafe { GetTickCount() }) * 1_000
    }

    pub fn clocks() -> Vec<Clock> {
        vec![
            Clock {
                name: "counter",
                now_us: counter_us,
                grain_us: 0,
            },
            Clock {
                name: "wall",
                now_us: wall_us,
                grain_us: TIMER_GRAIN_US,
            },
            Clock {
                name: "tick",
                now_us: tick_us,
                grain_us: TIMER_GRAIN_US,
            },
        ]
    }

    pub fn wait_ms(ms: u32) {
        // SAFETY: the call takes a number and touches no memory of ours.
        unsafe { Sleep(ms) };
    }

    fn last_error(call: &str) -> String {
        // SAFETY: the call takes nothing and touches no memory of ours.
        format!("{call} error {}", unsafe { GetLastError() })
    }

    fn open(name: &str, access: u32, disposition: u32) -> Result<Handle, String> {
        let path = CString::new(name).map_err(|why| why.to_string())?;
        // SAFETY: `path` ends in a zero and outlives the call; the rest are plain values.
        let file = unsafe {
            CreateFileA(
                path.as_ptr(),
                access,
                0,
                null(),
                disposition,
                NORMAL,
                null_mut(),
            )
        };
        if file == BAD_HANDLE {
            return Err(last_error("CreateFileA"));
        }
        Ok(file)
    }

    pub fn write_file(name: &str, bytes: &[u8]) -> Result<(), String> {
        let file = open(name, WRITE, CREATE_ALWAYS)?;
        let count = u32::try_from(bytes.len()).map_err(|why| why.to_string())?;
        let mut done = 0;
        // SAFETY: `bytes` holds `count` bytes and `done` is ours; `file` is open.
        let wrote = unsafe { WriteFile(file, bytes.as_ptr(), count, &raw mut done, null_mut()) };
        let why = (wrote == 0).then(|| last_error("WriteFile"));
        // SAFETY: `file` is open, and this is its one close.
        unsafe { CloseHandle(file) };
        match why {
            Some(why) => Err(why),
            None if done != count => Err(format!("WriteFile wrote {done} of {count}")),
            None => Ok(()),
        }
    }

    pub fn read_file(name: &str) -> Result<Vec<u8>, String> {
        let file = open(name, READ, OPEN_EXISTING)?;
        let mut back = Vec::new();
        let mut chunk = [0_u8; READ_CHUNK as usize];
        let answer = loop {
            let mut done = 0;
            // SAFETY: `chunk` holds what is asked and `done` is ours; `file` is open.
            let read = unsafe {
                ReadFile(
                    file,
                    chunk.as_mut_ptr(),
                    READ_CHUNK,
                    &raw mut done,
                    null_mut(),
                )
            };
            if read == 0 {
                break Err(last_error("ReadFile"));
            }
            if done == 0 {
                break Ok(());
            }
            back.extend_from_slice(&chunk[..done as usize]);
        };
        // SAFETY: `file` is open, and this is its one close.
        unsafe { CloseHandle(file) };
        answer.map(|()| back)
    }

    pub struct Lock {
        section: Box<UnsafeCell<CritSec>>,
    }

    // SAFETY: a critical section is made to be entered from many threads, and its box does not
    // move.
    unsafe impl Send for Lock {}
    // SAFETY: as above.
    unsafe impl Sync for Lock {}

    pub struct Held<'a>(&'a Lock);

    impl Lock {
        pub fn new() -> Self {
            let lock = Self {
                section: Box::default(),
            };
            // SAFETY: the section is ours, zeroed, and initialized this once.
            unsafe { InitializeCriticalSection(lock.section.get()) };
            lock
        }

        pub fn hold(&self) -> Held<'_> {
            // SAFETY: the section was initialized in `new` and lives as long as `self`.
            unsafe { EnterCriticalSection(self.section.get()) };
            Held(self)
        }

        pub fn try_hold(&self) -> Option<Held<'_>> {
            // SAFETY: as in `hold`.
            let entered = unsafe { TryEnterCriticalSection(self.section.get()) };
            (entered != 0).then(|| Held(self))
        }
    }

    impl Drop for Held<'_> {
        fn drop(&mut self) {
            // SAFETY: this thread entered the section to make this value.
            unsafe { LeaveCriticalSection(self.0.section.get()) };
        }
    }

    impl Drop for Lock {
        fn drop(&mut self) {
            // SAFETY: a `Held` borrows the lock, so no thread is inside at its drop.
            unsafe { DeleteCriticalSection(self.section.get()) };
        }
    }

    pub struct Thread(Handle);

    impl Thread {
        pub fn join(self) {
            // SAFETY: the handle came from `CreateThread`, and this is its one close.
            unsafe {
                WaitForSingleObject(self.0, FOREVER);
                CloseHandle(self.0);
            }
        }
    }

    type Work = Box<dyn FnOnce() + Send>;

    unsafe extern "system" fn run_work(argument: *mut c_void) -> u32 {
        // SAFETY: `spawn` made this from `Box::into_raw` and hands it to this one thread.
        let work = unsafe { Box::from_raw(argument.cast::<Work>()) };
        work();
        0
    }

    pub fn spawn(work: impl FnOnce() + Send + 'static) -> Thread {
        let work: Box<Work> = Box::new(Box::new(work));
        let argument = Box::into_raw(work).cast::<c_void>();
        // SAFETY: `run_work` takes back the box `argument` points at, on the new thread.
        let handle = unsafe { CreateThread(null(), 0, run_work, argument, 0, null_mut()) };
        assert!(!handle.is_null(), "{}", last_error("CreateThread"));
        Thread(handle)
    }

    pub struct Block {
        address: *mut u8,
        asked: usize,
    }

    impl Block {
        pub fn new(size: usize) -> Option<Self> {
            // SAFETY: the process's own heap, no flags, a plain size.
            let address = unsafe { HeapAlloc(GetProcessHeap(), 0, size) }.cast::<u8>();
            (!address.is_null()).then(|| Self {
                address,
                asked: size,
            })
        }

        pub fn size(&self) -> usize {
            // SAFETY: the block came from this heap and is not yet freed.
            unsafe { HeapSize(GetProcessHeap(), 0, self.address.cast_const().cast()) }
        }

        pub fn bytes(&self) -> &[u8] {
            // SAFETY: the block holds `asked` bytes, each written before it is read.
            unsafe { std::slice::from_raw_parts(self.address, self.asked) }
        }

        pub fn bytes_mut(&mut self) -> &mut [u8] {
            // SAFETY: as in `bytes`, and `&mut self` makes this the one reference.
            unsafe { std::slice::from_raw_parts_mut(self.address, self.asked) }
        }
    }

    impl Drop for Block {
        fn drop(&mut self) {
            // SAFETY: the block came from this heap, and this is its one free.
            unsafe { HeapFree(GetProcessHeap(), 0, self.address.cast()) };
        }
    }
}
