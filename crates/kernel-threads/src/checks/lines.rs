// SPDX-License-Identifier: MIT
//! The line each call writes, against the table harvested from the first clean run.
use super::want::LINES_WANT;
use super::{Answers, ONE_AT_A_TIME, ROW};
use crate::ThreadCalls;
use crate::host::Host;
use crate::table::{Calls, Row};
use std::sync::atomic::AtomicI32;
use std::sync::{Mutex, PoisonError};

/// The address the lines' test calls from, so its lines can be told apart.
const LINE_FROM: u32 = 0xC2;

const NO_COUNTER: Row = Row {
    calls: Calls {
        increment: None,
        decrement: None,
        ..ROW.calls
    },
    ..ROW
};

/// One line a call, caught. A line is told apart by its `from`.
static LINES: Mutex<Vec<String>> = Mutex::new(Vec::new());

struct Catch;

#[derive(Default)]
struct Fields(Vec<(String, String)>);

impl<'kvs> log::kv::VisitSource<'kvs> for Fields {
    fn visit_pair(
        &mut self,
        key: log::kv::Key<'kvs>,
        value: log::kv::Value<'kvs>,
    ) -> Result<(), log::kv::Error> {
        self.0.push((key.to_string(), value.to_string()));
        Ok(())
    }
}

impl log::Log for Catch {
    fn enabled(&self, _: &log::Metadata<'_>) -> bool {
        true
    }

    fn log(&self, record: &log::Record<'_>) {
        let mut fields = Fields::default();
        record.key_values().visit(&mut fields).ok();
        let wanted = format!("{LINE_FROM:x}");
        if fields
            .0
            .iter()
            .any(|(key, value)| key == "from" && *value == wanted)
        {
            let shown: Vec<String> = fields
                .0
                .iter()
                .map(|(key, value)| format!("{key}={value}"))
                .collect();
            LINES
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .push(format!("{} {}", record.args(), shown.join(" ")));
        }
    }

    fn flush(&self) {}
}

static CATCH: Catch = Catch;

#[test]
fn the_lines_each_call_writes() {
    let _one = ONE_AT_A_TIME.lock().unwrap_or_else(PoisonError::into_inner);
    log::set_logger(&CATCH).expect("this test alone sets the logger");
    log::set_max_level(log::LevelFilter::Debug);
    let counter = AtomicI32::new(5);
    let calls = ThreadCalls::new(&ROW, Host::new());
    let id = calls.current_thread_id(LINE_FROM);
    calls.initialize(LINE_FROM, 0x10);
    calls.enter(LINE_FROM, 0x10);
    calls.leave(LINE_FROM, 0x10);
    calls.delete(LINE_FROM, 0x10);
    let slot = calls.slot_alloc(LINE_FROM);
    let _ = calls.slot_set(LINE_FROM, slot, 7);
    let _ = calls.slot_get(LINE_FROM, slot);
    let _ = calls.slot_free(LINE_FROM, slot);
    let _ = calls.slot_free(LINE_FROM, slot);
    let _ = calls.increment(LINE_FROM, &counter);
    let _ = calls.decrement(LINE_FROM, &counter);
    let no_counter = ThreadCalls::new(&NO_COUNTER, Host::new());
    let _ = no_counter.increment(LINE_FROM, &counter);
    let _ = no_counter.decrement(LINE_FROM, &counter);
    let mut answers = Answers::default();
    let lines = LINES.lock().unwrap_or_else(PoisonError::into_inner);
    // A thread's number is its order of asking, so it is read here and not kept.
    let own = format!("v={id:x} ");
    for (at, line) in lines.iter().enumerate() {
        let name = LINES_WANT.get(at).map_or("line.more", |want| want.0);
        answers.row(name, line.replacen(&own, "v=<id> ", 1));
    }
    answers.print("LINES", "crate");
    assert_eq!(answers.named(), LINES_WANT);
}
