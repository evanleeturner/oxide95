// SPDX-License-Identifier: MIT
//! What the checks of `oxide_trace`'s macros stand on: a logger that keeps every line
//! for the test to read, the logbook to judge each line by, and a page of memory that
//! counts every touch of it. The page is the one honest count of a plain `*p`, as such
//! a read runs no code of ours.
//!
//! It is written for two hosts, Linux `x86_64` and Windows `x86`, and is empty on any
//! other. Its counts hold for debug builds only: an optimizing build may merge two reads
//! of the page into one.
#![cfg(any(
    all(target_os = "linux", target_arch = "x86_64"),
    all(windows, target_arch = "x86")
))]

use std::cell::RefCell;
use std::collections::BTreeSet;
use std::marker::PhantomData;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Mutex, MutexGuard, PoisonError};

use log::kv::{Error, Key, Value, VisitSource};
use log::{Level, LevelFilter, Log, Metadata, Record};

/// The page's fixed address, so that the worked table's addresses are written by hand.
pub const PAGE: usize = 0x2000_0000;
/// The page's length.
pub const PAGE_SIZE: usize = 4096;
/// The `u32` a site under test reads and writes.
pub const CELL: usize = PAGE + 0x10;
/// A `u32` a copy takes its bytes from.
pub const SOURCE: usize = PAGE + 0x20;
/// What [`Guard::reset`] leaves at [`SOURCE`].
pub const SOURCE_VALUE: u32 = 0x1122_3344;

const BOOK: &str = include_str!("../../../logbook.json");

static GATE: Mutex<()> = Mutex::new(());
static ROWS: Mutex<Vec<Row>> = Mutex::new(Vec::new());
static CAPTURE: Capture = Capture;
static FAULTS: AtomicUsize = AtomicUsize::new(0);
static STEPPING: AtomicBool = AtomicBool::new(false);

/// One log line as the facade handed it over.
#[derive(Clone, Debug)]
pub struct Row {
    pub level: Level,
    pub id: String,
    pub fields: Vec<(String, String)>,
}

impl Row {
    /// `id k=v k=v`, the form the worked table is written in.
    #[must_use]
    pub fn text(&self) -> String {
        let mut text = self.id.clone();
        for (key, value) in &self.fields {
            text.push(' ');
            text.push_str(key);
            text.push('=');
            text.push_str(value);
        }
        text
    }
}

struct Fields(Vec<(String, String)>);

impl<'kvs> VisitSource<'kvs> for Fields {
    fn visit_pair(&mut self, key: Key<'kvs>, value: Value<'kvs>) -> Result<(), Error> {
        self.0.push((key.as_str().to_owned(), value.to_string()));
        Ok(())
    }
}

struct Capture;

impl Log for Capture {
    fn enabled(&self, _metadata: &Metadata<'_>) -> bool {
        true
    }

    fn log(&self, record: &Record<'_>) {
        let mut fields = Fields(Vec::new());
        record
            .key_values()
            .visit(&mut fields)
            .expect("keeping a field cannot fail");
        let row = Row {
            level: record.level(),
            id: record.args().to_string(),
            fields: fields.0,
        };
        ROWS.lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(row);
    }

    fn flush(&self) {}
}

/// One test's hold on what the whole process shares, the log level and the page: tests
/// run one at a time behind it, and a red test does not turn the next one red.
pub struct Session {
    _held: MutexGuard<'static, ()>,
}

impl Session {
    /// Waits for the gate; leaves logging on and no row kept.
    #[must_use]
    pub fn begin() -> Self {
        let held = GATE.lock().unwrap_or_else(PoisonError::into_inner);
        // An earlier test in this process has set the same logger, which is no fault.
        let _ = log::set_logger(&CAPTURE);
        let session = Self { _held: held };
        session.logging(true);
        let _ = session.take();
        session
    }

    /// Turns the facade's level to DEBUG or to nothing at all.
    pub fn logging(&self, on: bool) {
        log::set_max_level(if on {
            LevelFilter::Debug
        } else {
            LevelFilter::Off
        });
    }

    /// The rows written since the last call.
    #[must_use]
    pub fn take(&self) -> Vec<Row> {
        std::mem::take(&mut *ROWS.lock().unwrap_or_else(PoisonError::into_inner))
    }

    /// Maps the page and starts counting; [`Guard::reset`] fills it.
    #[must_use]
    pub fn guard(&self) -> Guard<'_> {
        Guard {
            armed: host::arm(),
            session: PhantomData,
        }
    }
}

