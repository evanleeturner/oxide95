// SPDX-License-Identifier: MIT
//! A test-only log sink, as a rows crate's but with no logline adapter, so
//! the crate names no game: it keeps each DEBUG line on the thread that
//! logged it, rendered `event k=v` from the record's own message and
//! pairs, so a test reads the lines its own calls wrote while other tests
//! run beside it.
//!
//! Call: [`debug_lines`] `(call) -> Vec<String>`, the DEBUG lines `call`
//! logged on this thread, in order.
use std::cell::RefCell;
use std::fmt::Write;
use std::sync::Once;

use log::kv::{Error, Key, Value, VisitSource};
use log::{Level, LevelFilter, Log, Metadata, Record};

struct ThreadSink;

thread_local! {
    static LINES: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
}

/// A line being rendered: the message, then each pair as ` k=v`.
struct Line(String);

impl<'kvs> VisitSource<'kvs> for Line {
    fn visit_pair(&mut self, key: Key<'kvs>, value: Value<'kvs>) -> Result<(), Error> {
        write!(self.0, " {key}={value}").expect("a String takes any pair");
        Ok(())
    }
}

impl Log for ThreadSink {
    fn enabled(&self, metadata: &Metadata<'_>) -> bool {
        metadata.level() == Level::Debug
    }

    fn log(&self, record: &Record<'_>) {
        if self.enabled(record.metadata()) {
            let mut line = Line(record.args().to_string());
            record
                .key_values()
                .visit(&mut line)
                .expect("a record's pairs read");
            LINES.with(|lines| lines.borrow_mut().push(line.0));
        }
    }

    fn flush(&self) {}
}

static SINK: ThreadSink = ThreadSink;
static INSTALL: Once = Once::new();

/// Runs `call` with DEBUG on and returns the DEBUG lines it logged on
/// this thread. Installs the sink once for the whole test binary.
pub(crate) fn debug_lines(call: impl FnOnce()) -> Vec<String> {
    INSTALL.call_once(|| {
        log::set_logger(&SINK).expect("invariant: only this function sets the test logger");
        log::set_max_level(LevelFilter::Debug);
    });
    LINES.with(|lines| lines.borrow_mut().clear());
    call();
    LINES.with(|lines| lines.borrow_mut().drain(..).collect())
}
