# oxide95

oxide95 is a set of Rust libraries for people who are rewriting an old
Windows game in Rust and want it to run on a computer today.

A game made for Windows 95 leans on Windows for almost everything. To open
a file, put a window on the screen, draw a picture, play a sound, read the
joystick or check the time, it asks Windows and waits for the answer. A
rewritten game still asks those same questions. oxide95 answers them in
Windows' place, so the game can run on Linux, in a web browser or inside a
test, with no Windows underneath. For example, when the game reaches for
its CD, `logical_disc` hands it a copy of the disc saved as a file.

oxide95 can't run a game's original `.exe` file. The game has to be source
code (the text a program is built from) that you build together with these
libraries.

## What is here

| Libraries | What they answer for the game |
| --- | --- |
| `kernel_files`, `kernel_memory`, `kernel_process`, `kernel_registry`, `kernel_text`, `kernel_threads`, `kernel_time` | files, memory, the command line, saved settings, text, threads and clocks |
| `window`, `window-input`, `window-dialogs` | windows, the keyboard and mouse, menus and dialog boxes |
| `draw`, `draw-surfaces` | pixels, colour palettes and DirectDraw, the drawing system Windows had then |
| `sound-mix`, `sound-buffers`, `cd-audio` | sound through Miles Sound System and DirectSound, two sound systems games of that time used, and music from the game's CD |
| `video`, `video_smacker` | video clips in Smacker, a video format games of that time used |
| `joystick` | joysticks and game pads |
| `logical_disc` | a read-only copy of a CD, from a `.cue` and `.bin` pair or an `.iso` file |
| `oxide_trace`, `oxide_logline`, `oxide_trace_check` | the log, and tests of the log |
| `layer-check`, `web-shell` | small test programs and pages that check a computer's sound, screen, keys and basic parts with none of the other libraries' code |

Every call writes one line to a log: the call, what the game asked, and
what came back. When the game goes wrong, the log shows which answer was
wrong. Each library also keeps the part that answers the game apart from
the part that talks to your computer, so the same answers work on a
desktop, in a web page or in a test.

## How far along it is

This is early work. It has been tested on one Linux computer, both
directly and under Wine (a program that runs Windows software on Linux),
and in a web browser. It has not been tried on real Windows yet, no person
has heard its sound through a speaker yet, and it has never been built on
macOS. Libraries that leave some calls unanswered list them at the top of
their `src/lib.rs`.

To run the tests:

```bash
cargo test --workspace
```

## License

MIT. See `LICENSE`.