/// The page, closed: each touch of it faults, is counted, and is let through for the one
/// instruction that made it. A fault is the only thing that counts, so a read made by any
/// code at all is seen: the site's, the macro's, or the log line's.
pub struct Guard<'session> {
    armed: host::Armed,
    session: PhantomData<&'session Session>,
}

impl Guard<'_> {
    /// Fills [`CELL`] and [`SOURCE`] and zeroes the count, none of it counted.
    pub fn reset(&self, cell: u32) {
        host::set_open(true);
        // SAFETY: `armed` mapped the page, it is open, and both cells are aligned.
        unsafe {
            (CELL as *mut u32).write(cell);
            (SOURCE as *mut u32).write(SOURCE_VALUE);
        }
        host::set_open(false);
        FAULTS.store(0, Ordering::SeqCst);
    }

    /// The touches counted since [`Guard::reset`].
    #[must_use]
    pub fn faults(&self) -> usize {
        FAULTS.load(Ordering::SeqCst)
    }

    /// What [`CELL`] holds, read with the page open, so not counted.
    #[must_use]
    pub fn cell(&self) -> u32 {
        host::set_open(true);
        // SAFETY: `armed` mapped the page, it is open, and the cell is aligned.
        let value = unsafe { (CELL as *const u32).read() };
        host::set_open(false);
        value
    }
}

impl Drop for Guard<'_> {
    fn drop(&mut self) {
        host::disarm(&self.armed);
    }
}

/// A log of what a site evaluated, in order: each wrapped expression runs through it.
#[derive(Default)]
pub struct Ticks(RefCell<Vec<&'static str>>);

impl Ticks {
    /// Notes `name` and gives `value` back.
    pub fn hit<T>(&self, name: &'static str, value: T) -> T {
        self.0.borrow_mut().push(name);
        value
    }

    /// As [`Ticks::hit`], for a macro that takes its argument by reference and so refuses
    /// a temporary: `*ticks.at("a", &value)` is a place, and is evaluated as one.
    pub fn at<'value, T: ?Sized>(&self, name: &'static str, value: &'value T) -> &'value T {
        self.0.borrow_mut().push(name);
        value
    }

    /// The names noted, in order.
    #[must_use]
    pub fn names(&self) -> Vec<&'static str> {
        self.0.borrow().clone()
    }
}

/// Why `row` disagrees with the logbook, if it does: its id is unknown, its level is not
/// the book's `D`, or its keys are not the book's fields. `fn.in`'s `a` may be missing,
/// which the book says itself: "absent when it takes none".
#[must_use]
pub fn book_fault(row: &Row) -> Option<String> {
    let book: serde_json::Value = serde_json::from_str(BOOK).expect("the logbook is JSON");
    let events = book["events"].as_array().expect("the logbook has events");
    let Some(entry) = events.iter().find(|entry| entry["id"] == row.id.as_str()) else {
        return Some(format!("{} is not in the logbook", row.id));
    };
    if entry["level"] != "D" || row.level != Level::Debug {
        return Some(format!(
            "{} is {} here and {} in the logbook",
            row.id, row.level, entry["level"]
        ));
    }
    let declared: BTreeSet<&str> = entry["fields"]
        .as_object()
        .expect("a logbook row has fields")
        .keys()
        .map(String::as_str)
        .collect();
    let logged: BTreeSet<&str> = row.fields.iter().map(|(key, _)| key.as_str()).collect();
    let bare_entry = row.id == "fn.in" && logged == BTreeSet::from(["f"]);
    (declared != logged && !bare_entry)
        .then(|| format!("{} logs {logged:?}, the book says {declared:?}", row.id))
}

#[cfg(target_os = "linux")]
mod host {
    use std::ffi::{c_int, c_void};
    use std::ptr::null_mut;
    use std::sync::atomic::Ordering;

    use super::{FAULTS, PAGE, PAGE_SIZE, STEPPING};

    const PROT_NONE: c_int = 0;
    const PROT_READ_WRITE: c_int = 3;
    /// `MAP_PRIVATE | MAP_ANONYMOUS | MAP_FIXED_NOREPLACE`.
    const MAP_HERE: c_int = 0x02 | 0x20 | 0x10_0000;
    const SIGTRAP: c_int = 5;
    const SIGSEGV: c_int = 11;
    const SA_SIGINFO: c_int = 4;
    const SIG_DFL: usize = 0;
    /// `siginfo_t`'s `si_addr`.
    const FAULT_ADDRESS_AT: usize = 16;
    /// `ucontext_t`'s `gregs[REG_EFL]`.
    const FLAGS_AT: usize = 176;
    const TRAP_FLAG: u64 = 0x100;

    /// glibc's `struct sigaction` on `x86_64`.
    #[repr(C)]
    #[derive(Clone, Copy)]
    struct SigAction {
        handler: usize,
        mask: [u64; 16],
        flags: c_int,
        restorer: usize,
    }

