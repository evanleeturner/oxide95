// SPDX-License-Identifier: MIT
//! `window::user32` and `window_input::user32` name USER32 and link nothing themselves, so the
//! library goes last, after the crates that name it.
fn main() {
    println!("cargo:rustc-link-arg-bins=-luser32");
}
