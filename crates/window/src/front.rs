// SPDX-License-Identifier: MIT
//! The 24 fronts: one file's window and message calls over its program's
//! [`Desk`], each answering in the file's numbers with one `imp.call`
//! line. A procedure is the program's code: a front that calls one is
//! handed the caller, which runs it by its address.

use alloc::vec;

use crate::desk::{Class, Desk, Window};
use crate::table::Row;
use crate::{Bytes, Message, Numbers, Rect, Screen, bits};

/// The messages a front queues or answers, Windows' numbers.
const WM_CLOSE: u32 = 0x10;
const WM_QUIT: u32 = 0x12;
const WM_NCHITTEST: u32 = 0x84;
const WM_NCACTIVATE: u32 = 0x86;
/// The key messages `TranslateMessage` reads, and the characters it posts.
const WM_KEYDOWN: u32 = 0x100;
const WM_KEYUP: u32 = 0x101;
const WM_CHAR: u32 = 0x102;
const WM_SYSKEYDOWN: u32 = 0x104;
const WM_SYSKEYUP: u32 = 0x105;
const WM_SYSCHAR: u32 = 0x106;
/// `HTCLIENT`: every point is the client area's, the stand-in drawing no
/// frame.
const HTCLIENT: u32 = 1;
/// `PeekMessageA`'s `PM_REMOVE`.
const PM_REMOVE: u32 = 1;
/// `ShowWindow`'s `SW_HIDE`; every other command shows.
const SW_HIDE: i32 = 0;
/// `ShowWindow`'s commands that leave the focus where it is:
/// `SW_SHOWNOACTIVATE`, `SW_MINIMIZE`, `SW_SHOWMINNOACTIVE`, `SW_SHOWNA` and
/// `SW_FORCEMINIMIZE`.
const SW_LEAVING_FOCUS: [i32; 5] = [4, 6, 7, 8, 11];
/// `CW_USEDEFAULT`: the host's choice of place or size.
const CW_USEDEFAULT: i32 = i32::MIN;
/// The `GetWindowLongA` indexes the window keeps.
const GWL_WNDPROC: i32 = -4;
const GWL_STYLE: i32 = -16;
const GWL_EXSTYLE: i32 = -20;
const GWL_USERDATA: i32 = -21;
/// `SetWindowPos`' flags the desk reads.
const SWP_NOSIZE: u32 = 0x1;
const SWP_NOMOVE: u32 = 0x2;
const SWP_SHOWWINDOW: u32 = 0x40;
const SWP_HIDEWINDOW: u32 = 0x80;
/// The style bits a front reads or sets.
const WS_POPUP: u32 = 0x8000_0000;
const WS_CHILD: u32 = 0x4000_0000;
const WS_VISIBLE: u32 = 0x1000_0000;
const WS_CLIPSIBLINGS: u32 = 0x0400_0000;
const WS_CAPTION: u32 = 0x00c0_0000;
const WS_BORDER: u32 = 0x0080_0000;
const WS_DLGFRAME: u32 = 0x0040_0000;
const WS_THICKFRAME: u32 = 0x0004_0000;
/// The metrics `AdjustWindowRect` grows a rectangle by.
const SM_CYCAPTION: i32 = 4;
const SM_CXBORDER: i32 = 5;
const SM_CXDLGFRAME: i32 = 7;
const SM_CYMENU: i32 = 15;
const SM_CXFRAME: i32 = 32;

/// The window the focus passes to when its holder hides or goes: the newest
/// shown window that is no child, but `leaving`; 0 for none.
fn next_focus(state: &crate::desk::State, leaving: u32) -> u32 {
    state
        .windows
        .iter()
        .rev()
        .find(|row| row.handle != leaving && row.shown && row.style & WS_CHILD == 0)
        .map_or(0, |row| row.handle)
}

