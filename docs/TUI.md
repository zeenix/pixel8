# The terminal frontend (`pixel8-tui`)

The whole console — boot prompt, all five editors, running carts — can run inside a
terminal instead of a window, via the separate `pixel8-tui` binary:

```text
cargo install pixel8-tui
pixel8-tui                     boot the console in the terminal
pixel8-tui mygame              boot with a project or cart loaded
pixel8-tui run mygame.png      boot, load, and run immediately
```

**Linux requirement**: game input needs real key press/release events, which most
terminals cannot report. Either use a terminal that speaks the kitty keyboard
protocol (kitty, foot, alacritty, WezTerm, ghostty...), or grant yourself read
access to the kernel's input devices — a one-time step:

```text
sudo usermod -aG input $USER
newgrp input      # applies it to the current shell; new logins have it automatically
```

Without either, `pixel8-tui` refuses to start (see [Input](#input) for the
degraded-input escape hatch).

Everything the windowed console does works here, because both frontends drive the same
`Shell` and present the same 128x128 framebuffer; only the presenter and the input
source differ. The shell lives in the `pixel8-console` *library*, which `pixel8-tui`
depends on with the `window` feature off — so the terminal binary never builds the
winit/wgpu stack, and the windowed binary never builds viuer/crossterm. Quit with
`Ctrl+Q` (the terminal stand-in for the window's close button) or the console's
`shutdown` command.

## How frames are drawn

Each frame the indexed framebuffer is expanded to RGBA, scaled up with
nearest-neighbor (pixels stay square and crisp), and printed with
[`viuer`](https://crates.io/crates/viuer):

- **Sixel**, where the terminal advertises it (DA1 attribute 4: foot, xterm with
  `-ti vt340`, WezTerm, Konsole, iTerm2, mlterm, ...). Real pixels, encoded by the
  pure-Rust `icy_sixel` backend — Pixel8 deliberately avoids viuer's `sixel` feature,
  which binds the C libsixel.
- **Unicode half-blocks** (`▀`/`▄`) everywhere else: two screen pixels per cell,
  truecolor where `COLORTERM` says so.

**How big the screen comes out.** A half-block cell holds one screen pixel across and
two down, so at 1:1 the console needs 128 columns and 64 rows — and one screen pixel
ends up exactly as wide as one character cell. That is the floor: the terminal's font
size is what decides how big a Pixel8 pixel looks, and zooming out shrinks the console
just as resizing the window does in the GUI. Scaling up beyond 1:1 is therefore opt-in
(`PIXEL8_TUI_MAX_SCALE`); filling the terminal by default would peg the console to the
window's size and make the zoom level a no-op. A terminal too small for 128x64 cells is
the one exception — there the screen is shrunk to fit, which drops pixels, so zoom out
if the pixel art looks uneven. Sixel has no such floor: it draws real pixels and
defaults to a 4x screen.

The kitty and iTerm graphics protocols are intentionally disabled: viuer transmits
kitty images through temp files and neither protocol frees earlier frames, which
leaks in the terminal at 30 fps. On those terminals the half-block fallback is used
(kitty ignores sixel by design; iTerm2 answers the sixel probe and gets real pixels).

Frames identical to the previous one (an idle prompt, a parked editor) are never
re-encoded or re-sent. If printing a frame does take longer than the tick budget
(large sixel frames on a slow terminal or connection), whole frames are skipped
proportionally — the console keeps ticking at full speed and consuming input, and
degrades to a lower visual frame rate instead of lagging ever further behind.

## Input

- **Keyboard**: raw-mode key events. Shell shortcuts (`Ctrl+R`, `Ctrl+S`, `Esc`,
  `Alt+←/→`, `F1`, `F6`, ...) work as in the windowed console.
- **Game buttons** (arrows + `Z`/`X`, `C`/`V`, `N`/`M`): terminals classically report
  only key *presses*, never releases, so buttons take the best source available.
  The status line always shows which one is active:

  1. **`kitty keys`** — the terminal speaks the [kitty keyboard
     protocol](https://sw.kovidgoyal.net/kitty/keyboard-protocol/) (kitty, foot,
     alacritty, WezTerm, ghostty...): real press/release events, buttons behave
     exactly like in a window, chords and diagonals included.
  2. **`raw keys`** — key press/release state read straight from the kernel's input
     devices (`/dev/input`), bypassing the terminal. Works under Wayland, X11 and
     the bare console; chords and diagonals work. Only applied while the terminal
     reports focus, so typing in other windows never drives a cart, and only the
     ten mapped keys are tracked. Needs read access to `/dev/input` — on most
     distros that is one-time setup: `sudo usermod -aG input $USER`, then
     `newgrp input` in the current shell (new logins pick it up automatically).
     Set `PIXEL8_TUI_NO_RAW_KEYS=1` to opt out. Linux only.
  3. **`latched keys`** — the degraded last resort: releases are *inferred* from
     the OS autorepeat stream. A fresh press holds the button ~0.7 s (bridging the
     autorepeat delay, so holds read as continuous), once autorepeats stream in
     the hold shrinks to ~2.5x the observed repeat interval (letting go registers
     in ~0.1–0.3 s), and pressing a direction releases its opposite instantly.
     **The caveats**: a quick tap reads as a ~0.7 s hold (edge-triggered `btnp`
     input is unaffected), and chords (hold right + jump, diagonals) fade once the
     OS stops repeating the older key — one key's autorepeat is all the
     information such a terminal gives. Because of that, on Linux this tier is
     never entered silently: without kitty-protocol support or `/dev/input`
     access, `pixel8-tui` exits with instructions instead, and the latch must be
     opted into explicitly with `PIXEL8_TUI_NO_RAW_KEYS=1` (e.g. over SSH). On
     platforms with no raw keyboard source (macOS, Windows) it is the normal
     fallback for non-kitty terminals.
- **Mouse**: click, drag and hover all reach the editors, at terminal-cell
  granularity — coarser than a real pointer (roughly every other screen pixel at
  4x scale) but fully usable. When the terminal reports its pixel size, cell→pixel
  mapping is exact; otherwise a common 8x16 cell is assumed. The console's own
  pixel-art cursor is suppressed here: the terminal already draws a mouse pointer
  and — unlike a window — gives no way to hide it, so drawing both would show a
  distracting double cursor. (The windowed console does the reverse, hiding the OS
  pointer and keeping its own.)
- **Paste**: bracketed paste is fed through the shell as keystrokes, so pasting works
  in the console prompt and the code editor. (`Ctrl+C`/`Ctrl+V` still use the system
  clipboard, exactly like the windowed console, when a desktop clipboard is
  reachable.)

## Knobs

| variable                 | default | meaning                                             |
| ------------------------ | ------- | --------------------------------------------------- |
| `PIXEL8_TUI_MAX_SCALE`   | `4` sixel, `1` blocks | Largest integer scale of the 128 px screen (1–7). A sixel step is one device pixel; a half-block step is a whole character cell, hence the different defaults. Bigger costs more CPU per frame. |
| `PIXEL8_TUI_NO_RAW_KEYS` | unset   | Set to `1` to never read `/dev/input` and accept degraded, autorepeat-inferred key releases (otherwise a Linux requirement — see above). |

## Features

Like the console, `pixel8-tui` has a default-on `audio` cargo feature; build with
`--no-default-features` on machines without ALSA headers and the console runs
silently. There is no window/GPU code to disable — this binary never had any.

When building from the workspace, select the package (`cargo run -p pixel8-tui`, or
the `cargo tui` alias): a bare `cargo run --bin pixel8-tui` at the workspace root
makes cargo unify features across all members, which pointlessly builds the
windowed console's wgpu/winit stack too.
