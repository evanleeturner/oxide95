// SPDX-License-Identifier: MIT
//! The crate against a judge. Each question goes to [`Host`] through the
//! fronts; on a 32-bit Windows target it goes again to that target's own
//! `kernel32`, by calls declared here, with none of this crate's code on that
//! path. Every answer is printed, so a script can score a run and see a row go
//! missing: `cargo test -p kernel_threads checks -- --nocapture`.
//!
//! A thread held out is read, never timed. `enter` has no way to try, so a
//! second thread says it has arrived and calls `enter`; the first gives up the
//! processor and reads whether it got in, [`HELD_READS`] times. The same thread
//! at a free lock gives the control: how many reads it takes to get in when
//! nothing holds it out. Where the system lets a program read a thread's state,
//! that is printed too. Those measures are printed and not asserted, as a busy
//! computer moves them: the script scores them.
//!
//! A wait that must end has [`MUST_COME`] as its limit, so a lost wake-up is a
//! wrong answer and not a hang.
//!
//! Not asked, as Win32 leaves them undefined: a leave by a thread that holds
//! nothing, an entry into a lock never made, a delete while held.
//!
//! `TH1`: the lock: a second thread held out while one holds it, let in after.
//! `TH2`: the slots: each thread's own value, and a freed slot refused.
//! `TH3`: the counters: each change made whole while threads race.
use crate::host::Host;
use crate::table::{Calls, NO_SLOT, Row, SLOTS};
use crate::{ThreadCalls, Threads};
use std::fmt::Display;
use std::sync::atomic::{AtomicBool, AtomicI32, AtomicU32, Ordering};
use std::sync::{Arc, Barrier, Mutex, PoisonError, mpsc};
use std::thread;
use std::time::{Duration, Instant};

/// How long a thing that must happen may take before it is a wrong answer.
const MUST_COME: Duration = Duration::from_secs(30);
/// How many reads a held-out thread must stay out for.
const HELD_READS: u32 = 200_000;
/// How many times the control is measured.
const CONTROL_ROUNDS: u32 = 16;
/// The rounds each thread takes through a lock.
const LOCK_ROUNDS: u32 = 5_000;
/// The rounds each of 2 threads takes on a counter.
const COUNTER_ROUNDS: i32 = 20_000;
/// How many reads a thread that has left waits for another to take its turn.
const PATIENCE: u32 = 2_000;
/// An index no slot table holds.
const BAD_INDEX: u32 = 100_000;
/// The last error a call that keeps it must leave standing.
const KEPT: u32 = 0x1234;
/// The refused argument, `ERROR_INVALID_PARAMETER`.
const BAD_ARGUMENT: u32 = 87;
/// The lock the hold-out uses.
const HELD_LOCK: u32 = 0;
/// The lock the rounds use.
const ROUNDS_LOCK: u32 = 1;

const ROW: Row = Row {
    file: "check",
    calls: Calls {
        current_thread_id: "id",
        initialize: "initialize",
        enter: "enter",
        leave: "leave",
        delete: "delete",
        slot_alloc: "slot_alloc",
        slot_free: "slot_free",
        slot_get: "slot_get",
        slot_set: "slot_set",
        increment: Some("increment"),
        decrement: Some("decrement"),
    },
    bad_argument: BAD_ARGUMENT,
};

mod want;
use want::{COUNTER_WANT, FREED_WANT, LOCK_WANT, OUT_WANT, SLOTS_WANT};

mod lines;

#[cfg(all(windows, target_arch = "x86"))]
#[allow(unsafe_code, reason = "the judge is the target's own calls, by hand")]
mod judge;

/// What each check asks, of the crate and of the judge alike.
trait Back {
    fn id(&self) -> u32;
    fn initialize(&self, lock: u32);
    fn enter(&self, lock: u32);
    fn leave(&self, lock: u32);
    fn delete(&self, lock: u32);
    fn slot_alloc(&self) -> u32;
    fn slot_free(&self, slot: u32) -> u32;
    fn slot_get(&self, slot: u32) -> u32;
    fn slot_set(&self, slot: u32, value: u32) -> u32;
    fn last_error(&self) -> u32;
    fn set_last_error(&self, code: u32);
    fn increment(&self, counter: &AtomicI32) -> i32;
    fn decrement(&self, counter: &AtomicI32) -> i32;
}

type Shared = Arc<dyn Back + Send + Sync>;

