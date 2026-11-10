// SPDX-License-Identifier: MIT
//! A host's back through winit 0.30.13: Linux's X11 and Wayland, and
//! Windows; macOS builds untried. Each pump turns winit's
//! loop once and never waits, as `PeekMessageA` does: the host's close and
//! focus, its keys, buttons and pointer, and the pointer's relative motion
//! are handed on as [`Event`]s. A window is made in the loop's turn, so
//! one placed shows at the next pump. The screen's size is the one the
//! back is made with: the program is answered the size it was built for,
//! and its windows are placed on the host's desktop as the host allows.
//! The host draws the frame: the program's rectangle is the window's
//! inside, the frame's corner at its top left. Linux turns the loop on the
//! main thread alone, and one loop a program.
//!
//! A window may be asked full screen ([`Native::full_screen`]): no frame,
//! the whole of the monitor the host shows it on, at that monitor's own
//! size. The program's rectangle is kept while it is, never asked of the
//! host, and asked again as the window leaves it. A window is framed
//! until asked.
//!
//! A key is the virtual key number of its place on the keyboard,
//! Windows' own numbering, whatever the host's layout; Shift, Ctrl and
//! Alt either side are `0x10`, `0x11` and `0x12`. The pointer is placed
//! in the program's rectangle by the fit its picture is shown at
//! ([`PointerFit`]): the rectangle shown at the largest whole factor on
//! the window's inside, centred, as the drawing crate shows a buffer the
//! rectangle's size, a host pixel on the black strips at the picture's
//! nearest edge pixel; a window the rectangle's own size, or too small
//! for a whole factor, hands on its inside's pixel plus the rectangle's
//! corner. One `pointer.fit` line at INFO as a window's fit, its inside
//! or its rectangle's size changes; each move one `pointer.at` line at
//! DEBUG, the host's point and the program's. The
//! lock is the program's to keep (`window-input`): its script asks
//! [`Native::grab`] and [`Native::cursor_shown`] as it changes.
//!
//! The walls, named: a key outside [`virtual_key`]'s rows is dropped, as
//! is a button other than the 3; a key winit raises for itself as the
//! focus moves (its synthetic presses and releases) is dropped, the
//! program's queue never taking it; relative motion comes while a window
//! of the program's has the focus alone, winit's default; X11 takes
//! `Confined` alone, `Locked` not implemented there (winit's
//! `CursorGrabMode` notes), so a confined pointer still moves the host's
//! cursor inside the window while the program reads its motion; and X11's
//! grab hands each relative move on twice, the copy dropped here
//! (`Twins`). The pointer's picture is the program's rectangle, the
//! drawing crate's the buffer it is shown: the two agree while the
//! buffer is the rectangle's size; a drawing back asked to show from
//! the corner leaves the pointer at the whole fit; relative motion is
//! the host's counts, never divided by the fit.
use std::cell::RefCell;
use std::rc::Rc;
use std::time::{Duration, Instant};

use raw_window_handle::{
    DisplayHandle, HandleError, HasDisplayHandle, HasWindowHandle, WindowHandle,
};
use winit::application::ApplicationHandler;
use winit::dpi::{PhysicalPosition, PhysicalSize};
use winit::error::{EventLoopError, OsError};
use winit::event::{DeviceEvent, DeviceId, ElementState, MouseButton, WindowEvent};
use winit::event_loop::{ActiveEventLoop, EventLoop};
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::platform::pump_events::EventLoopExtPumpEvents;
use winit::window::{CursorGrabMode, Fullscreen, Window, WindowId};

use crate::{Event, PointerFit, Rect, Screen};

/// Where a program's window stands with its host's.
enum Made {
    /// Placed, not yet made: the loop's next turn makes it.
    Waiting,
    /// Shared with each [`Lent`]: a window let go lives until the last is
    /// dropped.
    Window(Rc<Window>),
    /// The host would not make it: its reason kept, never asked again.
    Refused(OsError),
}

/// One program window as last placed, whether it is asked full screen,
/// and the fit its pointer is placed by: made on an inside of no size
/// until the host window is made.
struct Placed {
    window: u32,
    rect: Rect,
    shown: bool,
    full: bool,
    made: Made,
    pointer: PointerFit,
}