/// What `CreateWindowExA` is given.
#[derive(Clone, Copy, Debug)]
pub struct Create<'a> {
    pub ex_style: u32,
    pub class: &'a [u8],
    pub title: &'a [u8],
    pub style: u32,
    /// Left, top, width and height, each maybe `CW_USEDEFAULT`.
    pub place: [i32; 4],
    pub parent: u32,
    pub menu: u32,
    pub param: u32,
}

/// One file's window calls: its row, and its program's desk.
pub struct Front<'d, S> {
    file: &'static Row,
    desk: &'d Desk<S>,
}

impl<'d, S: Screen> Front<'d, S> {
    pub const fn new(file: &'static Row, desk: &'d Desk<S>) -> Self {
        Self { file, desk }
    }

    /// Writes the call's one line: `a`, `s` and `o` only when the call
    /// has them, as the layer's lines show them.
    fn line(&self, call: &str, arguments: &[u32], given: &[u8], wrote: &[u32], answer: u32) {
        let (a, s, o) = (Numbers(arguments), Bytes(given), Numbers(wrote));
        let v = oxide_trace::__shown!(answer);
        self.desk.screen().quiet(|| {
            match (arguments.is_empty(), given.is_empty(), wrote.is_empty()) {
                (true, true, true) => log::debug!(c = call, v:% = v, by = "window"; "imp.call"),
                (false, true, true) => log::debug!(c = call, a:% = a, v:% = v,
                    by = "window"; "imp.call"),
                (false, false, true) => {
                    log::debug!(c = call, a:% = a, s:% = s, v:% = v, by = "window"; "imp.call");
                }
                (false, true, false) => {
                    log::debug!(c = call, a:% = a, o:% = o, v:% = v, by = "window"; "imp.call");
                }
                _ => log::debug!(c = call, a:% = a, s:% = s, o:% = o, v:% = v,
                    by = "window"; "imp.call"),
            }
        });
    }

    /// The answer `GetSystemMetrics` gives `index`.
    fn metric(&self, index: i32) -> i32 {
        let (width, height) = self.desk.screen().size();
        match index {
            0 => width,
            1 => height,
            _ => self
                .file
                .metrics
                .iter()
                .find(|(at, _)| *at == index)
                .map_or(0, |(_, answer)| *answer),
        }
    }

    /// `RegisterClassA`: the class `name`, its atom; 0 for a name taken.
    /// Callers: the launcher.
    #[must_use]
    pub fn register_class(
        &self,
        style: u32,
        procedure: u32,
        extra: u32,
        icon: u32,
        name: &[u8],
    ) -> u32 {
        let mut state = self.desk.state();
        let atom = if state.class(name).is_some() {
            0
        } else {
            let atom = state.atom();
            state.classes.push(Class {
                atom,
                name: name.to_vec(),
                style,
                procedure,
                extra,
                icon,
            });
            atom
        };
        drop(state);
        self.line(
            self.file.calls.register_class,
            &[style, procedure, extra, icon],
            name,
            &[],
            atom,
        );
        atom
    }

