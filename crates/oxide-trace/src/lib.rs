// SPDX-License-Identifier: MIT
//! The DEBUG trace's macros: each wraps one site of a port, evaluates the
//! wrapped expression once, and logs at DEBUG the values that moved.
//!
//! A port keeps every read and write it had, in the same count and order: a
//! macro logs only what the port already computed (an address it already
//! held, a value it already read or is about to write), never a fresh read.
//! Each line is one event of the workspace's logbook with the port's
//! address as the field `f`; a sink renders the line, and none is
//! installed here.
//!
//! A value prints by [`Value`]: integers in hex (two's complement when
//! signed), floats in decimal, pointers and function pointers as their
//! address. A value of any
//! other type prints `?` (building with `--cfg oxide_trace_strict` turns
//! each such site into a compile error, which counts them).
#![no_std]

use core::fmt;

#[doc(hidden)]
pub use log;

/// A value a log line can show.
pub trait Value {
    fn fmt_value(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result;
}

macro_rules! hex_values {
    ($($t:ty),*) => {
        $(impl Value for $t {
            fn fmt_value(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, "{self:x}")
            }
        })*
    };
}

hex_values!(
    u8, u16, u32, u64, u128, usize, i8, i16, i32, i64, i128, isize
);

impl Value for bool {
    fn fmt_value(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", u8::from(*self))
    }
}

impl Value for f32 {
    fn fmt_value(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

impl Value for f64 {
    fn fmt_value(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

impl<T: ?Sized> Value for *const T {
    fn fmt_value(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:x}", self.addr())
    }
}

impl<T: ?Sized> Value for *mut T {
    fn fmt_value(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:x}", self.addr())
    }
}

impl Value for () {
    fn fmt_value(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("-")
    }
}

/// A function pointer prints its address.
macro_rules! fn_pointer_values {
    ($($abi:tt),*) => {
        $(fn_pointer_values!(@arity $abi; []
            A1 A2 A3 A4 A5 A6 A7 A8 A9 A10 A11 A12 A13 A14 A15 A16);)*
    };
    (@arity $abi:tt; [$($done:ident)*] $next:ident $($rest:ident)*) => {
        fn_pointer_values!(@impl $abi; $($done)*);
        fn_pointer_values!(@arity $abi; [$($done)* $next] $($rest)*);
    };
    (@arity $abi:tt; [$($done:ident)*]) => {
        fn_pointer_values!(@impl $abi; $($done)*);
    };
    (@impl $abi:tt; $($argument:ident)*) => {
        impl<R, $($argument),*> Value for unsafe extern $abi fn($($argument),*) -> R {
            fn fmt_value(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, "{:x}", *self as usize)
            }
        }

        impl<R, $($argument),*> Value for extern $abi fn($($argument),*) -> R {
            fn fmt_value(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, "{:x}", *self as usize)
            }
        }
    };
}

fn_pointer_values!("Rust", "C");

#[cfg(target_arch = "x86")]
fn_pointer_values!("stdcall", "fastcall", "thiscall");

/// `None` prints `0`, the null its niche holds in the original.
impl<T: Value> Value for Option<T> {
    fn fmt_value(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Some(inner) => inner.fmt_value(f),
            None => f.write_str("0"),
        }
    }
}

/// The elements in order, comma-separated, no spaces.
impl<T: Value, const N: usize> Value for [T; N] {
    fn fmt_value(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (at, element) in self.iter().enumerate() {
            if at > 0 {
                f.write_str(",")?;
            }
            element.fmt_value(f)?;
        }
        Ok(())
    }
}

/// A value as a log field shows it: by [`Value`], or `?`.
#[doc(hidden)]
pub enum Shown<'a> {
    Value(&'a dyn Value),
    Opaque,
}

impl fmt::Display for Shown<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Shown::Value(value) => value.fmt_value(f),
            Shown::Opaque => f.write_str("?"),
        }
    }
}

/// The arguments at a port's entry, comma-separated in order.
#[doc(hidden)]
pub struct Args<'a, const N: usize>(pub [Shown<'a>; N]);

impl<const N: usize> fmt::Display for Args<'_, N> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (at, argument) in self.0.iter().enumerate() {
            if at > 0 {
                f.write_str(",")?;
            }
            argument.fmt(f)?;
        }
        Ok(())
    }
}