impl Placed {
    /// The pointer's fit made again from the rectangle and `inside`, the
    /// window's inside in the host's pixels; one `pointer.fit` line when
    /// it differs from the last.
    fn refit(&mut self, inside: (u32, u32)) {
        let now = PointerFit::new(self.rect, inside);
        if core::mem::replace(&mut self.pointer, now) != now {
            let (k, x, y) = now
                .fit()
                .map_or((0, 0, 0), |fit| (fit.factor, fit.x, fit.y));
            let ((width, height), (pw, ph)) = (now.inside(), now.picture());
            log::info!(
                window:% = oxide_trace::__shown!(self.window),
                k:% = oxide_trace::__shown!(k),
                x:% = oxide_trace::__shown!(x),
                y:% = oxide_trace::__shown!(y),
                w:% = oxide_trace::__shown!(width),
                h:% = oxide_trace::__shown!(height),
                pw:% = oxide_trace::__shown!(pw),
                ph:% = oxide_trace::__shown!(ph);
                "pointer.fit"
            );
        }
    }

    /// The program's point for the host's pointer at `host` in the
    /// window's inside, by the pointer's fit; one `pointer.at` line.
    fn point(&self, host: (i32, i32)) -> (i32, i32) {
        let (x, y) = self.pointer.point(self.rect, host);
        let (hx, hy) = (crate::bits(host.0), crate::bits(host.1));
        let (px, py) = (crate::bits(x), crate::bits(y));
        log::debug!(
            window:% = oxide_trace::__shown!(self.window),
            hx:% = oxide_trace::__shown!(hx),
            hy:% = oxide_trace::__shown!(hy),
            x:% = oxide_trace::__shown!(px),
            y:% = oxide_trace::__shown!(py);
            "pointer.at"
        );
        (x, y)
    }
}

/// A program window lent to another crate's drawing: its host handles, its
/// inside's size and its scale, and no winit type named. A window let go lives
/// until its last `Lent` is dropped, so a surface over it is never left over a
/// closed window.
#[derive(Clone)]
pub struct Lent(Rc<Window>);

impl Lent {
    /// The window's inside, width and height in the host's pixels.
    #[must_use]
    pub fn inner_size(&self) -> (u32, u32) {
        let size = self.0.inner_size();
        (size.width, size.height)
    }

    /// The host's pixels to one of the program's, as the host last said.
    #[must_use]
    pub fn scale_factor(&self) -> f64 {
        self.0.scale_factor()
    }
}

impl HasWindowHandle for Lent {
    fn window_handle(&self) -> Result<WindowHandle<'_>, HandleError> {
        self.0.window_handle()
    }
}

impl HasDisplayHandle for Lent {
    fn display_handle(&self) -> Result<DisplayHandle<'_>, HandleError> {
        self.0.display_handle()
    }
}

/// How the host holds the pointer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Hold {
    /// Free to leave the window.
    Free,
    /// Held where it stood, moving by relative motion alone: Wayland's
    /// and Windows', not X11's.
    Locked,
    /// Kept inside the window: X11's, Wayland's and Windows'.
    Confined,
}

/// Relative motion an X11 grab hands on twice, each pair kept once.
///
/// The rule, xorg-server's `dix/events.c` (tag 21.1.12, this box's
/// server, and `master` alike): `DeliverRawEvent` gives a grabbed
/// pointer's raw event to `DeliverGrabbedEvent`, whose owner-events walk
/// ends at the root's raw selection, then to every client selecting raw
/// events on the root; `FilterRawEvents` skips the grabbing client there
/// only when the grab's window is the root. winit grabs on its own
/// window and selects raw motion on the root, so each move comes twice,
/// side by side, one `DeliverRawEvent` writing both. Run on Xvfb:
/// 3 moves read 6 `MouseMotion`s grabbed, 3 free. Keys come
/// once: the keyboard is not grabbed. A pair never overlaps another, so
/// a copy is known by its bits alone, never by `==` on floats.
#[derive(Default)]
struct Twins {
    /// The window whose X11 grab is held: `None`, every motion kept.
    doubled: Option<u32>,
    /// The bits of a first motion whose copy has not yet come.
    first: Option<(u64, u64)>,
}