    impl SigAction {
        const fn new(handler: usize, flags: c_int) -> Self {
            Self {
                handler,
                mask: [0; 16],
                flags,
                restorer: 0,
            }
        }
    }

    unsafe extern "C" {
        fn mmap(
            address: *mut c_void,
            length: usize,
            protection: c_int,
            flags: c_int,
            file: c_int,
            offset: i64,
        ) -> *mut c_void;
        fn munmap(address: *mut c_void, length: usize) -> c_int;
        fn mprotect(address: *mut c_void, length: usize, protection: c_int) -> c_int;
        fn sigaction(signal: c_int, action: *const SigAction, old: *mut SigAction) -> c_int;
    }

    /// The two handlers this replaced, as Rust's own overflow handler sits on `SIGSEGV`.
    pub(super) struct Armed {
        old_fault: SigAction,
        old_trap: SigAction,
    }

    pub(super) fn set_open(open: bool) {
        let protection = if open { PROT_READ_WRITE } else { PROT_NONE };
        // SAFETY: only the page this crate mapped changes, and nothing but this crate
        // holds an address in it.
        let answer = unsafe { mprotect(PAGE as *mut c_void, PAGE_SIZE, protection) };
        assert_eq!(answer, 0, "mprotect refused the page");
    }

    /// A fault on the page: counts it, opens the page and sets the trap flag, so that the
    /// one instruction runs and traps. The trap: closes the page, clears the flag. Any
    /// other fault or trap is not ours: it goes back to the default and kills the process.
    unsafe extern "C" fn on_signal(signal: c_int, info: *mut c_void, context: *mut c_void) {
        // SAFETY: the kernel hands a `SA_SIGINFO` handler a live context, and the flags
        // sit at this offset of it on this host.
        let flags = unsafe { context.cast::<u8>().add(FLAGS_AT).cast::<u64>() };
        let ours = if signal == SIGSEGV {
            // SAFETY: the kernel hands a `SA_SIGINFO` handler a live info, and the
            // address sits at this offset of it for a `SIGSEGV`.
            let address = unsafe {
                info.cast::<u8>()
                    .add(FAULT_ADDRESS_AT)
                    .cast::<usize>()
                    .read()
            };
            (PAGE..PAGE + PAGE_SIZE).contains(&address)
        } else {
            STEPPING.load(Ordering::SeqCst)
        };
        if !ours {
            let default = SigAction::new(SIG_DFL, 0);
            // SAFETY: `sigaction` may be called from a handler, and `default` is live.
            unsafe { sigaction(signal, &raw const default, null_mut()) };
        } else if signal == SIGSEGV {
            FAULTS.fetch_add(1, Ordering::SeqCst);
            STEPPING.store(true, Ordering::SeqCst);
            set_open(true);
            // SAFETY: `flags` points into the live context, restored on return.
            unsafe { *flags |= TRAP_FLAG };
        } else {
            STEPPING.store(false, Ordering::SeqCst);
            set_open(false);
            // SAFETY: `flags` points into the live context, restored on return.
            unsafe { *flags &= !TRAP_FLAG };
        }
    }

    pub(super) fn arm() -> Armed {
        // SAFETY: a fresh anonymous mapping, refused rather than laid over anything.
        let page = unsafe {
            mmap(
                PAGE as *mut c_void,
                PAGE_SIZE,
                PROT_READ_WRITE,
                MAP_HERE,
                -1,
                0,
            )
        };
        assert_eq!(page.addr(), PAGE, "the page's fixed address is taken");
        let action = SigAction::new(on_signal as *const () as usize, SA_SIGINFO);
        let mut armed = Armed {
            old_fault: SigAction::new(SIG_DFL, 0),
            old_trap: SigAction::new(SIG_DFL, 0),
        };
        // SAFETY: `action` and both old slots are live, and `on_signal` touches only
        // atomics, the page's protection and the context it is handed.
        unsafe {
            sigaction(SIGSEGV, &raw const action, &raw mut armed.old_fault);
            sigaction(SIGTRAP, &raw const action, &raw mut armed.old_trap);
        }
        armed
    }

    pub(super) fn disarm(armed: &Armed) {
        // SAFETY: the old actions came from `sigaction`, the old slots may be null, and
        // nothing is left holding an address in the page.
        unsafe {
            sigaction(SIGSEGV, &raw const armed.old_fault, null_mut());
            sigaction(SIGTRAP, &raw const armed.old_trap, null_mut());
            munmap(PAGE as *mut c_void, PAGE_SIZE);
        }
    }
}