impl Back for ThreadCalls<Host> {
    fn id(&self) -> u32 {
        self.current_thread_id(0)
    }
    fn initialize(&self, lock: u32) {
        ThreadCalls::initialize(self, 0, lock);
    }
    fn enter(&self, lock: u32) {
        ThreadCalls::enter(self, 0, lock);
    }
    fn leave(&self, lock: u32) {
        ThreadCalls::leave(self, 0, lock);
    }
    fn delete(&self, lock: u32) {
        ThreadCalls::delete(self, 0, lock);
    }
    fn slot_alloc(&self) -> u32 {
        ThreadCalls::slot_alloc(self, 0)
    }
    fn slot_free(&self, slot: u32) -> u32 {
        ThreadCalls::slot_free(self, 0, slot)
    }
    fn slot_get(&self, slot: u32) -> u32 {
        ThreadCalls::slot_get(self, 0, slot)
    }
    fn slot_set(&self, slot: u32, value: u32) -> u32 {
        ThreadCalls::slot_set(self, 0, slot, value)
    }
    fn last_error(&self) -> u32 {
        self.threads().last_error()
    }
    fn set_last_error(&self, code: u32) {
        Threads::set_last_error(self.threads(), code);
    }
    fn increment(&self, counter: &AtomicI32) -> i32 {
        ThreadCalls::increment(self, 0, counter)
    }
    fn decrement(&self, counter: &AtomicI32) -> i32 {
        ThreadCalls::decrement(self, 0, counter)
    }
}