impl Twins {
    /// Whether `delta` is a move to hand on: armed, a first is kept and
    /// its copy dropped; unarmed, every one kept.
    fn keep(&mut self, delta: (f64, f64)) -> bool {
        if self.doubled.is_none() {
            return true;
        }
        let bits = (delta.0.to_bits(), delta.1.to_bits());
        if self.first.take() == Some(bits) {
            return false;
        }
        self.first = Some(bits);
        true
    }

    /// `window`'s X11 grab is held; one held before is the server's to
    /// end, a client holding a single grab.
    const fn arm(&mut self, window: u32) {
        self.doubled = Some(window);
    }

    /// `window`'s grab is let go, ended by the server or the window gone:
    /// every motion kept again; another window's grab stays armed.
    fn release(&mut self, window: u32) {
        if self.doubled == Some(window) {
            *self = Self::default();
        }
    }
}

/// The windows, the events raised since the last pump, the pointer's
/// motion not yet a whole pixel, and the grab's doubled motion: winit's
/// handler.
#[derive(Default)]
struct Desktop {
    placed: Vec<Placed>,
    events: Vec<Event>,
    motion: (f64, f64),
    twins: Twins,
}

impl Desktop {
    /// Makes each window placed since the loop's last turn.
    fn make(&mut self, event_loop: &ActiveEventLoop) {
        for placed in &mut self.placed {
            if matches!(placed.made, Made::Waiting) {
                let attributes = Window::default_attributes()
                    .with_inner_size(inside(placed.rect))
                    .with_position(corner(placed.rect))
                    .with_visible(placed.shown)
                    .with_fullscreen(screen(placed.full));
                placed.made = match event_loop.create_window(attributes) {
                    Ok(host) => {
                        let size = host.inner_size();
                        placed.refit((size.width, size.height));
                        Made::Window(Rc::new(host))
                    }
                    Err(refusal) => Made::Refused(refusal),
                };
            }
        }
    }
}

impl ApplicationHandler for Desktop {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        self.make(event_loop);
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        self.make(event_loop);
    }

    /// The host asked a window closed, moved the user's attention, or
    /// took a key, a button or the pointer: queued; the host's window
    /// stays until the program lets it go. A new inside makes the
    /// pointer's fit again, queuing nothing.
    fn window_event(&mut self, _: &ActiveEventLoop, id: WindowId, event: WindowEvent) {
        let Some(placed) = self
            .placed
            .iter_mut()
            .find(|placed| matches!(&placed.made, Made::Window(host) if host.id() == id))
        else {
            return;
        };
        let window = placed.window;
        let event = match event {
            WindowEvent::CloseRequested => Event::Close { window },
            WindowEvent::Focused(active) => Event::Active { window, active },
            WindowEvent::KeyboardInput {
                event,
                is_synthetic: false,
                ..
            } => {
                let PhysicalKey::Code(code) = event.physical_key else {
                    return;
                };
                let Some(key) = virtual_key(code) else {
                    return;
                };
                Event::Key {
                    window,
                    key,
                    down: event.state == ElementState::Pressed,
                }
            }
            WindowEvent::MouseInput { state, button, .. } => {
                let key = match button {
                    MouseButton::Left => 1,
                    MouseButton::Right => 2,
                    MouseButton::Middle => 4,
                    _ => return,
                };
                Event::Button {
                    window,
                    key,
                    down: state == ElementState::Pressed,
                }
            }
            WindowEvent::Resized(size) => {
                placed.refit((size.width, size.height));
                return;
            }
            WindowEvent::CursorMoved { position, .. } => {
                let host = (whole(position.x.round()), whole(position.y.round()));
                let (x, y) = placed.point(host);
                Event::Pointer { window, x, y }
            }
            _ => return,
        };
        self.events.push(event);
    }

    /// The pointer's relative motion, its whole pixels handed on and the
    /// rest kept for the next; an X11 grab's copy dropped first.
    fn device_event(&mut self, _: &ActiveEventLoop, _: DeviceId, event: DeviceEvent) {
        let DeviceEvent::MouseMotion { delta } = event else {
            return;
        };
        if !self.twins.keep(delta) {
            return;
        }
        let (x, y) = (self.motion.0 + delta.0, self.motion.1 + delta.1);
        let (dx, dy) = (x.trunc(), y.trunc());
        self.motion = (x - dx, y - dy);
        if dx != 0.0 || dy != 0.0 {
            self.events.push(Event::Motion {
                dx: whole(dx),
                dy: whole(dy),
            });
        }
    }
}