/// The probe [`__shown!`] picks a value's form through: a type with a
/// [`Value`] impl resolves to [`ViaValue`] at the first method-lookup step,
/// any other type to [`ViaOpaque`] at the second (autoref specialization).
#[doc(hidden)]
pub struct Probe<'a, T>(pub &'a T);

#[doc(hidden)]
pub trait ViaValue<'a> {
    fn shown(&self) -> Shown<'a>;
}

impl<'a, T: Value> ViaValue<'a> for Probe<'a, T> {
    fn shown(&self) -> Shown<'a> {
        Shown::Value(self.0)
    }
}

#[doc(hidden)]
pub trait ViaOpaque<'a> {
    fn shown(&self) -> Shown<'a>;
}

#[cfg(not(oxide_trace_strict))]
impl<'a, T> ViaOpaque<'a> for &Probe<'a, T> {
    fn shown(&self) -> Shown<'a> {
        Shown::Opaque
    }
}

/// Hands `value` to `log` by reference, then returns it: the one
/// evaluation every macro below routes its wrapped expression through, so
/// the expression keeps the type the site expects of it.
#[doc(hidden)]
#[inline]
pub fn tap<T>(value: T, log: impl FnOnce(&T)) -> T {
    log(&value);
    value
}

#[doc(hidden)]
#[macro_export]
macro_rules! __shown {
    ($value:expr) => {{
        #[allow(unused_imports)]
        use $crate::{ViaOpaque as _, ViaValue as _};
        (&$crate::Probe(&$value)).shown()
    }};
}

/// `fn.in`: a port's entry, each argument in order.
#[macro_export]
macro_rules! enter {
    ($f:literal) => {
        $crate::log::debug!(f = $f; "fn.in")
    };
    ($f:literal, $($argument:expr),+ $(,)?) => {
        $crate::log::debug!(
            f = $f, a:% = $crate::Args([$($crate::__shown!($argument)),+]); "fn.in"
        )
    };
}

/// `fn.ret`: the value a port returns (`return e`, or its tail).
#[macro_export]
macro_rules! rt {
    ($f:literal, $value:expr $(,)?) => {
        $crate::tap($value, |v| {
            $crate::log::debug!(f = $f, v:% = $crate::__shown!(*v); "fn.ret")
        })
    };
}

/// `mem.rd`: a pointer method's read (`p.read_unaligned()`).
#[macro_export]
macro_rules! rd {
    ($f:literal, $pointer:expr, $method:ident $(,)?) => {{
        #[allow(unused_parens, reason = "a receiver keeps the parens its method call needed")]
        let p = $pointer;
        $crate::tap(p.$method(), |v| {
            $crate::log::debug!(
                f = $f, a:% = $crate::__shown!(p), v:% = $crate::__shown!(*v); "mem.rd"
            )
        })
    }};
}

/// `mem.rd`: a read through `core::ptr`'s function form.
#[macro_export]
macro_rules! rdf {
    ($f:literal, $function:path, $pointer:expr $(,)?) => {{
        #[allow(unused_parens, reason = "a receiver keeps the parens its method call needed")]
        let p = $pointer;
        $crate::tap($function(p), |v| {
            $crate::log::debug!(
                f = $f, a:% = $crate::__shown!(p), v:% = $crate::__shown!(*v); "mem.rd"
            )
        })
    }};
}

/// `mem.rd`: a raw deref read (`*p`). The pointer is taken as `&raw`
/// of its place, as `dw` and `du` take theirs: a `&mut` is reborrowed,
/// never moved, and nothing is read to take it.
#[macro_export]
macro_rules! dr {
    ($f:literal, $pointer:expr $(,)?) => {{
        #[allow(unused_parens, reason = "a receiver keeps the parens its method call needed")]
        let p = &raw const *$pointer;
        $crate::tap(*p, |v| {
            $crate::log::debug!(
                f = $f, a:% = $crate::__shown!(p), v:% = $crate::__shown!(*v); "mem.rd"
            )
        })
    }};
}

/// `mem.wr`: a pointer method's write (`p.write_unaligned(v)`); the
/// receiver is evaluated first, then the value, as in the call.
#[macro_export]
macro_rules! wr {
    ($f:literal, $pointer:expr, $method:ident, $value:expr $(,)?) => {{
        #[allow(unused_parens, reason = "a receiver keeps the parens its method call needed")]
        let p = $pointer;
        p.$method($crate::tap($value, |v| {
            $crate::log::debug!(
                f = $f, a:% = $crate::__shown!(p), v:% = $crate::__shown!(*v); "mem.wr"
            )
        }))
    }};
}

