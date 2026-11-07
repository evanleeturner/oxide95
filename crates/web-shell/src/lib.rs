// SPDX-License-Identifier: MIT
//! An old Windows program sits in a loop and waits for its next message, and a web
//! page's main thread must never wait. This crate is a small test that the way round
//! that works, before any real program depends on it. A white square moves across a
//! canvas in a real browser, drawn by a Worker (a background thread of the page) that
//! never stops looping. Holding the up arrow lifts the square, and that key reaches the
//! Worker through shared memory (a block of memory both threads can see). It holds no
//! program's file and no program's code.
//!
//! A button on the page pauses the square, and it starts paused when the browser asks
//! for reduced motion. The pictures go on either way.
//!
//! # How it fits together
//!
//! The wasm is the compiled program (`WebAssembly`, the form a browser runs). All of it
//! sits on the Worker.
//!
//! | Part | Lives in | Does |
//! | --- | --- | --- |
//! | The page's thread | `page/track.mjs` | Writes keys into the shared words, shows each picture, keeps its own timer |
//! | The Worker | `page/track-worker.mjs` | Takes one message, then runs the wasm, which never returns |
//! | The wasm | `page/track.rs` | The thin export: `run` loops and sits on an import between turns |
//! | One turn | [`track`] | Keys to `window`, a key poll through `window_input`, a picture through `draw`, a file through `logical_disc` |
//! | The picture | [`mark`] | The square's place, and its pixels |
//! | The words | [`crossing`], `page/crossing.mjs` | The numbers both sides share; a test holds the two equal |
//! | The key map | `page/keys.mjs` | The one map from a browser key to a Windows virtual key number |
//!
//! # The wasm's imports
//!
//! An import is a function the wasm asks the page's script to give it. Every import it
//! takes is here, with why. Nothing else may be imported: the browser run reads the
//! built file against this table, both ways.
//!
//! | Import | Why |
//! | --- | --- |
//! | `env.word_load` | Reads a shared word: wasm's own memory is private on a stable toolchain, so the crossing lies outside it |
//! | `env.word_store` | Writes a shared word, for the same reason |
//! | `env.sit` | Waits between turns by `Atomics.wait`: wasm's own wait instruction needs a nightly toolchain |
//! | `env.now` | Gives the time since the page began, which this target has no other way to read |
//! | `env.present` | Hands the finished picture to WebGL 2, which only script can reach |
//! | `env.file_size` | Gives the picked file's size, or -1 with none |
//! | `env.read_at` | Reads a part of the picked file by `FileReaderSync`, which only script can reach |
//! | `env.line` | Hands a finished log line to the Worker's logger, the one route to the console |
//!
//! # What it costs
//!
//! - **Every crossing is a call or a copy, never a shared pointer.** A stable toolchain
//!   refuses a wasm whose own memory is shared, because its prebuilt standard library
//!   was not built for it. So each shared word is a call, and sound's later ring will
//!   copy.
//! - **Every picture is one message from the Worker to the page.** A blocked Worker's
//!   transferred canvas showed no frame on headless Chromium 148 with `SwiftShader`
//!   (software drawing), while a bitmap handed over each frame did. That result is for
//!   that browser alone.
//! - **A file is picked before Start.** A Worker that never yields takes no later
//!   message. A launcher's order is the same: pick, start, run.
//!
//! # Running it
//!
//! Build the wasm, then run the browser checks and the page's own tests.
//!
//! ```bash
//! CARGO_INCREMENTAL=0 cargo build -p web-shell --example page_track \
//!   --target wasm32-unknown-unknown --release
//! PLAYWRIGHT=<playwright's index.mjs> AXE=<axe.min.js> SCRATCH=<a folder> \
//!   node crates/web-shell/page/track-browser.mjs \
//!   target/wasm32-unknown-unknown/release/examples/page_track.wasm output.json
//! node --test crates/web-shell/page/
//! ```
//!
//! Only Chromium has been run, with software drawing, no graphics card. Accessibility is
//! held to the automated floor alone: no conformance is claimed, and a person's keyboard
//! and screen-reader walk is still owed.

pub mod crossing;
pub mod mark;
pub mod track;