/// A whole number of pixels as an `i32`, saturating past either end.
#[expect(
    clippy::cast_possible_truncation,
    reason = "every caller rounds or truncates first, and `as` saturates"
)]
const fn whole(pixels: f64) -> i32 {
    pixels as i32
}

/// The virtual key number of `code`'s place on the keyboard, Windows'
/// `VK_` numbering; `None` for a key with no row: the 26 letters, the 10
/// digits, F1 to F12, the arrows, Return, Escape, Backspace, Tab, space,
/// Shift, Ctrl and Alt, the 6 keys above the arrows, Pause and the 3
/// locks, the number pad, and the 11 `VK_OEM` keys of the US layout.
#[must_use]
pub const fn virtual_key(code: KeyCode) -> Option<u8> {
    use KeyCode as K;
    Some(match code {
        K::KeyA => b'A',
        K::KeyB => b'B',
        K::KeyC => b'C',
        K::KeyD => b'D',
        K::KeyE => b'E',
        K::KeyF => b'F',
        K::KeyG => b'G',
        K::KeyH => b'H',
        K::KeyI => b'I',
        K::KeyJ => b'J',
        K::KeyK => b'K',
        K::KeyL => b'L',
        K::KeyM => b'M',
        K::KeyN => b'N',
        K::KeyO => b'O',
        K::KeyP => b'P',
        K::KeyQ => b'Q',
        K::KeyR => b'R',
        K::KeyS => b'S',
        K::KeyT => b'T',
        K::KeyU => b'U',
        K::KeyV => b'V',
        K::KeyW => b'W',
        K::KeyX => b'X',
        K::KeyY => b'Y',
        K::KeyZ => b'Z',
        K::Digit0 => b'0',
        K::Digit1 => b'1',
        K::Digit2 => b'2',
        K::Digit3 => b'3',
        K::Digit4 => b'4',
        K::Digit5 => b'5',
        K::Digit6 => b'6',
        K::Digit7 => b'7',
        K::Digit8 => b'8',
        K::Digit9 => b'9',
        K::F1 => 0x70,
        K::F2 => 0x71,
        K::F3 => 0x72,
        K::F4 => 0x73,
        K::F5 => 0x74,
        K::F6 => 0x75,
        K::F7 => 0x76,
        K::F8 => 0x77,
        K::F9 => 0x78,
        K::F10 => 0x79,
        K::F11 => 0x7a,
        K::F12 => 0x7b,
        K::ArrowLeft => 0x25,
        K::ArrowUp => 0x26,
        K::ArrowRight => 0x27,
        K::ArrowDown => 0x28,
        K::Enter | K::NumpadEnter => 0x0d,
        K::Escape => 0x1b,
        K::Backspace => 0x08,
        K::Tab => 0x09,
        K::Space => 0x20,
        K::ShiftLeft | K::ShiftRight => 0x10,
        K::ControlLeft | K::ControlRight => 0x11,
        K::AltLeft | K::AltRight => 0x12,
        K::PageUp => 0x21,
        K::PageDown => 0x22,
        K::End => 0x23,
        K::Home => 0x24,
        K::Insert => 0x2d,
        K::Delete => 0x2e,
        K::Pause => 0x13,
        K::CapsLock => 0x14,
        K::NumLock => 0x90,
        K::ScrollLock => 0x91,
        K::Numpad0 => 0x60,
        K::Numpad1 => 0x61,
        K::Numpad2 => 0x62,
        K::Numpad3 => 0x63,
        K::Numpad4 => 0x64,
        K::Numpad5 => 0x65,
        K::Numpad6 => 0x66,
        K::Numpad7 => 0x67,
        K::Numpad8 => 0x68,
        K::Numpad9 => 0x69,
        K::NumpadMultiply => 0x6a,
        K::NumpadAdd => 0x6b,
        K::NumpadSubtract => 0x6d,
        K::NumpadDecimal => 0x6e,
        K::NumpadDivide => 0x6f,
        K::Semicolon => 0xba,
        K::Equal => 0xbb,
        K::Comma => 0xbc,
        K::Minus => 0xbd,
        K::Period => 0xbe,
        K::Slash => 0xbf,
        K::Backquote => 0xc0,
        K::BracketLeft => 0xdb,
        K::Backslash => 0xdc,
        K::BracketRight => 0xdd,
        K::Quote => 0xde,
        _ => return None,
    })
}