    /// `CreateWindowExA`: a window of a registered class, its handle; 0
    /// for a class not registered. A top-level window takes
    /// `WS_CLIPSIBLINGS`, and one neither popup nor child `WS_CAPTION`, as
    /// Windows gives them. Callers: the shell, the game file, the
    /// launcher.
    #[must_use]
    pub fn create_window(&self, create: &Create<'_>) -> u32 {
        let (width, height) = self.desk.screen().size();
        let [mut left, mut top, mut wide, mut high] = create.place;
        if left == CW_USEDEFAULT {
            (left, top) = (0, 0);
        }
        if wide == CW_USEDEFAULT {
            (wide, high) = (width, height);
        }
        let mut style = create.style;
        if create.parent == 0 {
            style |= WS_CLIPSIBLINGS;
            if style & (WS_POPUP | WS_CHILD) == 0 {
                style |= WS_CAPTION;
            }
        }
        let rect = Rect::sized(left, top, wide, high);
        let mut state = self.desk.state();
        let made = state
            .class(create.class)
            .map(|class| (class.procedure, class.extra));
        let handle = made.map_or(0, |(procedure, extra)| {
            let handle = state.handle();
            state.windows.push(Window {
                handle,
                class: create.class.to_vec(),
                procedure,
                rect,
                style,
                ex_style: create.ex_style,
                user: 0,
                longs: vec![0; usize::try_from(extra / 4).unwrap_or(0)],
                title: create.title.to_vec(),
                parent: create.parent,
                shown: style & WS_VISIBLE != 0,
            });
            if style & WS_VISIBLE != 0 && style & WS_CHILD == 0 {
                state.focus = handle;
            }
            handle
        });
        drop(state);
        if handle != 0 && style & WS_VISIBLE != 0 {
            self.desk.screen().place(handle, rect, true);
        }
        let arguments = [
            create.ex_style,
            create.style,
            bits(create.place[0]),
            bits(create.place[1]),
            bits(create.place[2]),
            bits(create.place[3]),
            create.parent,
            create.menu,
            create.param,
        ];
        self.line(
            self.file.calls.create_window,
            &arguments,
            create.class,
            &[],
            handle,
        );
        handle
    }

    /// `DestroyWindow`: 1, the window gone; 0 for one not here. Callers:
    /// the launcher.
    #[must_use]
    pub fn destroy_window(&self, window: u32) -> u32 {
        let answer = self.destroy(window);
        self.line(self.file.calls.destroy_window, &[window], &[], &[], answer);
        answer
    }

    fn destroy(&self, window: u32) -> u32 {
        let mut state = self.desk.state();
        let before = state.windows.len();
        state.windows.retain(|kept| kept.handle != window);
        let gone = state.windows.len() < before;
        if gone && state.focus == window {
            state.focus = next_focus(&state, window);
        }
        drop(state);
        if gone {
            self.desk.screen().gone(window);
        }
        u32::from(gone)
    }

    /// `FindWindowA`: the newest window of the class and title asked, each
    /// `None` for any, as a new window goes on top of its type; 0 for none.
    /// Callers: the launcher.
    #[must_use]
    pub fn find_window(&self, class: Option<&[u8]>, title: Option<&[u8]>) -> u32 {
        let state = self.desk.state();
        let found = state
            .windows
            .iter()
            .rev()
            .find(|window| {
                class.is_none_or(|class| window.class.eq_ignore_ascii_case(class))
                    && title.is_none_or(|title| window.title == title)
            })
            .map_or(0, |window| window.handle);
        drop(state);
        self.line(
            self.file.calls.find_window,
            &[],
            class.unwrap_or_default(),
            &[],
            found,
        );
        found
    }

    /// `LoadIconA`: a fresh handle; an icon is drawn elsewhere, never
    /// here. Callers: the launcher.
    #[must_use]
    pub fn load_icon(&self, instance: u32, name: u32) -> u32 {
        let handle = self.desk.state().handle();
        self.line(
            self.file.calls.load_icon,
            &[instance, name],
            &[],
            &[],
            handle,
        );
        handle
    }

    /// `ShowWindow`: 1 when the window was shown before, else 0; hidden
    /// for `SW_HIDE`, else shown. A window with no parent takes the focus
    /// as it is shown, but for the commands in [`SW_LEAVING_FOCUS`]; one
    /// hidden while holding it passes it on. Callers: the shell, the game
    /// file, the launcher.
    #[must_use]
    pub fn show_window(&self, window: u32, command: i32) -> u32 {
        let shown = command != SW_HIDE;
        let mut state = self.desk.state();
        let before = state.window_mut(window).map(|row| {
            let before = row.shown;
            row.shown = shown;
            if shown {
                row.style |= WS_VISIBLE;
            } else {
                row.style &= !WS_VISIBLE;
            }
            (before, row.rect)
        });
        let child = state
            .window(window)
            .is_none_or(|row| row.style & WS_CHILD != 0);
        if !child && shown && !SW_LEAVING_FOCUS.contains(&command) {
            state.focus = window;
        } else if !shown && state.focus == window {
            state.focus = next_focus(&state, window);
        }
        drop(state);
        if let Some((_, rect)) = before {
            self.desk.screen().place(window, rect, shown);
        }
        let answer = u32::from(before.is_some_and(|(before, _)| before));
        self.line(
            self.file.calls.show_window,
            &[window, bits(command)],
            &[],
            &[],
            answer,
        );
        answer
    }