/// One back's answers: rows are judged, measures are the check's own reading
/// of itself.
#[derive(Default)]
struct Answers {
    rows: Vec<(&'static str, String)>,
    measures: Vec<(&'static str, String)>,
}

impl Answers {
    fn row(&mut self, name: &'static str, answer: impl Display) {
        self.rows.push((name, answer.to_string()));
    }

    fn measure(&mut self, name: &'static str, value: impl Display) {
        self.measures.push((name, value.to_string()));
    }

    fn print(&self, check: &str, back: &str) {
        for (name, answer) in &self.rows {
            println!("CHECK {check} {back} {name} = {answer}");
        }
        for (name, value) in &self.measures {
            println!("MEASURE {check} {back} {name} = {value}");
        }
    }

    fn named(&self) -> Vec<(&str, &str)> {
        self.rows
            .iter()
            .map(|(name, answer)| (*name, answer.as_str()))
            .collect()
    }
}

/// The checks run one at a time: a row's reads should not share the processors
/// with another check's rounds.
static ONE_AT_A_TIME: Mutex<()> = Mutex::new(());

fn the_crate() -> Shared {
    Arc::new(ThreadCalls::new(&ROW, Host::new()))
}

/// Asks the crate and, where there is one, the judge; prints all of it first,
/// so a wrong answer leaves every row to be read.
fn judged(check: &str, want: &[(&str, &str)], ask: fn(&Shared) -> Answers) {
    let _one = ONE_AT_A_TIME.lock().unwrap_or_else(PoisonError::into_inner);
    let answers = ask(&the_crate());
    answers.print(check, "crate");
    #[cfg(all(windows, target_arch = "x86"))]
    {
        let judge: Shared = Arc::new(judge::Kernel32::new());
        let judged = ask(&judge);
        judged.print(check, "judge");
        assert_eq!(judged.named(), want, "the judge left the table, {check}");
    }
    assert_eq!(answers.named(), want, "the crate left the table, {check}");
}

/// The last error cleared first, so an old one reads as nothing.
fn both(back: &Shared, call: impl FnOnce(&Shared) -> u32) -> String {
    back.set_last_error(0);
    let answer = call(back);
    format!("{answer} error {}", back.last_error())
}

/// The last error set first, to read whether the call kept it.
fn kept(back: &Shared, call: impl FnOnce(&Shared) -> u32) -> String {
    back.set_last_error(KEPT);
    let answer = call(back);
    format!("{answer} error {}", back.last_error())
}

/// Reads `done` until it holds; `Some` of the reads it took, `None` at the
/// limit.
fn until(done: impl Fn() -> bool) -> Option<u32> {
    let end = Instant::now() + MUST_COME;
    let mut reads = 0;
    loop {
        if done() {
            return Some(reads);
        }
        if Instant::now() >= end {
            return None;
        }
        reads += 1;
        thread::yield_now();
    }
}

/// Where a thread's state is read. Measured under Wine 9.0: 2 threads read the
/// same number there and neither's state, so that host prints `same` and `unread`,
/// and the reads against the control are all it has.
const PROC: &str = if cfg!(windows) { "Z:/proc" } else { "/proc" };

/// The calling thread's number on a system with `/proc`.
fn own_task() -> Option<String> {
    let stat = std::fs::read_to_string(format!("{PROC}/thread-self/stat")).ok()?;
    stat.split(' ').next().map(str::to_owned)
}

/// A thread's program name and state as `/proc` has them, or `unread`.
fn task_state(task: Option<&str>) -> String {
    let read = || {
        let task = task?;
        let stat = std::fs::read_to_string(format!("{PROC}/self/task/{task}/stat")).ok()?;
        let (open, close) = (stat.find('(')?, stat.rfind(')')?);
        let state = stat[close + 1..].trim_start().chars().next()?;
        Some(format!("{} {state}", &stat[open + 1..close]))
    };
    read().unwrap_or_else(|| "unread".to_owned())
}

/// One thread at a lock, and what it has said.
#[derive(Default)]
struct Door {
    id: AtomicU32,
    task: Mutex<Option<String>>,
    arrived: AtomicBool,
    inside: AtomicBool,
    may_leave: AtomicBool,
    left: AtomicBool,
}

impl Door {
    fn task(&self) -> Option<String> {
        self.task
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    /// True if it stays out for all the reads.
    fn stays_out(&self) -> bool {
        for _ in 0..HELD_READS {
            thread::yield_now();
            if self.inside.load(Ordering::SeqCst) {
                return false;
            }
        }
        true
    }

    fn comes_in(&self) -> bool {
        until(|| self.inside.load(Ordering::SeqCst)).is_some()
    }

    /// Lets it go, and waits to see it gone.
    fn finishes(&self) -> bool {
        self.may_leave.store(true, Ordering::SeqCst);
        until(|| self.left.load(Ordering::SeqCst)).is_some()
    }
}

/// Starts a thread that says it has arrived, enters, says so, holds until let
/// go, and leaves. It is never joined: one stuck is a wrong answer.
fn knock(back: &Shared, lock: u32) -> Arc<Door> {
    let door = Arc::new(Door::default());
    let (back, shared) = (Arc::clone(back), Arc::clone(&door));
    thread::spawn(move || {
        shared.id.store(back.id(), Ordering::SeqCst);
        *shared.task.lock().unwrap_or_else(PoisonError::into_inner) = own_task();
        shared.arrived.store(true, Ordering::SeqCst);
        back.enter(lock);
        shared.inside.store(true, Ordering::SeqCst);
        let let_go = until(|| shared.may_leave.load(Ordering::SeqCst)).is_some();
        back.leave(lock);
        shared.left.store(let_go, Ordering::SeqCst);
    });
    door
}

/// A thread that has arrived, or the limit.
fn knocked(back: &Shared, lock: u32) -> Option<Arc<Door>> {
    let door = knock(back, lock);
    until(|| door.arrived.load(Ordering::SeqCst))?;
    Some(door)
}

/// A free lock: does a thread get in, and how many reads does that take.
fn control(back: &Shared, answers: &mut Answers) -> bool {
    let mut worst = 0;
    for _ in 0..CONTROL_ROUNDS {
        let Some(door) = knocked(back, HELD_LOCK) else {
            answers.row("free.other_in", "out");
            return false;
        };
        let Some(reads) = until(|| door.inside.load(Ordering::SeqCst)) else {
            answers.row("free.other_in", "out");
            return false;
        };
        worst = worst.max(reads);
        if !door.finishes() {
            answers.row("free.other_in", "stuck");
            return false;
        }
    }
    answers.row("free.other_in", "in");
    answers.measure("control.worst_reads", worst);
    answers.measure("held.reads", HELD_READS);
    true
}

fn in_or_out(inside: bool) -> &'static str {
    if inside { "in" } else { "out" }
}

/// A thread at a lock nothing holds; it is let go at once.
fn walks_in(back: &Shared, lock: u32) -> bool {
    let door = knock(back, lock);
    door.comes_in() && door.finishes()
}

/// `threads` threads through one lock, each with a count only the lock guards.
/// The count's read and write are apart, so threads that overlap lose counts.
/// Each waits after leaving until another has been in: a thread that leaves and
/// enters again at once can keep a lock for all its rounds, with nothing
/// contending.
fn rounds(back: &Shared, answers: &mut Answers, threads: u32, names: [&'static str; 3]) {
    #[derive(Default)]
    struct Tally {
        count: AtomicU32,
        inside: AtomicU32,
        overlap: AtomicU32,
        contended: AtomicU32,
        done: AtomicU32,
        last: AtomicU32,
    }
    let tally = Arc::new(Tally::default());
    let start = Arc::new(Barrier::new(
        usize::try_from(threads).expect("a few threads"),
    ));
    back.initialize(ROUNDS_LOCK);
    for which in 1..=threads {
        let (back, tally, start) = (Arc::clone(back), Arc::clone(&tally), Arc::clone(&start));
        thread::spawn(move || {
            start.wait();
            for _ in 0..LOCK_ROUNDS {
                if tally.inside.load(Ordering::SeqCst) > 0 {
                    tally.contended.fetch_add(1, Ordering::SeqCst);
                }
                back.enter(ROUNDS_LOCK);
                if tally.inside.fetch_add(1, Ordering::SeqCst) > 0 {
                    tally.overlap.fetch_add(1, Ordering::SeqCst);
                }
                tally.last.store(which, Ordering::SeqCst);
                let count = tally.count.load(Ordering::SeqCst);
                thread::yield_now();
                tally.count.store(count + 1, Ordering::SeqCst);
                tally.inside.fetch_sub(1, Ordering::SeqCst);
                back.leave(ROUNDS_LOCK);
                let mut patience = PATIENCE;
                while patience > 0
                    && tally.last.load(Ordering::SeqCst) == which
                    && tally.done.load(Ordering::SeqCst) == 0
                {
                    patience -= 1;
                    thread::yield_now();
                }
            }
            tally.done.fetch_add(1, Ordering::SeqCst);
        });
    }
    let finished = until(|| tally.done.load(Ordering::SeqCst) == threads).is_some();
    let count = tally.count.load(Ordering::SeqCst);
    if !finished {
        answers.row(names[0], "stuck");
    } else if count == threads * LOCK_ROUNDS {
        answers.row(names[0], "exact");
    } else {
        answers.row(names[0], count);
    }
    answers.row(names[1], tally.overlap.load(Ordering::SeqCst));
    answers.measure(names[2], tally.contended.load(Ordering::SeqCst));
    if finished {
        back.delete(ROUNDS_LOCK);
    }
}

/// The lock: a thread's number, a thread held out and let in, a lock handed on,
/// a lock made again, and threads taking turns.
fn lock_answers(back: &Shared) -> Answers {
    let mut answers = Answers::default();
    let me = back.id();
    answers.row("id.nonzero", me != 0);
    answers.row("id.stable", back.id() == me);
    back.initialize(HELD_LOCK);
    if !control(back, &mut answers) {
        return answers;
    }

    back.enter(HELD_LOCK);
    back.enter(HELD_LOCK);
    answers.row("twice.returned", true);
    let Some(second) = knocked(back, HELD_LOCK) else {
        return answers;
    };
    answers.row("id.two_differ", second.id.load(Ordering::SeqCst) != me);
    let (mine, theirs) = (own_task(), second.task());
    answers.measure(
        "os.two_threads",
        match (&mine, &theirs) {
            (Some(mine), Some(theirs)) if mine != theirs => "differ",
            (Some(_), Some(_)) => "same",
            _ => "unread",
        },
    );
    answers.row("held.second.inside", u8::from(!second.stays_out()));
    answers.measure("held.os", task_state(theirs.as_deref()));
    back.leave(HELD_LOCK);
    answers.row("one_left.second.inside", u8::from(!second.stays_out()));
    answers.measure("one_left.os", task_state(theirs.as_deref()));
    back.leave(HELD_LOCK);
    answers.row("both_left.second.inside", u8::from(second.comes_in()));

    let Some(third) = knocked(back, HELD_LOCK) else {
        return answers;
    };
    answers.row("handed.third.inside", u8::from(!third.stays_out()));
    let second_gone = second.finishes();
    answers.row(
        "handed.third.in_on_release",
        u8::from(second_gone && third.comes_in()),
    );
    if !third.finishes() {
        return answers;
    }

    answers.row("left.other_in", in_or_out(walks_in(back, HELD_LOCK)));
    back.delete(HELD_LOCK);
    back.initialize(HELD_LOCK);
    answers.row("reborn.other_in", in_or_out(walks_in(back, HELD_LOCK)));
    back.delete(HELD_LOCK);

    rounds(
        back,
        &mut answers,
        2,
        ["rounds.2.count", "rounds.2.overlap", "rounds.2.contended"],
    );
    rounds(
        back,
        &mut answers,
        4,
        ["rounds.4.count", "rounds.4.overlap", "rounds.4.contended"],
    );
    answers
}

type Job = Box<dyn FnOnce() + Send>;

/// A second thread that lives across questions, so its slots keep their values
/// between them.
struct Second(mpsc::Sender<Job>);

impl Second {
    fn start() -> Self {
        let (send, jobs) = mpsc::channel::<Job>();
        thread::spawn(move || {
            for job in jobs {
                job();
            }
        });
        Self(send)
    }

    fn ask<T: Send + 'static>(&self, job: impl FnOnce() -> T + Send + 'static) -> T {
        let (answer, answered) = mpsc::channel();
        self.send(move || {
            answer.send(job()).ok();
        });
        answered
            .recv_timeout(MUST_COME)
            .expect("the second thread answers")
    }

    fn send(&self, job: impl FnOnce() + Send + 'static) {
        self.0
            .send(Box::new(job))
            .expect("the second thread listens");
    }

    /// Its answer and its last error, as [`both`] reads them on it.
    fn both(&self, back: &Shared, call: impl FnOnce(&Shared) -> u32 + Send + 'static) -> String {
        let back = Arc::clone(back);
        self.ask(move || both(&back, call))
    }
}

/// A second thread's refusal, read beside the first thread's last error. No
/// channel between the set and the read: its wait could leave an error of its
/// own on a target where the last error is the system's.
fn errors_are_a_threads(back: &Shared, second: &Second, slot_answers: &mut Answers) {
    let (go, done) = (
        Arc::new(AtomicBool::new(false)),
        Arc::new(AtomicBool::new(false)),
    );
    let (their_error, read) = mpsc::channel();
    {
        let (back, go, done) = (Arc::clone(back), Arc::clone(&go), Arc::clone(&done));
        second.send(move || {
            while !go.load(Ordering::SeqCst) {
                std::hint::spin_loop();
            }
            their_error
                .send(both(&back, |back| back.slot_free(BAD_INDEX)))
                .ok();
            done.store(true, Ordering::SeqCst);
        });
    }
    back.set_last_error(KEPT);
    go.store(true, Ordering::SeqCst);
    while !done.load(Ordering::SeqCst) {
        std::hint::spin_loop();
    }
    let mine = back.last_error();
    let theirs = read
        .recv_timeout(MUST_COME)
        .expect("the second thread said");
    slot_answers.row("error.second", theirs);
    slot_answers.row("error.first", mine);
}

/// The slots: unset, set, across 2 threads, freed and taken again, and which
/// calls keep the last error.
fn slot_answers(back: &Shared) -> Answers {
    let mut answers = Answers::default();
    let second = Second::start();
    let slot = back.slot_alloc();
    answers.row("slot.taken", slot != NO_SLOT);
    answers.measure("first.index", slot);
    answers.row("first.unset", both(back, |back| back.slot_get(slot)));
    answers.row("first.set", both(back, |back| back.slot_set(slot, 7)));
    answers.row("first.get", both(back, |back| back.slot_get(slot)));
    back.slot_set(slot, u32::MAX);
    answers.row("value.full_width", both(back, |back| back.slot_get(slot)));
    back.slot_set(slot, 0);
    answers.row("value.zero", both(back, |back| back.slot_get(slot)));
    answers.row("keeps.get", kept(back, |back| back.slot_get(slot)));
    answers.row("keeps.set", kept(back, |back| back.slot_set(slot, 7)));

    answers.row(
        "cross.b.unset",
        second.both(back, move |back| back.slot_get(slot)),
    );
    answers.row(
        "cross.b.set",
        second.both(back, move |back| back.slot_set(slot, 9)),
    );
    answers.row(
        "cross.b.get",
        second.both(back, move |back| back.slot_get(slot)),
    );
    answers.row("cross.a.get", both(back, |back| back.slot_get(slot)));
    errors_are_a_threads(back, &second, &mut answers);

    answers.row("keeps.free", kept(back, |back| back.slot_free(slot)));
    let again = back.slot_alloc();
    answers.row(
        "again.index",
        if again == slot { "same" } else { "another" },
    );
    answers.row("again.a.unset", both(back, |back| back.slot_get(again)));
    answers.row(
        "again.b.unset",
        second.both(back, move |back| back.slot_get(again)),
    );
    back.slot_free(again);

    answers.row("bad.get", both(back, |back| back.slot_get(BAD_INDEX)));
    answers.row("bad.set", both(back, |back| back.slot_set(BAD_INDEX, 7)));
    answers.row("bad.free", both(back, |back| back.slot_free(BAD_INDEX)));
    answers
}

/// An index under the table's size that was taken and is free now.
fn freed_answers(back: &Shared) -> Answers {
    let mut answers = Answers::default();
    let slot = back.slot_alloc();
    back.slot_free(slot);
    answers.row("freed.get", both(back, |back| back.slot_get(slot)));
    answers.row("freed.set", both(back, |back| back.slot_set(slot, 7)));
    answers.row("freed.free", both(back, |back| back.slot_free(slot)));
    answers
}

/// The counters at each edge, each answer beside what the counter then holds,
/// then 2 threads on one counter.
fn counter_answers(back: &Shared) -> Answers {
    type Step = fn(&(dyn Back + Send + Sync), &AtomicI32) -> i32;
    let mut answers = Answers::default();
    let up: Step = |back, counter| back.increment(counter);
    let down: Step = |back, counter| back.decrement(counter);
    let steps: [(&'static str, i32, Step); 7] = [
        ("inc.5", 5, up),
        ("dec.6", 6, down),
        ("inc.max", i32::MAX, up),
        ("dec.min", i32::MIN, down),
        ("inc.m1", -1, up),
        ("dec.1", 1, down),
        ("dec.0", 0, down),
    ];
    for (name, start, step) in steps {
        let counter = AtomicI32::new(start);
        let answer = step(back.as_ref(), &counter);
        answers.row(
            name,
            format!("{answer} held {}", counter.load(Ordering::SeqCst)),
        );
    }

    let counter = Arc::new(AtomicI32::new(0));
    let start = Arc::new(Barrier::new(2));
    let threads: Vec<_> = (0..2)
        .map(|_| {
            let (back, counter, start) =
                (Arc::clone(back), Arc::clone(&counter), Arc::clone(&start));
            thread::spawn(move || {
                start.wait();
                (0..COUNTER_ROUNDS)
                    .map(|round| {
                        if round % 64 == 0 {
                            thread::yield_now();
                        }
                        back.increment(&counter)
                    })
                    .collect::<Vec<i32>>()
            })
        })
        .collect();
    let told: Vec<Vec<i32>> = threads
        .into_iter()
        .map(|thread| thread.join().expect("a counter's thread ends"))
        .collect();
    let interleaved = told
        .iter()
        .flat_map(|answers| answers.windows(2))
        .filter(|pair| pair[1] != pair[0] + 1)
        .count();
    let mut all: Vec<i32> = told.concat();
    all.sort_unstable();
    let each_once = all.iter().copied().eq(1..=2 * COUNTER_ROUNDS);
    answers.row(
        "rounds.answers",
        if each_once {
            "each once"
        } else {
            "not each once"
        },
    );
    answers.row(
        "rounds.sum",
        if counter.load(Ordering::SeqCst) == 2 * COUNTER_ROUNDS {
            "exact"
        } else {
            "lost"
        },
    );
    answers.measure("rounds.interleaved", interleaved);
    answers
}

#[test]
fn th1_the_lock_against_the_judge() {
    judged("TH1", LOCK_WANT, lock_answers);
}

#[test]
fn th2_the_slots_against_the_judge() {
    judged("TH2", SLOTS_WANT, slot_answers);
}

#[test]
#[ignore = "the crate refuses a read and a write of a freed slot, and Wine's own calls answer both"]
fn th2_a_freed_slot_against_the_judge() {
    judged("TH2", FREED_WANT, freed_answers);
}

/// The crate's alone: the judge's count is another system's, and running it out
/// here would refuse the test program's own threads their slots.
#[test]
fn th2_the_slots_run_out() {
    let _one = ONE_AT_A_TIME.lock().unwrap_or_else(PoisonError::into_inner);
    let back = the_crate();
    let mut answers = Answers::default();
    let mut count = 0;
    let mut last = NO_SLOT;
    let mut answer = back.slot_alloc();
    while answer != NO_SLOT && count <= SLOTS {
        count += 1;
        last = answer;
        answer = back.slot_alloc();
    }
    answers.row("out.count", count);
    answers.row("out.answer", answer);
    back.slot_free(last);
    answers.row(
        "out.one_freed",
        if back.slot_alloc() == last {
            "comes back"
        } else {
            "lost"
        },
    );
    answers.print("TH2", "crate");
    assert_eq!(answers.named(), OUT_WANT);
}

#[test]
fn th3_the_counters_against_the_judge() {
    judged("TH3", COUNTER_WANT, counter_answers);
}
