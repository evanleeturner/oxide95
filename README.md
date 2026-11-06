# oxide95

If you have rewritten an old 32-bit Windows program in Rust, it still expects
Windows to answer its calls: open this file, make this window, draw these
pixels, what time is it. oxide95 is a set of Rust libraries that answer those
calls themselves, so the rewritten program can run without Windows.

The libraries cover calls a Windows 95 era program makes to KERNEL32, USER32,
GDI32 and ADVAPI32 (the Windows system libraries for files, windows, drawing
and settings), plus the C library's memory helpers. Each call gives back what
the calling program reads from it and nothing more, so Windows itself is never
rebuilt. oxide95 cannot run an existing `.exe` file: your program has to be
source code that you compile against these libraries.

## What is here

| Library | What it answers |
| --- | --- |
| `kernel_files` | opening, reading, writing, moving and finding files |
| `kernel_memory` | `malloc`, `free` and the C library's other memory helpers |
| `kernel_process` | the program's command line, path, version and last error |
| `kernel_registry` | saved settings, kept in a text file or in memory |
| `kernel_text` | code pages and text conversion |
| `kernel_threads` | thread numbers, locks, per-thread slots and counters |
| `kernel_time` | clocks and dates |
| `cd-audio` | the calls a program makes to find its CD-audio disc; it reads no disc and plays no sound |
| `logical_disc` | a read-only copy of a CD: a cue sheet with its bin files, or an iso |
| `window` | windows and their message queues |
| `window-input` | the keyboard and the mouse |
| `window-dialogs` | menus and dialog boxes, laid out from the program's own files |
| `draw` | pixels and palettes |
| `draw-surfaces` | DirectDraw's surfaces and palette, painted through `draw` |
| `oxide_trace` | macros that log the values a rewritten function reads and writes |
| `oxide_logline` | writes those logs in one line format and checks them against `logbook.json` |

Each library keeps the part that answers Windows calls apart from the part
that talks to your platform, so the same answers serve a desktop, a browser
page or a test. Every call writes one log line naming the call, its numbers
and its answer, so a wrong answer shows up in the log.

Some calls and cases are not answered yet. `kernel_registry`, `kernel_text`,
`cd-audio`, `draw`, `draw-surfaces`, `window`, `window-dialogs` and `window-input` list
theirs at the top of `src/lib.rs`.

## Where it has been checked

The tests pass on Linux:

```bash
cargo test --workspace
```

The code also compiles for two more targets, `i686-pc-windows-gnu` (32-bit
Windows) and `wasm32-unknown-unknown` (a browser page), but those two commands
only check that it compiles and run nothing:

```bash
cargo clippy --workspace --all-targets --target i686-pc-windows-gnu
cargo check --workspace --target wasm32-unknown-unknown \
  --features kernel_threads/page,kernel_registry/page,window/page,window-input/desk,draw/page
```

macOS has never been built or tested.

## License

MIT. See `LICENSE`.