    /// `SetWindowPos`: 1, the window moved and sized unless the flags say
    /// not, shown or hidden when they say; 0 for a window not here. The
    /// window it goes after is logged, z-order not kept. Callers: the
    /// shell, the game file.
    #[must_use]
    pub fn set_window_pos(&self, window: u32, after: u32, place: Rect, flags: u32) -> u32 {
        let mut state = self.desk.state();
        let placed = state.window_mut(window).map(|row| {
            if flags & SWP_NOMOVE == 0 {
                let (wide, high) = (
                    row.rect.right - row.rect.left,
                    row.rect.bottom - row.rect.top,
                );
                row.rect = Rect::sized(place.left, place.top, wide, high);
            }
            if flags & SWP_NOSIZE == 0 {
                row.rect.right = row.rect.left + (place.right - place.left);
                row.rect.bottom = row.rect.top + (place.bottom - place.top);
            }
            if flags & SWP_SHOWWINDOW != 0 {
                row.shown = true;
                row.style |= WS_VISIBLE;
            } else if flags & SWP_HIDEWINDOW != 0 {
                row.shown = false;
                row.style &= !WS_VISIBLE;
            }
            (row.rect, row.shown)
        });
        drop(state);
        if let Some((rect, shown)) = placed {
            self.desk.screen().place(window, rect, shown);
        }
        let answer = u32::from(placed.is_some());
        let arguments = [
            window,
            after,
            bits(place.left),
            bits(place.top),
            bits(place.right - place.left),
            bits(place.bottom - place.top),
            flags,
        ];
        self.line(self.file.calls.set_window_pos, &arguments, &[], &[], answer);
        answer
    }

    /// The frame's width and the caption's height that `style` names, by
    /// this row's metrics.
    fn frame(&self, style: u32) -> (i32, i32) {
        let frame = if style & WS_THICKFRAME != 0 {
            self.metric(SM_CXFRAME)
        } else if style & WS_DLGFRAME != 0 {
            self.metric(SM_CXDLGFRAME)
        } else if style & WS_BORDER != 0 {
            self.metric(SM_CXBORDER)
        } else {
            0
        };
        let caption = if style & WS_CAPTION == WS_CAPTION {
            self.metric(SM_CYCAPTION)
        } else {
            0
        };
        (frame, caption)
    }

    /// `AdjustWindowRect`: the window rectangle around the client area
    /// `rect`, grown by the frame, the caption and the menu the style
    /// names, by this row's metrics. Callers: the shell, the game file,
    /// the launcher.
    #[must_use]
    pub fn adjust_window_rect(&self, rect: Rect, style: u32, menu: bool) -> Rect {
        let (frame, caption) = self.frame(style);
        let bar = if menu { self.metric(SM_CYMENU) } else { 0 };
        let grown = Rect {
            left: rect.left - frame,
            top: rect.top - frame - caption - bar,
            right: rect.right + frame,
            bottom: rect.bottom + frame,
        };
        let arguments = [
            bits(rect.left),
            bits(rect.top),
            bits(rect.right),
            bits(rect.bottom),
            style,
            u32::from(menu),
        ];
        let wrote = [
            bits(grown.left),
            bits(grown.top),
            bits(grown.right),
            bits(grown.bottom),
        ];
        self.line(
            self.file.calls.adjust_window_rect,
            &arguments,
            &[],
            &wrote,
            1,
        );
        grown
    }

