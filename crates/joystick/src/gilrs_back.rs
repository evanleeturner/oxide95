// SPDX-License-Identifier: MIT
//! The pads through `gilrs`, on Linux and macOS, behind the `gilrs` feature.
//!
//! It holds its library in a `RefCell`, so the holder is what keeps one thread in here at
//! a time: the layer's lock, as for any other back. The numbers come out as the page's
//! script gives them: the same order of buttons, with the four directions as a hat and
//! the two sticks as X, Y, R and U. `gilrs` has up as positive and these numbers have up
//! as low, so both Y axes turn over.
//!
//! No library at all is no pads and one line, never a stop: a pad only adds.
use core::cell::{Cell, RefCell};

use gilrs::{Axis, Button, Gamepad, GamepadId, Gilrs};

use crate::pads::{CENTRE, Pads, Reading, Shape, Span, has, hat, stick};

/// In the order of the standard mapping, the four directions taken out.
const BUTTONS: [Button; 13] = [
    Button::South,
    Button::East,
    Button::West,
    Button::North,
    Button::LeftTrigger,
    Button::RightTrigger,
    Button::LeftTrigger2,
    Button::RightTrigger2,
    Button::Select,
    Button::Start,
    Button::LeftThumb,
    Button::RightThumb,
    Button::Mode,
];

/// How far a hat that reports as an axis must lean to count as pressed.
const HAT_LEAN: f32 = 0.5;

/// Every pad here has the same shape, the one the script gives a standard-mapped pad.
const SHAPE: Shape = Shape {
    spans: [Span::FULL; 6],
    caps: has::R | has::U | has::POV | has::POV_FOUR_WAYS,
    axes: 4,
    #[allow(clippy::cast_possible_truncation)]
    buttons: BUTTONS.len() as u32,
};

/// The pads `gilrs` finds.
pub struct GilrsPads {
    gilrs: Option<RefCell<Gilrs>>,
    /// One more than the highest slot seen, which never goes down: as `crate::page`'s.
    seen: Cell<u32>,
}

impl GilrsPads {
    /// Starts the library; no library is no pads, and one line.
    #[must_use]
    pub fn new() -> Self {
        let gilrs = match Gilrs::new() {
            Ok(gilrs) => Some(RefCell::new(gilrs)),
            Err(refused) => {
                log::warn!(e:% = refused, by = "joystick"; "joy.gilrs.refused");
                None
            }
        };
        Self {
            gilrs,
            seen: Cell::new(0),
        }
    }

    /// Takes in what has happened since the last call, then asks `read` about the pads.
    fn after_events<T>(&self, read: impl FnOnce(&Gilrs) -> T) -> Option<T> {
        let mut gilrs = self.gilrs.as_ref()?.borrow_mut();
        while gilrs.next_event().is_some() {}
        Some(read(&gilrs))
    }
}

impl Default for GilrsPads {
    fn default() -> Self {
        Self::new()
    }
}

fn slot(id: GamepadId) -> usize {
    id.into()
}

fn connected(gilrs: &Gilrs, device: u32) -> Option<Gamepad<'_>> {
    let wanted = usize::try_from(device).ok()?;
    gilrs
        .gamepads()
        .find(|(id, _)| slot(*id) == wanted)
        .map(|(_, pad)| pad)
}

fn read(pad: &Gamepad<'_>) -> Reading {
    let mut buttons = 0;
    for (bit, button) in BUTTONS.into_iter().enumerate() {
        if pad.is_pressed(button) {
            buttons |= 1 << bit;
        }
    }
    let up = pad.is_pressed(Button::DPadUp) || pad.value(Axis::DPadY) > HAT_LEAN;
    let down = pad.is_pressed(Button::DPadDown) || pad.value(Axis::DPadY) < -HAT_LEAN;
    let right = pad.is_pressed(Button::DPadRight) || pad.value(Axis::DPadX) > HAT_LEAN;
    let left = pad.is_pressed(Button::DPadLeft) || pad.value(Axis::DPadX) < -HAT_LEAN;
    Reading {
        axes: [
            stick(pad.value(Axis::LeftStickX)),
            stick(-pad.value(Axis::LeftStickY)),
            CENTRE,
            stick(pad.value(Axis::RightStickX)),
            stick(-pad.value(Axis::RightStickY)),
            CENTRE,
        ],
        buttons,
        pressed: buttons.count_ones(),
        pov: hat(up, right, down, left),
    }
}

impl Pads for GilrsPads {
    fn devices(&self) -> u32 {
        let highest = self.after_events(|gilrs| {
            gilrs
                .gamepads()
                .map(|(id, _)| slot(id) + 1)
                .max()
                .unwrap_or(0)
        });
        if let Some(count) = highest.and_then(|count| u32::try_from(count).ok()) {
            self.seen.set(self.seen.get().max(count));
        }
        self.seen.get()
    }

    fn shape(&self, device: u32) -> Option<Shape> {
        self.after_events(|gilrs| connected(gilrs, device).map(|_| SHAPE))
            .flatten()
    }

    fn reading(&self, device: u32) -> Option<Reading> {
        self.after_events(|gilrs| connected(gilrs, device).map(|pad| read(&pad)))
            .flatten()
    }
}
