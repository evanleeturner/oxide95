// SPDX-License-Identifier: MIT
//! The computer's own picture and key, by `softbuffer` and `winit` with no crate of this
//! workspace in the way, so a red here clears the project's code.
//!
//! Call: `computer_picture <seconds>`. It shows the worked picture in a window at 40,40,
//! prints `ready` once the picture has been presented, and stays up that long. The screen
//! is read from outside, by `xwd -root`, so this program says only the words a wrong
//! picture must carry. The keys come from outside too, by `xdotool`, which enters past the
//! device: nobody pressed one.
//!
//! Prints one `key` line a key and one `CK1` line, on stdout. Exits 0 on a pass, 1 on red,
//! 2 on a call it cannot read.
//!
//! `CK1`: each key sent from outside reads down and then up in the window.
//! `CV1`: the worked picture shown in the window, as the screen read from outside holds it.
use std::num::NonZeroU32;
use std::process::ExitCode;
use std::rc::Rc;
use std::time::{Duration, Instant};

use softbuffer::{Context, Surface};
use winit::application::ApplicationHandler;
use winit::dpi::{PhysicalPosition, PhysicalSize};
use winit::event::{ElementState, KeyEvent, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::window::{Window, WindowId};

#[path = "../bench/worked_picture.rs"]
mod worked_picture;

use worked_picture::{HEIGHT, KEY_RED, LEFT, PICTURE_RED, TOP, WIDTH, picture};

const TITLE: &str = "computer_picture";
const TURN: Duration = Duration::from_millis(20);
const KEYS: [(KeyCode, &str); 3] = [
    (KeyCode::KeyA, "KeyA"),
    (KeyCode::Escape, "Escape"),
    (KeyCode::ShiftLeft, "ShiftLeft"),
];

/// The window and the surface drawn on it.
struct Shown {
    window: Rc<Window>,
    surface: Surface<Rc<Window>, Rc<Window>>,
}

/// What one run has shown and heard.
struct Check {
    picture: Vec<u32>,
    until: Instant,
    shown: Option<Shown>,
    ready: bool,
    presses: [u32; KEYS.len()],
    releases: [u32; KEYS.len()],
    refused: Option<String>,
}

/// Opens the window and its surface; a screen `softbuffer` will not draw on is an `Err`.
fn open(event_loop: &ActiveEventLoop) -> Result<Shown, String> {
    let attributes = Window::default_attributes()
        .with_title(TITLE)
        .with_inner_size(PhysicalSize::new(WIDTH, HEIGHT))
        .with_position(PhysicalPosition::new(LEFT, TOP))
        .with_resizable(false);
    let window = event_loop
        .create_window(attributes)
        .map_err(|why| format!("window: {why}"))?;
    let window = Rc::new(window);
    let context = Context::new(window.clone()).map_err(|why| format!("context: {why}"))?;
    let mut surface =
        Surface::new(&context, window.clone()).map_err(|why| format!("surface: {why}"))?;
    let (Some(width), Some(height)) = (NonZeroU32::new(WIDTH), NonZeroU32::new(HEIGHT)) else {
        return Err("the picture has no size".to_owned());
    };
    surface
        .resize(width, height)
        .map_err(|why| format!("resize: {why}"))?;
    Ok(Shown { window, surface })
}

impl Check {
    fn new(seconds: u64) -> Self {
        Self {
            picture: picture(),
            until: Instant::now() + Duration::from_secs(seconds),
            shown: None,
            ready: false,
            presses: [0; KEYS.len()],
            releases: [0; KEYS.len()],
            refused: None,
        }
    }

    /// Presents the picture, and says `ready` after the first time it has gone out.
    fn paint(&mut self) -> Result<(), String> {
        let Some(shown) = self.shown.as_mut() else {
            return Ok(());
        };
        let mut buffer = shown
            .surface
            .buffer_mut()
            .map_err(|why| format!("buffer: {why}"))?;
        buffer.copy_from_slice(&self.picture);
        buffer.present().map_err(|why| format!("present: {why}"))?;
        if !self.ready {
            self.ready = true;
            println!("CV1 if-wrong {PICTURE_RED}");
            println!("ready");
        }
        Ok(())
    }

    /// Counts a key's press or release; a repeat is the same press still held.
    fn note_key(&mut self, event: &KeyEvent) {
        if event.repeat {
            return;
        }
        let PhysicalKey::Code(code) = event.physical_key else {
            return;
        };
        let Some(index) = KEYS.iter().position(|(key, _)| *key == code) else {
            return;
        };
        match event.state {
            ElementState::Pressed => self.presses[index] += 1,
            ElementState::Released => self.releases[index] += 1,
        }
    }

    /// Prints what the run heard, and makes the exit code.
    fn finish(&self) -> ExitCode {
        if let Some(why) = &self.refused {
            println!("CV1 refused {why}");
            println!("CV1 red {PICTURE_RED}");
            return ExitCode::FAILURE;
        }
        let mut whole = true;
        for (index, (_, name)) in KEYS.iter().enumerate() {
            let (presses, releases) = (self.presses[index], self.releases[index]);
            println!("key {name} press={presses} release={releases}");
            whole &= presses == 1 && releases == 1;
        }
        if whole {
            println!("CK1 pass");
            ExitCode::SUCCESS
        } else {
            println!("CK1 red {KEY_RED}");
            ExitCode::FAILURE
        }
    }
}

impl ApplicationHandler for Check {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.shown.is_some() {
            return;
        }
        match open(event_loop) {
            Ok(shown) => self.shown = Some(shown),
            Err(why) => {
                self.refused = Some(why);
                event_loop.exit();
            }
        }
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::RedrawRequested => {
                if let Err(why) = self.paint() {
                    self.refused = Some(why);
                    event_loop.exit();
                }
            }
            WindowEvent::KeyboardInput { event, .. } => self.note_key(&event),
            _ => {}
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        let now = Instant::now();
        if now >= self.until {
            event_loop.exit();
            return;
        }
        if let Some(shown) = &self.shown {
            shown.window.request_redraw();
        }
        event_loop.set_control_flow(ControlFlow::WaitUntil(now + TURN));
    }
}

fn main() -> ExitCode {
    let mut args = std::env::args().skip(1);
    let (Some(seconds), None) = (args.next().and_then(|text| text.parse().ok()), args.next())
    else {
        eprintln!("usage: computer_picture <seconds>");
        return ExitCode::from(2);
    };
    let mut check = Check::new(seconds);
    let run = EventLoop::new()
        .map_err(|why| why.to_string())
        .and_then(|event_loop| {
            event_loop
                .run_app(&mut check)
                .map_err(|why| why.to_string())
        });
    if let Err(why) = run {
        check.refused = Some(format!("event loop: {why}"));
    }
    check.finish()
}