    /// `GetWindowRect`: the window's rectangle; `None` for a window not
    /// here. Callers: the shell, the game file.
    #[must_use]
    pub fn get_window_rect(&self, window: u32) -> Option<Rect> {
        let rect = self.desk.state().window(window).map(|row| row.rect);
        let wrote = rect.map_or([0; 4], |rect| {
            [
                bits(rect.left),
                bits(rect.top),
                bits(rect.right),
                bits(rect.bottom),
            ]
        });
        let shown = if rect.is_some() { &wrote[..] } else { &[] };
        self.line(
            self.file.calls.get_window_rect,
            &[window],
            &[],
            shown,
            u32::from(rect.is_some()),
        );
        rect
    }

    /// The long `index` names: the procedure, the style, the extended
    /// style, the user's word, or a long of the class's extra bytes.
    fn long(row: &mut Window, index: i32) -> Option<&mut u32> {
        match index {
            GWL_WNDPROC => Some(&mut row.procedure),
            GWL_STYLE => Some(&mut row.style),
            GWL_EXSTYLE => Some(&mut row.ex_style),
            GWL_USERDATA => Some(&mut row.user),
            _ if index >= 0 && index % 4 == 0 => usize::try_from(index / 4)
                .ok()
                .and_then(|at| row.longs.get_mut(at)),
            _ => None,
        }
    }

    /// `GetWindowLongA`: the long `index` names; 0 for a window not here
    /// or an index it does not keep. Callers: the shell, the game file.
    #[must_use]
    pub fn get_window_long(&self, window: u32, index: i32) -> u32 {
        let mut state = self.desk.state();
        let answer = state
            .window_mut(window)
            .and_then(|row| Self::long(row, index).map(|long| *long))
            .unwrap_or(0);
        drop(state);
        self.line(
            self.file.calls.get_window_long,
            &[window, bits(index)],
            &[],
            &[],
            answer,
        );
        answer
    }

    /// `SetWindowLongA`: keeps `value` as the long `index` names, its
    /// value before the answer; 0 for a window not here or an index it
    /// does not keep. A style changed this way shows and hides nothing.
    /// Callers: the shell, the game file.
    #[must_use]
    pub fn set_window_long(&self, window: u32, index: i32, value: u32) -> u32 {
        let mut state = self.desk.state();
        let answer = state
            .window_mut(window)
            .and_then(|row| Self::long(row, index).map(|long| core::mem::replace(long, value)))
            .unwrap_or(0);
        drop(state);
        self.line(
            self.file.calls.set_window_long,
            &[window, bits(index), value],
            &[],
            &[],
            answer,
        );
        answer
    }

    /// The window's client origin on the screen: its rectangle's corner,
    /// past the frame and caption its style names. With none, the corner.
    fn origin(&self, window: u32) -> Option<(i32, i32)> {
        let kept = self
            .desk
            .state()
            .window(window)
            .map(|row| (row.rect, row.style));
        kept.map(|(rect, style)| {
            let (frame, caption) = self.frame(style);
            (rect.left + frame, rect.top + frame + caption)
        })
    }

    /// `ClientToScreen`: the client point `x`, `y` on the screen; `None`
    /// for a window not here. Callers: the shell, the game file.
    #[must_use]
    pub fn client_to_screen(&self, window: u32, x: i32, y: i32) -> Option<(i32, i32)> {
        let point = self.origin(window).map(|(left, top)| (x + left, y + top));
        self.point_line(self.file.calls.client_to_screen, window, (x, y), point);
        point
    }

    /// `ScreenToClient`: the screen point `x`, `y` in the window's client
    /// area; `None` for a window not here. Callers: the shell, the game
    /// file.
    #[must_use]
    pub fn screen_to_client(&self, window: u32, x: i32, y: i32) -> Option<(i32, i32)> {
        let point = self.origin(window).map(|(left, top)| (x - left, y - top));
        self.point_line(self.file.calls.screen_to_client, window, (x, y), point);
        point
    }

