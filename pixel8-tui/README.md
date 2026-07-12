# pixel8-tui

The [Pixel8](https://github.com/zeenix/pixel8) fantasy console in your terminal — the
same shell, editors and carts the windowed `pixel8` binary drives, rendered over
[sixel](https://en.wikipedia.org/wiki/Sixel) where the terminal supports it (via a
pure-Rust encoder) and unicode half-blocks everywhere else.

```text
cargo install pixel8-tui
pixel8-tui                     boot the console
pixel8-tui mygame              boot with a project or cart loaded
pixel8-tui run mygame.png      boot, load, and run immediately
```

`Ctrl+Q` quits. Game buttons need real key press/release events, so on Linux one of
these is required: a terminal speaking the kitty keyboard protocol (kitty, foot,
alacritty, WezTerm, ghostty...), or read access to the kernel's input devices —
one-time setup:

```text
sudo usermod -aG input $USER
newgrp input      # applies it to the current shell; new logins have it automatically
```

With neither, `pixel8-tui` exits with these instructions (set
`PIXEL8_TUI_NO_RAW_KEYS=1` to accept degraded, autorepeat-inferred input instead,
e.g. over SSH). See
[docs/TUI.md](https://github.com/zeenix/pixel8/blob/main/docs/TUI.md) for the full
list of caveats and knobs.

This crate is a thin frontend: the console itself lives in the
[`pixel8-console`](https://crates.io/crates/pixel8-console) library, which this binary
depends on with the window stack (winit/wgpu) switched off.
