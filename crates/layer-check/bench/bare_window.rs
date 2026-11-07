// SPDX-License-Identifier: MIT
//! A bare window for the bench's picture and key under Wine, its calls declared by hand,
//! with no crate of ours in the way. Shared by file, as the bench's programs are built one
//! file at a time by `wine.sh link`.
use std::ffi::{CStr, c_void};

pub type Handle = *mut c_void;

const WS_POPUP_VISIBLE: u32 = 0x9000_0000;
const SW_SHOW: i32 = 5;
const PM_REMOVE: u32 = 1;

type Procedure = unsafe extern "system" fn(Handle, u32, usize, isize) -> isize;

/// Windows' `WNDCLASSA`.
#[repr(C)]
struct Class {
    style: u32,
    procedure: Procedure,
    class_extra: i32,
    window_extra: i32,
    instance: Handle,
    icon: Handle,
    cursor: Handle,
    background: Handle,
    menu_name: *const u8,
    name: *const u8,
}

/// Windows' `MSG`.
#[repr(C)]
struct Message {
    window: Handle,
    message: u32,
    w: usize,
    l: isize,
    time: u32,
    x: i32,
    y: i32,
}

#[link(name = "kernel32")]
unsafe extern "system" {
    fn GetModuleHandleA(name: *const u8) -> Handle;
}

#[link(name = "user32")]
unsafe extern "system" {
    fn RegisterClassA(class: *const Class) -> u16;
    fn CreateWindowExA(
        extended: u32,
        class: *const u8,
        title: *const u8,
        style: u32,
        x: i32,
        y: i32,
        width: i32,
        height: i32,
        parent: Handle,
        menu: Handle,
        instance: Handle,
        extra: Handle,
    ) -> Handle;
    fn DefWindowProcA(window: Handle, message: u32, w: usize, l: isize) -> isize;
    fn ShowWindow(window: Handle, command: i32) -> i32;
    fn SetFocus(window: Handle) -> Handle;
    fn PeekMessageA(
        message: *mut Message,
        window: Handle,
        lowest: u32,
        highest: u32,
        remove: u32,
    ) -> i32;
    fn DispatchMessageA(message: *const Message) -> isize;
}

/// Opens a borderless window at a place and a size, shown and asking for the focus; prints
/// each call in the log's `imp.call` form, as this file links no logger.
pub fn open(title: &CStr, place: (i32, i32), size: (i32, i32)) -> Result<Handle, String> {
    let name = title.as_ptr().cast::<u8>();
    // SAFETY: a null name asks for this program; the class and the title outlive the calls,
    // and Windows copies the class when it registers it.
    unsafe {
        let instance = GetModuleHandleA(std::ptr::null());
        let class = Class {
            style: 0,
            procedure: DefWindowProcA,
            class_extra: 0,
            window_extra: 0,
            instance,
            icon: std::ptr::null_mut(),
            cursor: std::ptr::null_mut(),
            background: std::ptr::null_mut(),
            menu_name: std::ptr::null(),
            name,
        };
        let atom = RegisterClassA(&raw const class);
        println!("imp.call c=USER32!RegisterClassA v={atom:x} by=layer-check");
        let window = CreateWindowExA(
            0,
            name,
            name,
            WS_POPUP_VISIBLE,
            place.0,
            place.1,
            size.0,
            size.1,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            instance,
            std::ptr::null_mut(),
        );
        println!("imp.call c=USER32!CreateWindowExA v={window:?} by=layer-check");
        if window.is_null() {
            return Err("CreateWindowExA gave no window".to_owned());
        }
        let shown = ShowWindow(window, SW_SHOW);
        let focus = SetFocus(window);
        println!("imp.call c=USER32!ShowWindow v={shown} by=layer-check");
        println!("imp.call c=USER32!SetFocus v={focus:?} by=layer-check");
        Ok(window)
    }
}

/// Hands every waiting message to the window's procedure, so Wine goes on delivering input.
pub fn pump() {
    let mut message = Message {
        window: std::ptr::null_mut(),
        message: 0,
        w: 0,
        l: 0,
        time: 0,
        x: 0,
        y: 0,
    };
    // SAFETY: the message record outlives the calls, and a null window asks for them all.
    unsafe {
        while PeekMessageA(&raw mut message, std::ptr::null_mut(), 0, 0, PM_REMOVE) != 0 {
            DispatchMessageA(&raw const message);
        }
    }
}