    fn point_line(&self, call: &str, window: u32, given: (i32, i32), point: Option<(i32, i32)>) {
        let wrote = point.map_or([0; 2], |(x, y)| [bits(x), bits(y)]);
        let shown = if point.is_some() { &wrote[..] } else { &[] };
        let arguments = [window, bits(given.0), bits(given.1)];
        self.line(call, &arguments, &[], shown, u32::from(point.is_some()));
    }

    /// `SetFocus`: the window holding the focus before, 0 none; the focus
    /// kept unchanged for a window not here. Callers: the shell.
    #[must_use]
    pub fn set_focus(&self, window: u32) -> u32 {
        let mut state = self.desk.state();
        let before = state.focus;
        if window == 0 || state.window(window).is_some() {
            state.focus = window;
        }
        drop(state);
        self.line(self.file.calls.set_focus, &[window], &[], &[], before);
        before
    }

    /// `GetSystemMetrics`: indexes 0 and 1 the screen's width and height
    /// from the back; any other the row's answer, 0 for one it does not
    /// name. Callers: the shell, the game file, the launcher.
    #[must_use]
    pub fn get_system_metrics(&self, index: i32) -> i32 {
        let answer = self.metric(index);
        self.line(
            self.file.calls.get_system_metrics,
            &[bits(index)],
            &[],
            &[],
            bits(answer),
        );
        answer
    }

    /// `DefWindowProcA`: `WM_NCHITTEST` answers `HTCLIENT`,
    /// `WM_NCACTIVATE` 1, `WM_CLOSE` destroys the window and answers 0;
    /// any other message 0. Callers: the shell, the game file, the
    /// launcher.
    #[must_use]
    pub fn def_window_proc(&self, window: u32, message: u32, w: u32, l: u32) -> u32 {
        let answer = match message {
            WM_NCHITTEST => HTCLIENT,
            WM_NCACTIVATE => 1,
            WM_CLOSE => {
                self.destroy(window);
                0
            }
            _ => 0,
        };
        self.line(
            self.file.calls.def_window_proc,
            &[window, message, w, l],
            &[],
            &[],
            answer,
        );
        answer
    }

    /// `PeekMessageA`: the back pumped, then the first queued message for
    /// `window` (0 any) numbered `first` to `last` (both 0 any), taken
    /// off the queue when `remove` holds `PM_REMOVE`; `None` when none
    /// waits. Callers: the shell, the game file.
    #[must_use]
    pub fn peek_message(&self, window: u32, first: u32, last: u32, remove: u32) -> Option<Message> {
        self.desk.pump();
        let mut state = self.desk.state();
        let at = state.queue.iter().position(|queued| {
            (window == 0 || queued.window == window)
                && ((first == 0 && last == 0) || (first..=last).contains(&queued.message))
        });
        let found = at.and_then(|at| {
            if remove & PM_REMOVE == 0 {
                state.queue.get(at).copied()
            } else {
                state.queue.remove(at)
            }
        });
        if let Some(message) = found {
            state.time = message.time;
        }
        drop(state);
        let wrote = found.map(|message| message.fields()).unwrap_or_default();
        let shown = if found.is_some() { &wrote[..] } else { &[] };
        let answer = u32::from(found.is_some());
        self.line(
            self.file.calls.peek_message,
            &[window, first, last, remove],
            &[],
            shown,
            answer,
        );
        found
    }

    /// `WaitMessage`: the back pumped, then 1 at once; a page cannot wait,
    /// and the program's loop peeks again. Callers: the shell, the game
    /// file.
    #[must_use]
    pub fn wait_message(&self) -> u32 {
        self.desk.pump();
        self.line(self.file.calls.wait_message, &[], &[], &[], 1);
        1
    }