/// The host's full screen when `full`: no frame, the monitor the host
/// shows the window on; `None`, a framed window.
fn screen(full: bool) -> Option<Fullscreen> {
    full.then_some(Fullscreen::Borderless(None))
}

/// The rectangle's top left, where the host's frame goes.
fn corner(rect: Rect) -> PhysicalPosition<i32> {
    PhysicalPosition::new(rect.left, rect.top)
}

/// The rectangle's size, the window's inside: 1 pixel at least, a host
/// refusing an empty window.
fn inside(rect: Rect) -> PhysicalSize<u32> {
    let side = |span: i32| u32::try_from(span).unwrap_or(0).max(1);
    PhysicalSize::new(
        side(rect.right.saturating_sub(rect.left)),
        side(rect.bottom.saturating_sub(rect.top)),
    )
}

/// Whether the loop's host is an X11 server, whose grab doubles relative
/// motion ([`Twins`]); the cfg is winit's `x11_platform`.
#[cfg(all(
    unix,
    not(any(target_os = "ios", target_os = "macos")),
    not(target_os = "android"),
    not(target_os = "emscripten"),
    not(target_os = "redox")
))]
fn on_x11(event_loop: &EventLoop<()>) -> bool {
    use winit::platform::x11::EventLoopExtX11;
    event_loop.is_x11()
}

/// No X11 server off winit's X11 targets.
#[cfg(not(all(
    unix,
    not(any(target_os = "ios", target_os = "macos")),
    not(target_os = "android"),
    not(target_os = "emscripten"),
    not(target_os = "redox")
)))]
const fn on_x11(_: &EventLoop<()>) -> bool {
    false
}

/// The back on a host's desktop.
pub struct Native {
    size: (i32, i32),
    began: Instant,
    x11: bool,
    event_loop: RefCell<EventLoop<()>>,
    desktop: RefCell<Desktop>,
}

impl Native {
    /// A back answering a `width` by `height` screen, its loop made now.
    /// Linux refuses a loop off the main thread, or a second one.
    pub fn new(width: i32, height: i32) -> Result<Self, EventLoopError> {
        let event_loop = EventLoop::new()?;
        Ok(Self {
            size: (width, height),
            began: Instant::now(),
            x11: on_x11(&event_loop),
            event_loop: RefCell::new(event_loop),
            desktop: RefCell::new(Desktop::default()),
        })
    }

    /// Whether the host has made `window`'s window: `Ok(false)` for one
    /// waiting for the next pump, gone or never placed; `Err` the host's
    /// reason it would not.
    pub fn made(&self, window: u32) -> Result<bool, String> {
        let desktop = self.desktop.borrow();
        match desktop.placed.iter().find(|placed| placed.window == window) {
            Some(Placed {
                made: Made::Window(_),
                ..
            }) => Ok(true),
            Some(Placed {
                made: Made::Refused(refusal),
                ..
            }) => Err(refusal.to_string()),
            _ => Ok(false),
        }
    }

    /// Runs `ask` on `window`'s host window; `Err` for one not made.
    fn host<T>(&self, window: u32, ask: impl FnOnce(&Window) -> T) -> Result<T, String> {
        let desktop = self.desktop.borrow();
        match desktop.placed.iter().find(|placed| placed.window == window) {
            Some(Placed {
                made: Made::Window(host),
                ..
            }) => Ok(ask(host)),
            _ => Err(format!("window {window} not made")),
        }
    }