/// `mem.wr`: a raw deref write (`*p = v`); the value is evaluated first,
/// then the pointer, as in the assignment.
#[macro_export]
macro_rules! dw {
    ($f:literal, $pointer:expr, $value:expr $(,)?) => {{
        let v = $value;
        #[allow(unused_parens, reason = "a receiver keeps the parens its method call needed")]
        let p = &raw mut *$pointer;
        *p = $crate::tap(v, |v| {
            $crate::log::debug!(
                f = $f, a:% = $crate::__shown!(p), v:% = $crate::__shown!(*v); "mem.wr"
            )
        });
    }};
}

/// `mem.upd`: a raw deref compound assignment (`*p += v`); the operand is
/// logged, never the result, which would be a second read.
#[macro_export]
macro_rules! du {
    ($f:literal, $pointer:expr, $op:tt, $value:expr $(,)?) => {{
        let v = $value;
        #[allow(unused_parens, reason = "a receiver keeps the parens its method call needed")]
        let p = &raw mut *$pointer;
        *p $op $crate::tap(v, |v| {
            $crate::log::debug!(
                f = $f, a:% = $crate::__shown!(p), op = stringify!($op),
                v:% = $crate::__shown!(*v); "mem.upd"
            )
        });
    }};
}

/// `glob.rd`: a global read by value (`DAT_x`, `DAT_x[i]`).
#[macro_export]
macro_rules! gr {
    ($f:literal, $global:literal, $value:expr $(,)?) => {
        $crate::tap($value, |v| {
            $crate::log::debug!(f = $f, g = $global, v:% = $crate::__shown!(*v); "glob.rd")
        })
    };
}

/// `glob.wr`: a global assigned (`DAT_x = v`); the value is evaluated
/// first, then the place, as in the assignment.
#[macro_export]
macro_rules! gw {
    ($f:literal, $global:literal, $place:expr, $value:expr $(,)?) => {
        $place = $crate::tap($value, |v| {
            $crate::log::debug!(f = $f, g = $global, v:% = $crate::__shown!(*v); "glob.wr")
        })
    };
}

/// `glob.upd`: a global's compound assignment (`DAT_x += v`); the operand
/// is logged, never the result, which would be a second read.
#[macro_export]
macro_rules! gu {
    ($f:literal, $global:literal, $place:expr, $op:tt, $value:expr $(,)?) => {
        $place $op $crate::tap($value, |v| {
            $crate::log::debug!(
                f = $f, g = $global, op = stringify!($op), v:% = $crate::__shown!(*v); "glob.upd"
            )
        })
    };
}

/// `call.ret`: a callee's result; `c` is its address, or its name when it
/// has none (`ind` for a call through a pointer).
#[macro_export]
macro_rules! cr {
    ($f:literal, $callee:literal, $call:expr $(,)?) => {
        $crate::tap($call, |v| {
            $crate::log::debug!(f = $f, c = $callee, v:% = $crate::__shown!(*v); "call.ret")
        })
    };
}

/// `mem.copy`: `core::ptr::copy` or `copy_nonoverlapping`, its source,
/// destination and count; the bytes copied are not read again to log.
#[macro_export]
macro_rules! cp {
    ($f:literal, $function:path, $source:expr, $destination:expr, $count:expr $(,)?) => {{
        let s = $source;
        let d = $destination;
        let n = $count;
        $crate::log::debug!(
            f = $f, s:% = $crate::__shown!(s), d:% = $crate::__shown!(d),
            n:% = $crate::__shown!(n); "mem.copy"
        );
        $function(s, d, n)
    }};
}

/// `mem.fill`: `core::ptr::write_bytes`, its destination, byte and count.
#[macro_export]
macro_rules! fill {
    ($f:literal, $function:path, $destination:expr, $byte:expr, $count:expr $(,)?) => {{
        let d = $destination;
        let b = $byte;
        let n = $count;
        $crate::log::debug!(
            f = $f, d:% = $crate::__shown!(d), b:% = $crate::__shown!(b),
            n:% = $crate::__shown!(n); "mem.fill"
        );
        $function(d, b, n)
    }};
}