    /// `TranslateMessage`: a key down's character, the back's
    /// [`Screen::character`], posted as `WM_CHAR` (`WM_SYSCHAR` from a
    /// `WM_SYSKEYDOWN`) to be taken next, as Windows reads its posted
    /// messages before its input; none for a key typing none. 1 for any of
    /// the 4 key messages, else 0, as Windows answers. Callers: the shell,
    /// the game file.
    #[must_use]
    pub fn translate_message(&self, message: &Message) -> u32 {
        let typed = match message.message {
            WM_KEYDOWN => Some(WM_CHAR),
            WM_SYSKEYDOWN => Some(WM_SYSCHAR),
            _ => None,
        };
        let character = typed.map_or(0, |_| self.desk.screen().character(message.w));
        if let (Some(posted), true) = (typed, character != 0) {
            self.desk.state().queue.push_front(Message {
                message: posted,
                w: character,
                ..*message
            });
        }
        let answer = u32::from(matches!(
            message.message,
            WM_KEYDOWN | WM_KEYUP | WM_SYSKEYDOWN | WM_SYSKEYUP
        ));
        self.line(
            self.file.calls.translate_message,
            &message.fields(),
            &[],
            &[],
            answer,
        );
        answer
    }

    /// The procedure of `window`, read before any call: the call may
    /// change the tables.
    fn procedure(&self, window: u32) -> Option<u32> {
        self.desk.state().window(window).map(|row| row.procedure)
    }

    /// `DispatchMessageA`: the window's procedure called with `message`
    /// by `call`, its answer; 0 for a window not here. Callers: the
    /// shell, the game file.
    pub fn dispatch_message(
        &self,
        message: &Message,
        call: impl FnOnce(u32, &Message) -> u32,
    ) -> u32 {
        let answer = self
            .procedure(message.window)
            .map_or(0, |procedure| call(procedure, message));
        self.line(
            self.file.calls.dispatch_message,
            &message.fields(),
            &[],
            &[],
            answer,
        );
        answer
    }

    /// `SendMessageA`: the window's procedure called at once by `call`,
    /// its answer; 0 for a window not here. Callers: the shell, the game
    /// file.
    pub fn send_message(
        &self,
        window: u32,
        message: u32,
        w: u32,
        l: u32,
        call: impl FnOnce(u32, &Message) -> u32,
    ) -> u32 {
        let sent = Message {
            window,
            message,
            w,
            l,
            time: self.desk.screen().now(),
            ..Message::default()
        };
        let answer = self
            .procedure(window)
            .map_or(0, |procedure| call(procedure, &sent));
        self.line(
            self.file.calls.send_message,
            &[window, message, w, l],
            &[],
            &[],
            answer,
        );
        answer
    }

    /// Queues one message, stamped with the back's time and its pointer's
    /// point.
    fn queue(&self, window: u32, message: u32, w: u32, l: u32) {
        let time = self.desk.screen().now();
        let (x, y) = self.desk.screen().point();
        self.desk.state().queue.push_back(Message {
            window,
            message,
            w,
            l,
            time,
            x,
            y,
        });
    }

    /// `PostMessageA`: the message queued, 1; 0 for a window not here,
    /// window 0 the program's own queue. Callers: the shell.
    #[must_use]
    pub fn post_message(&self, window: u32, message: u32, w: u32, l: u32) -> u32 {
        let here = window == 0 || self.desk.state().window(window).is_some();
        if here {
            self.queue(window, message, w, l);
        }
        let answer = u32::from(here);
        self.line(
            self.file.calls.post_message,
            &[window, message, w, l],
            &[],
            &[],
            answer,
        );
        answer
    }

    /// `PostQuitMessage`: `WM_QUIT` queued with the code, for the loop to
    /// take. Callers: the shell, the game file.
    pub fn post_quit_message(&self, code: u32) {
        self.queue(0, WM_QUIT, code, 0);
        self.line(self.file.calls.post_quit_message, &[code], &[], &[], 0);
    }

    /// `GetMessageTime`: the time of the last message `PeekMessageA`
    /// took, 0 before any. Callers: the game file.
    #[must_use]
    pub fn get_message_time(&self) -> u32 {
        let answer = self.desk.state().time;
        self.line(self.file.calls.get_message_time, &[], &[], &[], answer);
        answer
    }
}