    /// Lends `window`'s host window; `Err` for one not made, so a caller
    /// lends after [`Self::made`] says so.
    pub fn lend(&self, window: u32) -> Result<Lent, String> {
        let desktop = self.desktop.borrow();
        match desktop.placed.iter().find(|placed| placed.window == window) {
            Some(Placed {
                made: Made::Window(host),
                ..
            }) => Ok(Lent(Rc::clone(host))),
            _ => Err(format!("window {window} not made")),
        }
    }

    /// Asks the host to hold the pointer over `window` as `hold`; `Err`
    /// the host's refusal, or a window not made. A hold taken on X11
    /// arms [`Twins`]; `Free` releases it.
    pub fn hold(&self, window: u32, hold: Hold) -> Result<(), String> {
        let mode = match hold {
            Hold::Free => CursorGrabMode::None,
            Hold::Locked => CursorGrabMode::Locked,
            Hold::Confined => CursorGrabMode::Confined,
        };
        self.host(window, |host| host.set_cursor_grab(mode))?
            .map_err(|refusal| refusal.to_string())?;
        let twins = &mut self.desktop.borrow_mut().twins;
        if hold == Hold::Free {
            twins.release(window);
        } else if self.x11 {
            twins.arm(window);
        }
        Ok(())
    }

    /// The program's lock as the host can hold it: when `locked`,
    /// `Locked`, else `Confined` on its refusal; unlocked, `Free`. The
    /// hold taken, or `Err` every refusal.
    pub fn grab(&self, window: u32, locked: bool) -> Result<Hold, String> {
        if !locked {
            return self.hold(window, Hold::Free).map(|()| Hold::Free);
        }
        match self.hold(window, Hold::Locked) {
            Ok(()) => Ok(Hold::Locked),
            Err(refused) => self
                .hold(window, Hold::Confined)
                .map(|()| Hold::Confined)
                .map_err(|confined| format!("locked: {refused}; confined: {confined}")),
        }
    }

    /// Shows the host's cursor over `window`, or hides it: `ShowCursor`'s
    /// count 0 or more, or below 0.
    pub fn cursor_shown(&self, window: u32, shown: bool) -> Result<(), String> {
        self.host(window, |host| host.set_cursor_visible(shown))
    }

    /// Asks `window` full screen, or framed again: one waiting is made
    /// so at the next pump; one made changes at once and, framed again,
    /// is put back at the program's rectangle. `Err` for a window never
    /// placed, or the host's reason it would not make one.
    pub fn full_screen(&self, window: u32, full: bool) -> Result<(), String> {
        let mut desktop = self.desktop.borrow_mut();
        let Some(placed) = desktop
            .placed
            .iter_mut()
            .find(|placed| placed.window == window)
        else {
            return Err(format!("window {window} not placed"));
        };
        if let Made::Refused(refusal) = &placed.made {
            return Err(refusal.to_string());
        }
        placed.full = full;
        if let Made::Window(host) = &placed.made {
            host.set_fullscreen(screen(full));
            if !full {
                host.set_outer_position(corner(placed.rect));
                let _ = host.request_inner_size(inside(placed.rect));
            }
        }
        Ok(())
    }
}

impl Screen for Native {
    fn size(&self) -> (i32, i32) {
        self.size
    }

    /// A window the host has made is placed at once, its new inside asked
    /// of the host, the host's answer never read back: the program keeps
    /// its own rectangle; a full screen one keeps the rectangle for when
    /// it leaves, the host asked only whether it shows. Any other is made
    /// at the next pump. A made window's pointer fit is made again for
    /// the rectangle's size on the inside last known. A hidden
    /// window's grab is released: X11 ends a grab whose window is not
    /// viewable.
    fn place(&self, window: u32, rect: Rect, shown: bool) {
        let mut desktop = self.desktop.borrow_mut();
        match desktop.placed.iter_mut().find(|was| was.window == window) {
            Some(was) => {
                was.rect = rect;
                was.shown = shown;
                if let Made::Window(host) = &was.made {
                    if !was.full {
                        host.set_outer_position(corner(rect));
                        let _ = host.request_inner_size(inside(rect));
                    }
                    host.set_visible(shown);
                }
                if matches!(was.made, Made::Window(_)) {
                    let inside = was.pointer.inside();
                    was.refit(inside);
                }
            }
            None => desktop.placed.push(Placed {
                window,
                rect,
                shown,
                full: false,
                made: Made::Waiting,
                pointer: PointerFit::new(rect, (0, 0)),
            }),
        }
        if !shown {
            desktop.twins.release(window);
        }
    }

