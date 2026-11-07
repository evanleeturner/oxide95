// SPDX-License-Identifier: MIT
//! How `oxide_trace` prints a value, against a table worked by hand, through `__shown!`, so
//! that the route to `?` is judged with the rest; and what `--cfg oxide_trace_strict`
//! refuses, judged by the compiler.
//!
//! A red here names `oxide_trace`, not a program built on it.
#![cfg(any(
    all(target_os = "linux", target_arch = "x86_64"),
    all(windows, target_arch = "x86")
))]

/// A value as a log field shows it.
macro_rules! show {
    ($value:expr) => {{
        let value = $value;
        format!("{}", oxide_trace::__shown!(value))
    }};
}

/// Each function pointer type prints `0x401234`, alone or in a `Some`; its `None` prints 0.
macro_rules! address_of {
    ($($pointer:ty),* $(,)?) => {
        $({
            // SAFETY: a non-null number standing in for an address, printed and never
            // called.
            let pointer = unsafe { std::mem::transmute::<usize, $pointer>(0x0040_1234) };
            assert_eq!(show!(pointer), "401234", "{}", stringify!($pointer));
            assert_eq!(show!(Some(pointer)), "401234", "{}", stringify!($pointer));
            assert_eq!(show!(None::<$pointer>), "0", "{}", stringify!($pointer));
        })*
    };
}

/// One calling convention, safe and unsafe, of 0, 1 and 16 arguments.
macro_rules! abi_rows {
    ($abi:tt) => {
        address_of!(
            extern $abi fn() -> u32,
            unsafe extern $abi fn() -> u32,
            extern $abi fn(u32) -> u32,
            unsafe extern $abi fn(u32) -> u32,
            extern $abi fn(u32, u32, u32, u32, u32, u32, u32, u32,
                           u32, u32, u32, u32, u32, u32, u32, u32) -> u32,
            unsafe extern $abi fn(u32, u32, u32, u32, u32, u32, u32, u32,
                                  u32, u32, u32, u32, u32, u32, u32, u32) -> u32,
        )
    };
}

/// A type with no `Value`.
struct Unknown;

#[test]
fn value_table() {
    assert_eq!(show!(0xab_u8), "ab");
    assert_eq!(show!(0xabc_u16), "abc");
    assert_eq!(show!(0xabcd_u32), "abcd");
    assert_eq!(show!(0xabcd_u64), "abcd");
    assert_eq!(show!(0xabcd_u128), "abcd");
    assert_eq!(show!(0xabcd_usize), "abcd");
    assert_eq!(show!(-1_i8), "ff");
    assert_eq!(show!(-2_i16), "fffe");
    assert_eq!(show!(-1_i32), "ffffffff");
    assert_eq!(show!(-1_i64), "ffffffffffffffff");
    assert_eq!(show!(-1_i128), "ffffffffffffffffffffffffffffffff");
    #[cfg(target_pointer_width = "64")]
    assert_eq!(show!(-1_isize), "ffffffffffffffff");
    #[cfg(target_pointer_width = "32")]
    assert_eq!(show!(-1_isize), "ffffffff");

    assert_eq!(show!(true), "1");
    assert_eq!(show!(false), "0");

    assert_eq!(show!(1.5_f32), "1.5");
    assert_eq!(show!(0.1_f32), "0.1");
    assert_eq!(show!(-0.0_f64), "-0.0");
    assert_eq!(show!(f64::NAN), "NaN");
    assert_eq!(show!(f64::INFINITY), "inf");

    assert_eq!(show!(0x1000 as *const u8), "1000");
    assert_eq!(show!(0x1000 as *mut u8), "1000");
    assert_eq!(show!(core::ptr::null::<u8>()), "0");

    assert_eq!(show!(()), "-");
    assert_eq!(show!(None::<u32>), "0");
    assert_eq!(show!(Some(0x2a_u32)), "2a");
    assert_eq!(show!([1_u8, 0xff]), "1,ff");
    assert_eq!(show!([0_u8; 0]), "");
    assert_eq!(show!([[1_u8, 2], [3, 4]]), "1,2,3,4");

    abi_rows!("Rust");
    abi_rows!("C");

    assert_eq!(show!(Unknown), "?");
    assert_eq!(show!(&5_u32), "?");
}

/// The conventions only `x86` has, so the row a game's old calls print through.
#[cfg(target_arch = "x86")]
#[test]
fn x86_table() {
    abi_rows!("stdcall");
    abi_rows!("fastcall");
    abi_rows!("thiscall");
}

/// The compiler is the judge, so this asks it: a known type passes under the flag, an unknown
/// one is refused under it and passes without it.
#[cfg(not(windows))]
#[test]
fn strict_refuses() {
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::process::Command;

    const STRICT: &str = "--cfg oxide_trace_strict";

    /// A throwaway package whose one function holds `site`, with the workspace's lock.
    fn probe(name: &str, site: &str) -> PathBuf {
        let here = Path::new(env!("CARGO_MANIFEST_DIR"));
        let dir = Path::new(env!("CARGO_TARGET_TMPDIR"))
            .join("strict")
            .join(name);
        fs::create_dir_all(dir.join("src")).expect("the probe's folder is writable");
        let trace = here.join("../oxide-trace");
        let manifest = format!(
            "[package]\nname = \"probe_{name}\"\nversion = \"0.0.0\"\nedition = \"2024\"\n\n\
             [dependencies]\noxide_trace = {{ path = \"{}\" }}\n\n[workspace]\n",
            trace.display()
        );
        fs::write(dir.join("Cargo.toml"), manifest).expect("the manifest is writable");
        fs::write(
            dir.join("src/lib.rs"),
            format!("pub struct Unknown;\n\npub fn site() {{\n    {site};\n}}\n"),
        )
        .expect("the source is writable");
        fs::copy(here.join("../../Cargo.lock"), dir.join("Cargo.lock"))
            .expect("the lock is readable");
        dir
    }

    /// Whether the compiler took the probe under `flags`, and what it said.
    fn compiled(dir: &Path, flags: &str) -> (bool, String) {
        let output = Command::new(env!("CARGO"))
            .args(["check", "--offline"])
            .current_dir(dir)
            .env("CARGO_TARGET_DIR", dir.join("target"))
            .env("RUSTFLAGS", flags)
            .env_remove("CARGO_ENCODED_RUSTFLAGS")
            .output()
            .expect("cargo runs");
        let said = String::from_utf8_lossy(&output.stderr).into_owned();
        (output.status.success(), said)
    }

    let known = probe("known", "oxide_trace::rt!(\"401000\", 1_u32)");
    let (passed, said) = compiled(&known, STRICT);
    assert!(passed, "a known type under the flag: {said}");

    let unknown = probe("unknown", "oxide_trace::rt!(\"401000\", Unknown)");
    let (passed, said) = compiled(&unknown, STRICT);
    assert!(!passed, "an unknown type under the flag passed");
    assert!(said.contains("E0599"), "refused for another reason: {said}");

    let (passed, said) = compiled(&unknown, "");
    assert!(passed, "an unknown type without the flag: {said}");
}