#[cfg(windows)]
mod host {
    use std::ffi::c_void;
    use std::sync::atomic::Ordering;

    use super::{FAULTS, PAGE, PAGE_SIZE, STEPPING};

    const MEM_RESERVE_COMMIT: u32 = 0x3000;
    const MEM_RELEASE: u32 = 0x8000;
    const PAGE_NOACCESS: u32 = 1;
    const PAGE_READWRITE: u32 = 4;
    const ACCESS_VIOLATION: u32 = 0xC000_0005;
    const SINGLE_STEP: u32 = 0x8000_0004;
    const CONTINUE_EXECUTION: i32 = -1;
    const CONTINUE_SEARCH: i32 = 0;
    /// `EXCEPTION_RECORD`'s `ExceptionInformation[1]` on `x86`.
    const FAULT_ADDRESS_AT: usize = 24;
    /// `CONTEXT`'s `EFlags` on `x86`.
    const FLAGS_AT: usize = 0xC0;
    const TRAP_FLAG: u32 = 0x100;

    #[repr(C)]
    struct ExceptionPointers {
        record: *const u8,
        context: *mut u8,
    }

    type Handler = unsafe extern "system" fn(*mut ExceptionPointers) -> i32;

    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn VirtualAlloc(
            address: *mut c_void,
            length: usize,
            kind: u32,
            protection: u32,
        ) -> *mut c_void;
        fn VirtualFree(address: *mut c_void, length: usize, kind: u32) -> i32;
        fn VirtualProtect(
            address: *mut c_void,
            length: usize,
            protection: u32,
            old: *mut u32,
        ) -> i32;
        fn AddVectoredExceptionHandler(first: u32, handler: Handler) -> *mut c_void;
        fn RemoveVectoredExceptionHandler(handle: *mut c_void) -> u32;
    }

    pub(super) struct Armed(*mut c_void);

    pub(super) fn set_open(open: bool) {
        let protection = if open { PAGE_READWRITE } else { PAGE_NOACCESS };
        let mut old = 0;
        // SAFETY: only the page this crate mapped changes, nothing but this crate holds
        // an address in it, and `old` is live.
        let answer =
            unsafe { VirtualProtect(PAGE as *mut c_void, PAGE_SIZE, protection, &raw mut old) };
        assert_ne!(answer, 0, "VirtualProtect refused the page");
    }

    /// A fault on the page: counts it, opens the page and sets the trap flag, so that the
    /// one instruction runs and traps. The trap: closes the page, clears the flag. Any
    /// other exception is passed on.
    unsafe extern "system" fn on_exception(pointers: *mut ExceptionPointers) -> i32 {
        // SAFETY: the system hands a vectored handler live pointers to both records,
        // whose fields sit at these offsets on this host.
        unsafe {
            let record = (*pointers).record;
            let flags = (*pointers).context.add(FLAGS_AT).cast::<u32>();
            match record.cast::<u32>().read() {
                ACCESS_VIOLATION => {
                    let address = record.add(FAULT_ADDRESS_AT).cast::<usize>().read();
                    if !(PAGE..PAGE + PAGE_SIZE).contains(&address) {
                        return CONTINUE_SEARCH;
                    }
                    FAULTS.fetch_add(1, Ordering::SeqCst);
                    STEPPING.store(true, Ordering::SeqCst);
                    set_open(true);
                    *flags |= TRAP_FLAG;
                    CONTINUE_EXECUTION
                }
                SINGLE_STEP if STEPPING.load(Ordering::SeqCst) => {
                    STEPPING.store(false, Ordering::SeqCst);
                    set_open(false);
                    *flags &= !TRAP_FLAG;
                    CONTINUE_EXECUTION
                }
                _ => CONTINUE_SEARCH,
            }
        }
    }

    pub(super) fn arm() -> Armed {
        // SAFETY: a fresh mapping, refused rather than laid over anything.
        let page = unsafe {
            VirtualAlloc(
                PAGE as *mut c_void,
                PAGE_SIZE,
                MEM_RESERVE_COMMIT,
                PAGE_READWRITE,
            )
        };
        assert_eq!(page.addr(), PAGE, "the page's fixed address is taken");
        // SAFETY: `on_exception` touches only atomics, the page's protection and the
        // records it is handed.
        Armed(unsafe { AddVectoredExceptionHandler(1, on_exception) })
    }

    pub(super) fn disarm(armed: &Armed) {
        // SAFETY: the handle came from the call that added the handler, and nothing is
        // left holding an address in the page.
        unsafe {
            RemoveVectoredExceptionHandler(armed.0);
            VirtualFree(PAGE as *mut c_void, 0, MEM_RELEASE);
        }
    }
}