    /// The host's window closes as it is let go, its grab with it.
    fn gone(&self, window: u32) {
        let mut desktop = self.desktop.borrow_mut();
        desktop.placed.retain(|placed| placed.window != window);
        desktop.twins.release(window);
    }

    /// One turn of winit's loop, waiting for nothing; the events it raised
    /// taken out before any is handed on, so a handler may place or let
    /// go.
    fn pump(&self, into: &mut dyn FnMut(Event)) {
        let mut desktop = self.desktop.borrow_mut();
        self.event_loop
            .borrow_mut()
            .pump_app_events(Some(Duration::ZERO), &mut *desktop);
        let events = core::mem::take(&mut desktop.events);
        drop(desktop);
        events.into_iter().for_each(into);
    }

    fn now(&self) -> u32 {
        u32::try_from(self.began.elapsed().as_millis()).unwrap_or(u32::MAX)
    }
}

/// The grab's double, replayed from Xvfb runs:
/// xdotool's 3 relative moves, (7, 3), (7, 3) and (-4, 9).
#[cfg(test)]
mod tests {
    use super::Twins;

    /// The moves `twins` hands on of `deltas`, in order.
    fn kept(twins: &mut Twins, deltas: &[(f64, f64)]) -> Vec<(f64, f64)> {
        deltas
            .iter()
            .copied()
            .filter(|&delta| twins.keep(delta))
            .collect()
    }

    /// A grabbed run: 6 `MouseMotion`s, each pair kept once. Moves 1 and 2 are
    /// equal, so 4 equal deltas in a row keep 2.
    #[test]
    fn grabbed_pairs_keep_one_each() {
        let mut twins = Twins::default();
        twins.arm(1);
        let read = [
            (7.0, 3.0),
            (7.0, 3.0),
            (7.0, 3.0),
            (7.0, 3.0),
            (-4.0, 9.0),
            (-4.0, 9.0),
        ];
        assert_eq!(
            kept(&mut twins, &read),
            [(7.0, 3.0), (7.0, 3.0), (-4.0, 9.0)]
        );
    }

    /// A free run: 3 `MouseMotion`s, unarmed, all kept; released, the same.
    #[test]
    fn free_moves_all_kept() {
        let read = [(7.0, 3.0), (7.0, 3.0), (-4.0, 9.0)];
        assert_eq!(kept(&mut Twins::default(), &read), read);
        let mut twins = Twins::default();
        twins.arm(1);
        twins.keep((7.0, 3.0));
        twins.release(1);
        assert_eq!(kept(&mut twins, &read), read);
    }

    /// The rule's case, unmeasured: a move read before the grab took
    /// comes once, its copy never coming, and the next move is a first.
    /// The deltas are a real run's, the lone one placed by hand.
    #[test]
    fn a_lone_first_leaves_the_next_a_first() {
        let mut twins = Twins::default();
        twins.arm(1);
        let read = [
            (-4.0, 9.0),
            (7.0, 3.0),
            (7.0, 3.0),
            (7.0, 3.0),
            (7.0, 3.0),
            (-4.0, 9.0),
            (-4.0, 9.0),
        ];
        assert_eq!(
            kept(&mut twins, &read),
            [(-4.0, 9.0), (7.0, 3.0), (7.0, 3.0), (-4.0, 9.0)]
        );
    }

    /// Another window's release leaves the grab armed.
    #[test]
    fn release_is_the_grabbing_windows() {
        let mut twins = Twins::default();
        twins.arm(1);
        twins.release(2);
        assert_eq!(kept(&mut twins, &[(7.0, 3.0), (7.0, 3.0)]), [(7.0, 3.0)]);
    }
}
