// SPDX-License-Identifier: MIT
//! This workspace's logbook against its own log calls.

use std::path::Path;

#[test]
fn every_log_call_is_cataloged_and_every_entry_is_used() {
    oxide_logline::fence::check(&Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")).enforce();
}
