// SPDX-License-Identifier: MIT
//! A real window under Wine, for the programs that need `draw::user32`'s back.

/// The call site a front's trace line names: these programs have one.
pub const FROM: u32 = 1;
const WS_POPUP: u32 = 1 << 31;
const WS_VISIBLE: u32 = 1 << 28;
const SW_SHOW: u32 = 5;
const CLASS_WORDS: usize = 10;

#[link(name = "kernel32")]
unsafe extern "system" {
    fn GetModuleHandleA(name: u32) -> u32;
}

/// The low 32 bits of a value's address, the whole pointer on this target.
pub fn address<T: ?Sized>(value: &T) -> u32 {
    u32::try_from(std::ptr::from_ref(value).cast::<()>().addr()).unwrap_or(0)
}

unsafe extern "system" fn procedure(window: u32, message: u32, wide: u32, long: u32) -> u32 {
    // SAFETY: the window's own message, passed on whole.
    unsafe { window::user32::def_window_proc(FROM, window, message, wide, long) }
}

/// Registers the class and makes the window, shown at `left`, `top`: its handle.
pub fn make_window(left: i32, top: i32, width: u32, height: u32) -> u32 {
    let class_name = b"surfacesclass\0";
    let title = b"surfaces\0";
    // SAFETY: null names this program.
    let instance = unsafe { GetModuleHandleA(0) };
    let handler = procedure as unsafe extern "system" fn(u32, u32, u32, u32) -> u32;
    let mut class = [0_u32; CLASS_WORDS];
    class[1] = u32::try_from((handler as *const ()).addr()).unwrap_or(0);
    class[4] = instance;
    class[9] = address(class_name);
    // SAFETY: the class record and the texts outlive the calls.
    unsafe {
        let _ = window::user32::register_class(FROM, address(&class));
        let window = window::user32::create_window(
            FROM,
            0,
            address(class_name),
            address(title),
            WS_POPUP | WS_VISIBLE,
            u32::try_from(left).unwrap_or(0),
            u32::try_from(top).unwrap_or(0),
            width,
            height,
            0,
            0,
            instance,
            0,
        );
        let _ = window::user32::show_window(FROM, window, SW_SHOW);
        let _ = window::user32::set_focus(FROM, window);
        window
    }
}
