//! Terminal frontend: the whole console — shell, editors, running carts —
//! rendered inside a terminal instead of a window.
//!
//! Every presented frame the 128x128 indexed framebuffer is expanded to
//! RGBA, scaled with nearest-neighbor to the exact pixel size the printer
//! will emit, and handed to `viuer`. Where the terminal advertises sixel
//! support (DA1 attribute 4) viuer's pure-Rust `icy_sixel` encoder draws
//! real pixels; everywhere else it falls back to unicode half-blocks
//! (▀ / ▄), two screen pixels per cell. The kitty and iTerm protocols are
//! deliberately disabled: viuer transmits kitty images through temp files
//! and neither protocol frees previous frames, which leaks at 30 fps.
//!
//! Input comes from crossterm in raw mode. Terminals traditionally report
//! only key *presses* — never releases — so game buttons take the best
//! source available (shown in the status line):
//!
//! 1. **kitty keys** — the terminal speaks the kitty keyboard protocol and reports real
//!    press/release events; buttons behave exactly as in a window, chords included.
//! 2. **raw keys** — key state read from the kernel's input devices (`/dev/input`, Linux, needs the
//!    `input` group), gated on terminal focus; chords included. See `raw_keys.rs`.
//! 3. **latched keys** — releases inferred from the OS autorepeat stream: a fresh press holds a
//!    button long enough to bridge the autorepeat delay, once repeats stream in the hold shrinks to
//!    just past the observed repeat interval, and pressing a direction releases its opposite
//!    instantly (only the newest key autorepeats, so the old one could never re-arm anyway). Holds
//!    read as continuous, releases register within a couple of repeat intervals; but quick taps
//!    read as ~0.7 s holds and chords fade once the OS stops repeating the older key — one key's
//!    autorepeat is all the information such a terminal gives. Because that is a degraded
//!    experience, on Linux this tier must be opted into with PIXEL8_TUI_NO_RAW_KEYS=1; without the
//!    opt-in, a press-only terminal with no `/dev/input` access is a startup error that explains
//!    the fixes. On platforms with no raw keyboard source it is the normal fallback.
//!
//! Mouse events arrive per cell, so pointing in editors is coarser than
//! in the windowed console but works the same way.

use crate::raw_keys::RawKeys;
use anyhow::{bail, Context, Result};
use crossterm::{
    cursor,
    event::{
        self, DisableBracketedPaste, DisableFocusChange, DisableMouseCapture, EnableBracketedPaste,
        EnableFocusChange, EnableMouseCapture, Event, KeyCode, KeyEvent, KeyEventKind,
        KeyModifiers, KeyboardEnhancementFlags, MouseButton, MouseEvent, MouseEventKind,
        PopKeyboardEnhancementFlags, PushKeyboardEnhancementFlags,
    },
    execute, queue,
    style::{Print, Stylize},
    terminal::{self, Clear, ClearType, EnterAlternateScreen, LeaveAlternateScreen, SetTitle},
};
use image::{imageops, DynamicImage, RgbaImage};
use pixel8_console::{
    frame_duration, sdk_path,
    shell::{Key, Mods, Shell},
};
use pixel8_runtime::fb::{Framebuffer, HEIGHT, WIDTH};
use std::{
    io::{stdout, IsTerminal, Write},
    panic,
    sync::atomic::{AtomicBool, Ordering},
    time::{Duration, Instant},
};

/// Boot the console in the terminal, optionally with a cart loaded (and
/// auto-run). Returns when the user quits (Ctrl+Q or the `shutdown`
/// command).
pub fn run(load: Option<String>, auto_run: bool) -> Result<()> {
    if !std::io::stdin().is_terminal() || !stdout().is_terminal() {
        bail!("pixel8-tui needs an interactive terminal on stdin and stdout");
    }

    #[cfg(feature = "audio")]
    let _audio_out = pixel8_runtime::audio::AudioOutput::start();
    #[cfg(feature = "audio")]
    let audio = _audio_out
        .as_ref()
        .map(|a| a.handle())
        .unwrap_or_else(pixel8_runtime::audio::AudioHandle::dummy);
    #[cfg(not(feature = "audio"))]
    let audio = pixel8_runtime::audio::AudioHandle::dummy();

    let mut shell = Shell::new(audio, sdk_path());
    if let Some(path) = load {
        shell.startup_load(&path);
        if auto_run {
            shell.startup_run();
        }
    }

    let guard = TerminalGuard::enter().context("Setting up the terminal")?;
    // Probe after raw mode is on (the DA1 query needs an immediate, unechoed
    // response) and before the event loop starts consuming stdin.
    let backend = if viuer::is_sixel_supported() {
        Backend::Sixel
    } else {
        Backend::Blocks
    };
    // Erroring here (no usable button source) propagates past `guard`, so
    // the terminal is restored before the message prints.
    let buttons = button_tracker(
        guard.enhanced_keys,
        raw_keys_disabled(std::env::var("PIXEL8_TUI_NO_RAW_KEYS").ok().as_deref()),
    )?;
    let mut tui = Tui::new(shell, backend, buttons);
    let result = tui.run_loop();
    drop(guard);
    result
}

/// Pick the best game-button source. On Linux a press-only terminal
/// without `/dev/input` access is an error, not a fallback: the latch
/// approximation (held keys stutter, chords don't work) is only used when
/// explicitly requested (PIXEL8_TUI_NO_RAW_KEYS) — or on platforms with no
/// raw keyboard source at all.
fn button_tracker(enhanced_keys: bool, raw_disabled: bool) -> Result<ButtonTracker> {
    if enhanced_keys {
        return Ok(ButtonTracker::Direct);
    }
    if raw_disabled {
        return Ok(ButtonTracker::Latched(KeyLatch::new()));
    }
    if let Some(raw) = RawKeys::start() {
        return Ok(ButtonTracker::Raw(raw));
    }
    #[cfg(target_os = "linux")]
    bail!(
        "This terminal doesn't report key releases (no kitty keyboard protocol), and \
         /dev/input isn't readable, so game buttons would misbehave (held keys stutter, \
         chords don't work). Fix one of:\n\
         \x20 - grant yourself raw key access: sudo usermod -aG input $USER, \
         then `newgrp input` in this shell (new logins pick it up automatically)\n\
         \x20 - use a terminal with the kitty keyboard protocol (kitty, foot, alacritty, \
         WezTerm, ghostty, ...)\n\
         \x20 - set PIXEL8_TUI_NO_RAW_KEYS=1 to accept the degraded input anyway"
    );
    #[cfg(not(target_os = "linux"))]
    Ok(ButtonTracker::Latched(KeyLatch::new()))
}

/// The live session: shell state plus everything needed to present frames
/// and translate terminal events.
struct Tui {
    shell: Shell,
    backend: Backend,
    buttons: ButtonTracker,
    skipper: FrameSkipper,
    frame_cache: FrameCache,
    layout: Layout,
    max_scale: u32,
    next_tick: Instant,
    quit: bool,
    /// Whether the terminal reports having focus; raw key state is only
    /// applied while it does, so typing elsewhere can't drive a cart.
    focused: bool,
    last_title: String,
    last_status: String,
}

impl Tui {
    fn new(shell: Shell, backend: Backend, buttons: ButtonTracker) -> Self {
        let max_scale = parse_max_scale(std::env::var("PIXEL8_TUI_MAX_SCALE").ok().as_deref());
        Self {
            shell,
            backend,
            buttons,
            skipper: FrameSkipper::new(),
            frame_cache: FrameCache::new(),
            // Placeholder until run_loop's initial relayout measures the
            // real terminal.
            layout: Layout::compute(backend, 80, 24, None, max_scale),
            max_scale,
            next_tick: Instant::now(),
            quit: false,
            focused: true,
            last_title: String::new(),
            last_status: String::new(),
        }
    }

    /// The tick/present loop, paced exactly like the windowed console:
    /// catch up on missed ticks, then present at most one frame.
    fn run_loop(&mut self) -> Result<()> {
        self.relayout()?;
        self.next_tick = Instant::now();
        loop {
            self.pump_events()?;
            let frame = frame_duration(self.shell.tick_fps());
            let now = Instant::now();
            let mut ticked = false;
            while Instant::now() >= self.next_tick {
                match &mut self.buttons {
                    ButtonTracker::Direct => {}
                    // Fresh physical key state straight from the kernel.
                    ButtonTracker::Raw(raw) => {
                        let state = raw.poll();
                        for (b, down) in state.iter().enumerate() {
                            self.shell.set_button(b, *down && self.focused);
                        }
                    }
                    // Latched buttons whose hold ran out with no autorepeat
                    // in sight were released for real; let the tick see it.
                    ButtonTracker::Latched(latch) => {
                        for b in latch.take_expired(Instant::now()) {
                            self.shell.set_button(b, false);
                        }
                    }
                }
                self.shell.tick();
                self.next_tick += frame;
                ticked = true;
                // Don't death-spiral after a long stall.
                if now > self.next_tick + frame * 10 {
                    self.next_tick = now + frame;
                }
            }
            if self.quit || self.shell.want_exit {
                return Ok(());
            }
            if ticked && self.skipper.should_present() {
                let start = Instant::now();
                self.present()?;
                self.skipper.presented(start.elapsed(), frame);
            }
        }
    }

    /// Feed terminal events to the shell, blocking at most until the next
    /// tick is due. The queue is always drained completely: handling an
    /// event costs microseconds while printing a frame costs milliseconds,
    /// so returning mid-queue on the tick deadline would let input back up
    /// whenever printing runs behind — held-key autorepeat alone outpaces a
    /// one-event-per-tick drain, burying later key presses seconds deep in
    /// stale repeats.
    fn pump_events(&mut self) -> Result<()> {
        loop {
            // Once the tick is due this is a zero-timeout poll, i.e. it
            // only drains what is already buffered and then returns.
            let timeout = self.next_tick.saturating_duration_since(Instant::now());
            if !event::poll(timeout)? {
                return Ok(());
            }
            match event::read()? {
                Event::Key(k) => self.on_key(k),
                Event::Mouse(m) => self.on_mouse(m),
                Event::Paste(text) => self.on_paste(&text),
                Event::Resize(..) => self.relayout()?,
                Event::FocusGained => self.focused = true,
                Event::FocusLost => {
                    self.focused = false;
                    self.release_all_buttons();
                }
            }
        }
    }

    fn on_key(&mut self, ev: KeyEvent) {
        let mut mods = key_mods(ev.modifiers);
        // BackTab arrives as its own key; the shell knows it as shift+Tab.
        if ev.code == KeyCode::BackTab {
            mods.shift = true;
        }
        // A quit chord the shell doesn't use (Ctrl+C is "copy" in editors);
        // the windowed console relies on the window's close button instead.
        if ev.kind == KeyEventKind::Press
            && mods.ctrl
            && matches!(ev.code, KeyCode::Char('q' | 'Q'))
        {
            self.quit = true;
            return;
        }
        if let Some(b) = game_button(ev.code) {
            match (&mut self.buttons, ev.kind) {
                (ButtonTracker::Direct, KeyEventKind::Press) => self.shell.set_button(b, true),
                (ButtonTracker::Direct, KeyEventKind::Release) => self.shell.set_button(b, false),
                (ButtonTracker::Direct, KeyEventKind::Repeat) => {}
                // The kernel is the authority; terminal presses are noise.
                (ButtonTracker::Raw(_), _) => {}
                (ButtonTracker::Latched(latch), KeyEventKind::Press | KeyEventKind::Repeat) => {
                    if let Some(opposite) = latch.press(b, Instant::now()) {
                        self.shell.set_button(opposite, false);
                    }
                    self.shell.set_button(b, true);
                }
                // Never sent without the kitty keyboard protocol.
                (ButtonTracker::Latched(_), KeyEventKind::Release) => {}
            }
        }
        if matches!(ev.kind, KeyEventKind::Press | KeyEventKind::Repeat) {
            if let Some(key) = shell_key(ev.code) {
                self.shell.key(key, mods);
            }
        }
    }

    fn on_mouse(&mut self, ev: MouseEvent) {
        let (x, y) = self.layout.cell_to_screen(ev.column, ev.row);
        self.shell.mouse.x = x;
        self.shell.mouse.y = y;
        match ev.kind {
            MouseEventKind::Down(MouseButton::Left) => {
                self.shell.mouse.left = true;
                self.shell.mouse.left_pressed = true;
            }
            MouseEventKind::Up(MouseButton::Left) => self.shell.mouse.left = false,
            MouseEventKind::Down(MouseButton::Right) => {
                self.shell.mouse.right = true;
                self.shell.mouse.right_pressed = true;
            }
            MouseEventKind::Up(MouseButton::Right) => self.shell.mouse.right = false,
            _ => {}
        }
    }

    /// Bracketed paste: feed the text through the shell as ordinary key
    /// strokes, so pasting works in the console prompt and every editor.
    fn on_paste(&mut self, text: &str) {
        for c in text.chars() {
            let key = match c {
                '\n' | '\r' => Key::Enter,
                '\t' => Key::Tab,
                c if c.is_control() => continue,
                c => Key::Char(c),
            };
            self.shell.key(key, Mods::default());
        }
    }

    fn release_all_buttons(&mut self) {
        if let ButtonTracker::Latched(latch) = &mut self.buttons {
            latch.clear();
        }
        for b in 0..BUTTONS {
            self.shell.set_button(b, false);
        }
    }

    /// Measure the terminal and recompute where and how big the screen is
    /// drawn. Called at startup and on every resize event.
    fn relayout(&mut self) -> Result<()> {
        let (cols, rows) = terminal::size()?;
        // Pixel sizes matter only for sixel; not every terminal reports them.
        let win_px = terminal::window_size()
            .ok()
            .filter(|w| w.width > 0 && w.height > 0)
            .map(|w| (w.width, w.height));
        self.layout = Layout::compute(self.backend, cols, rows, win_px, self.max_scale);
        execute!(stdout(), Clear(ClearType::All))?;
        // Force the image and the status line to be drawn again.
        self.frame_cache.invalidate();
        self.last_status.clear();
        Ok(())
    }

    fn present(&mut self) -> Result<()> {
        let fb = self.shell.draw();
        // Reprinting an identical frame would cost a full encode and ship
        // (milliseconds) for no visible change; comparing 16 KiB is free by
        // comparison. Idle screens — the prompt, a parked editor — are
        // identical most ticks.
        if self.frame_cache.changed(fb) {
            let img = frame_image(fb, self.layout.px);
            viuer::print(&img, &self.viuer_config()).context("Printing a frame")?;
        }
        self.draw_chrome()
    }

    fn viuer_config(&self) -> viuer::Config {
        viuer::Config {
            width: Some(self.layout.cells.0),
            height: Some(self.layout.cells.1),
            // Kitty/iTerm would go through temp files and leak frames in
            // the terminal; see the module docs. Sixel or blocks only.
            use_kitty: false,
            use_iterm: false,
            use_sixel: self.backend == Backend::Sixel,
            ..Default::default()
        }
    }

    /// The terminal title plus a dim status line under the image.
    fn draw_chrome(&mut self) -> Result<()> {
        let title = self.shell.window_title();
        let mut out = stdout();
        if title != self.last_title {
            queue!(out, SetTitle(title.as_str()))?;
            self.last_title = title.clone();
        }
        if let Some(row) = self.layout.status_row {
            let keys = match self.buttons {
                ButtonTracker::Direct => "kitty keys",
                ButtonTracker::Raw(_) => "raw keys",
                ButtonTracker::Latched(_) => "latched keys",
            };
            let status = format!("{title} · {} · {keys} · Ctrl+Q quit", self.backend.label());
            if status != self.last_status {
                let line: String = status.chars().take(self.layout.cols as usize).collect();
                queue!(
                    out,
                    cursor::MoveTo(0, row),
                    Clear(ClearType::UntilNewLine),
                    Print(line.dim())
                )?;
                self.last_status = status;
            }
        }
        out.flush()?;
        Ok(())
    }
}

/// Puts the terminal into raw mode + alternate screen on creation and
/// restores it on drop; a panic hook covers the unclean path so panic
/// messages print onto a usable screen.
struct TerminalGuard {
    /// The terminal speaks the kitty keyboard protocol, i.e. reports key
    /// releases, so game buttons don't need latching.
    enhanced_keys: bool,
}

impl TerminalGuard {
    fn enter() -> Result<Self> {
        terminal::enable_raw_mode()?;
        RAW_MODE_ACTIVE.store(true, Ordering::SeqCst);
        install_panic_restore();
        execute!(
            stdout(),
            EnterAlternateScreen,
            cursor::Hide,
            EnableMouseCapture,
            EnableBracketedPaste,
            EnableFocusChange
        )?;
        let enhanced_keys = terminal::supports_keyboard_enhancement().unwrap_or(false);
        if enhanced_keys {
            execute!(
                stdout(),
                PushKeyboardEnhancementFlags(KeyboardEnhancementFlags::REPORT_EVENT_TYPES)
            )?;
        }
        Ok(Self { enhanced_keys })
    }
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        restore_terminal(self.enhanced_keys);
    }
}

/// Best-effort restore; runs on both clean exit and panic, so it must never
/// itself panic and must be safe to call twice.
fn restore_terminal(enhanced_keys: bool) {
    if !RAW_MODE_ACTIVE.swap(false, Ordering::SeqCst) {
        return;
    }
    let mut out = stdout();
    if enhanced_keys {
        let _ = execute!(out, PopKeyboardEnhancementFlags);
    }
    let _ = execute!(
        out,
        DisableFocusChange,
        DisableBracketedPaste,
        DisableMouseCapture,
        cursor::Show,
        LeaveAlternateScreen
    );
    let _ = terminal::disable_raw_mode();
}

/// On panic, put the terminal back into a usable state *before* the default
/// hook prints the message into the alternate screen.
fn install_panic_restore() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        let default = panic::take_hook();
        panic::set_hook(Box::new(move |info| {
            // Popping never-pushed enhancement flags is harmless.
            restore_terminal(true);
            default(info);
        }));
    });
}

static RAW_MODE_ACTIVE: AtomicBool = AtomicBool::new(false);

/// Which of viuer's printers presents frames.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Backend {
    /// Real pixels through the pure-Rust `icy_sixel` encoder.
    Sixel,
    /// Unicode half-blocks: each cell shows two vertically stacked pixels.
    Blocks,
}

impl Backend {
    fn label(self) -> &'static str {
        match self {
            Backend::Sixel => "sixel",
            Backend::Blocks => "blocks",
        }
    }
}

/// How the 128x128 screen maps onto the terminal grid.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Layout {
    /// Terminal width in cells, for truncating the status line.
    cols: u16,
    /// Print size handed to viuer, in terminal cells.
    cells: (u32, u32),
    /// Exact pixel size the printer will emit — the pre-scale target.
    px: (u32, u32),
    /// Size of one terminal cell in the printed image's pixel space, for
    /// mapping mouse cells back onto the virtual screen.
    cell_px: (f32, f32),
    /// Row right below the image for the status line, if there is room.
    status_row: Option<u16>,
}

impl Layout {
    fn compute(
        backend: Backend,
        cols: u16,
        rows: u16,
        win_px: Option<(u16, u16)>,
        max_scale: u32,
    ) -> Layout {
        match backend {
            Backend::Sixel => Self::sixel(cols, rows, win_px, max_scale),
            Backend::Blocks => Self::blocks(cols, rows, max_scale),
        }
    }

    /// Sixel draws real pixels; viuer's sixel path emits 6x12 px per
    /// requested cell. Aim for the largest square that fits the terminal's
    /// pixel area, snapped down to a multiple of 12 so both dimensions land
    /// on whole cell counts.
    fn sixel(cols: u16, rows: u16, win_px: Option<(u16, u16)>, max_scale: u32) -> Layout {
        let cell = match win_px {
            Some((w, h)) => (w as f32 / cols.max(1) as f32, h as f32 / rows.max(1) as f32),
            // The terminal didn't report pixel sizes; assume a common cell.
            None => FALLBACK_CELL_PX,
        };
        // Keep one row for the status line.
        let avail_w = cols as f32 * cell.0;
        let avail_h = rows.saturating_sub(1) as f32 * cell.1;
        let raw = (avail_w.min(avail_h).max(0.0) as u32)
            .min(WIDTH as u32 * max_scale)
            .min(MAX_SIXEL_PX);
        let side = (raw - raw % 12).max(12);
        let rows_used = (side as f32 / cell.1).ceil() as u16;
        Layout {
            cols,
            cells: (side / 6, side / 12),
            px: (side, side),
            cell_px: cell,
            status_row: (rows_used < rows).then_some(rows_used),
        }
    }

    /// Half-block cells are one pixel wide and two tall. The bottom row is
    /// kept free: viuer ends the image with a newline, which would scroll
    /// the alternate screen if the image touched the last row — and the
    /// status line lives there.
    fn blocks(cols: u16, rows: u16, max_scale: u32) -> Layout {
        let rows_img = rows.saturating_sub(1).max(1) as u32;
        let mut side = (2 * rows_img.min(cols as u32 / 2)).max(2);
        if side >= WIDTH as u32 {
            // Snap to whole multiples of the screen so pixels stay uniform.
            side = (side - side % WIDTH as u32).min(WIDTH as u32 * max_scale);
        }
        Layout {
            cols,
            cells: (side, side / 2),
            px: (side, side),
            cell_px: (1.0, 2.0),
            status_row: (u32::from(rows) > side / 2).then_some((side / 2) as u16),
        }
    }

    /// Map a mouse cell to virtual screen coordinates, the terminal analog
    /// of `gpu::Viewport::window_to_screen`. Cell centers keep the rounding
    /// symmetric; results can be off-screen, which the shell tolerates.
    fn cell_to_screen(&self, col: u16, row: u16) -> (i32, i32) {
        let x = (col as f32 + 0.5) * self.cell_px.0 * WIDTH as f32 / self.px.0 as f32;
        let y = (row as f32 + 0.5) * self.cell_px.1 * HEIGHT as f32 / self.px.1 as f32;
        (x.floor() as i32, y.floor() as i32)
    }
}

/// Map keys to the six game buttons, mirroring the windowed console.
fn game_button(code: KeyCode) -> Option<usize> {
    Some(match code {
        KeyCode::Left => 0,
        KeyCode::Right => 1,
        KeyCode::Up => 2,
        KeyCode::Down => 3,
        KeyCode::Char(c) => match c.to_ascii_lowercase() {
            'z' | 'c' | 'n' => 4,
            'x' | 'v' | 'm' => 5,
            _ => return None,
        },
        _ => return None,
    })
}

fn shell_key(code: KeyCode) -> Option<Key> {
    Some(match code {
        KeyCode::Char(c) => Key::Char(c),
        KeyCode::Left => Key::Left,
        KeyCode::Right => Key::Right,
        KeyCode::Up => Key::Up,
        KeyCode::Down => Key::Down,
        KeyCode::Backspace => Key::Backspace,
        KeyCode::Delete => Key::Delete,
        KeyCode::Enter => Key::Enter,
        KeyCode::Tab | KeyCode::BackTab => Key::Tab,
        KeyCode::Esc => Key::Escape,
        KeyCode::Home => Key::Home,
        KeyCode::End => Key::End,
        KeyCode::PageUp => Key::PageUp,
        KeyCode::PageDown => Key::PageDown,
        KeyCode::F(1) => Key::ToggleStats,
        KeyCode::F(6) => Key::CaptureLabel,
        _ => return None,
    })
}

fn key_mods(m: KeyModifiers) -> Mods {
    Mods {
        ctrl: m.contains(KeyModifiers::CONTROL),
        shift: m.contains(KeyModifiers::SHIFT),
        alt: m.contains(KeyModifiers::ALT),
    }
}

/// How game-button state is tracked, depending on what the terminal (or
/// the OS underneath it) can say.
enum ButtonTracker {
    /// The terminal reports key releases (kitty keyboard protocol).
    Direct,
    /// Key state read from the kernel's input devices (`/dev/input`),
    /// bypassing the terminal; applied only while the terminal has focus.
    Raw(RawKeys),
    /// Presses only: releases are inferred from the autorepeat stream.
    Latched(KeyLatch),
}

/// Release-less button state, adapted to the OS keyboard autorepeat (the
/// only signal a press-only terminal gives about a key still being down).
///
/// A fresh press arms a hold long enough to bridge the autorepeat delay,
/// so a held key reads as one continuous press from the first event. Once
/// autorepeats arrive (short gaps between presses), the hold shrinks to
/// just past the observed repeat interval, so a real release is noticed
/// within a couple of intervals instead of the full initial hold. And
/// since the OS only autorepeats the most recently pressed key, a held
/// opposite direction can never re-arm — pressing right releases a
/// latched left instantly, making direction switches immediate.
struct KeyLatch {
    deadline: [Option<Instant>; BUTTONS],
    last_press: [Option<Instant>; BUTTONS],
}

impl KeyLatch {
    fn new() -> Self {
        Self {
            deadline: [None; BUTTONS],
            last_press: [None; BUTTONS],
        }
    }

    /// Arm (or re-arm) a button; returns the opposite direction to release
    /// now, if that one was latched.
    fn press(&mut self, button: usize, now: Instant) -> Option<usize> {
        let hold = match self.last_press[button].map(|p| now.duration_since(p)) {
            // Gaps this short only occur as OS autorepeat — the key is
            // pinned down, and the repeat stream will keep re-arming us.
            Some(gap) if gap <= REPEAT_GAP_MAX => {
                (gap * 5 / 2).clamp(REPEAT_HOLD_MIN, REPEAT_HOLD_MAX)
            }
            // A fresh press, or the first repeat after the OS delay.
            _ => INITIAL_HOLD,
        };
        self.last_press[button] = Some(now);
        self.deadline[button] = Some(now + hold);
        let opposite = opposite(button)?;
        self.deadline[opposite].take().map(|_| opposite)
    }

    /// Buttons whose hold has expired; each is reported exactly once.
    fn take_expired(&mut self, now: Instant) -> Vec<usize> {
        let mut expired = Vec::new();
        for (button, slot) in self.deadline.iter_mut().enumerate() {
            if slot.is_some_and(|deadline| deadline <= now) {
                *slot = None;
                expired.push(button);
            }
        }
        expired
    }

    fn clear(&mut self) {
        self.deadline = [None; BUTTONS];
        self.last_press = [None; BUTTONS];
    }
}

/// The opposing d-pad direction, if any (left/right, up/down).
fn opposite(button: usize) -> Option<usize> {
    match button {
        0 => Some(1),
        1 => Some(0),
        2 => Some(3),
        3 => Some(2),
        _ => None,
    }
}

/// Drop frames when printing can't keep up: after a slow print, skip as
/// many ticks as the overrun covers (capped), so a slow terminal degrades
/// to a lower frame rate instead of falling ever further behind.
struct FrameSkipper {
    pending_skips: u32,
}

impl FrameSkipper {
    fn new() -> Self {
        Self { pending_skips: 0 }
    }

    fn should_present(&mut self) -> bool {
        if self.pending_skips > 0 {
            self.pending_skips -= 1;
            false
        } else {
            true
        }
    }

    fn presented(&mut self, cost: Duration, budget: Duration) {
        let budget = budget.max(Duration::from_micros(1));
        self.pending_skips = ((cost.as_micros() / budget.as_micros()) as u32).min(MAX_FRAME_SKIP);
    }
}

/// The last frame that was actually printed — pixels plus display palette,
/// the two inputs that decide what a frame looks like — so identical frames
/// are never encoded and shipped twice.
struct FrameCache {
    pixels: Vec<u8>,
    palette: [u8; 16],
}

impl FrameCache {
    fn new() -> Self {
        Self {
            pixels: Vec::new(),
            palette: [0; 16],
        }
    }

    /// Record the frame and report whether it differs from the previous
    /// one. The first frame after `new` or `invalidate` always differs.
    fn changed(&mut self, fb: &Framebuffer) -> bool {
        let same = self.pixels.as_slice() == fb.pixels() && &self.palette == fb.display_palette();
        if !same {
            self.pixels.clear();
            self.pixels.extend_from_slice(fb.pixels());
            self.palette = *fb.display_palette();
        }
        !same
    }

    /// Forget the recorded frame, forcing the next one to print (after a
    /// resize the screen was cleared, whatever the pixels say).
    fn invalidate(&mut self) {
        self.pixels.clear();
    }
}

/// Expand the indexed framebuffer to RGBA and scale it (nearest-neighbor:
/// pixels stay crisp) to the exact size the printer will emit, so viuer's
/// own smoothing resize becomes an identity.
fn frame_image(fb: &Framebuffer, px: (u32, u32)) -> DynamicImage {
    let (w, h) = (WIDTH as u32, HEIGHT as u32);
    let mut rgba = vec![0u8; (w * h * 4) as usize];
    fb.write_rgba(&mut rgba);
    let img = RgbaImage::from_raw(w, h, rgba).expect("buffer sized to the framebuffer");
    if px == (w, h) {
        DynamicImage::ImageRgba8(img)
    } else {
        DynamicImage::ImageRgba8(imageops::resize(
            &img,
            px.0,
            px.1,
            imageops::FilterType::Nearest,
        ))
    }
}

/// PIXEL8_TUI_NO_RAW_KEYS: opt out of reading `/dev/input` (any value but
/// empty or "0"), forcing autorepeat-inferred releases instead.
fn raw_keys_disabled(var: Option<&str>) -> bool {
    var.is_some_and(|v| !v.is_empty() && v != "0")
}

/// PIXEL8_TUI_MAX_SCALE: largest integer scale of the 128 px screen in
/// sixel mode. Bigger frames cost more to encode 30 times a second.
fn parse_max_scale(var: Option<&str>) -> u32 {
    var.and_then(|v| v.trim().parse::<u32>().ok())
        .map(|s| s.clamp(1, 7))
        .unwrap_or(DEFAULT_MAX_SCALE)
}

/// The number of game buttons (left, right, up, down, A, B).
pub(crate) const BUTTONS: usize = 6;
/// Hold on a fresh press: just past common OS autorepeat delays (GNOME
/// 500 ms, X11 660 ms), so a held key never gaps before repeats begin.
const INITIAL_HOLD: Duration = Duration::from_millis(700);
/// Press gaps up to this are autorepeat; even slow settings repeat faster
/// than ~7 keys per second, and humans can't tap much faster.
const REPEAT_GAP_MAX: Duration = Duration::from_millis(150);
/// Bounds for the shrunken hold while autorepeat streams in (2.5x the
/// observed repeat gap, so one dropped repeat doesn't blip the button).
const REPEAT_HOLD_MIN: Duration = Duration::from_millis(90);
const REPEAT_HOLD_MAX: Duration = Duration::from_millis(300);
/// viuer's sixel printer clamps widths to 1000 px (an xterm workaround);
/// staying below keeps our pre-scale exact. Must be a multiple of 12.
const MAX_SIXEL_PX: u32 = 996;
/// Assumed cell pixel size when the terminal doesn't report one.
const FALLBACK_CELL_PX: (f32, f32) = (8.0, 16.0);
const DEFAULT_MAX_SCALE: u32 = 4;
/// Never skip more than this many frames in a row (~3 fps floor at 30).
const MAX_FRAME_SKIP: u32 = 9;

#[cfg(test)]
mod tests {
    use super::*;
    use pixel8_runtime::palette;

    // -- Layout: blocks --

    #[test]
    fn blocks_fit_a_standard_terminal() {
        let l = Layout::compute(Backend::Blocks, 80, 24, None, 4);
        // 23 rows of image (one kept for the status line), 46x46 pixels.
        assert_eq!(l.cells, (46, 23));
        assert_eq!(l.px, (46, 46));
        assert_eq!(l.status_row, Some(23));
    }

    #[test]
    fn blocks_snap_to_integer_scale_when_big() {
        let l = Layout::compute(Backend::Blocks, 300, 140, None, 4);
        // 2*139 = 278 raw, snapped down to 2x the 128px screen.
        assert_eq!(l.cells, (256, 128));
        assert_eq!(l.px, (256, 256));
        assert_eq!(l.status_row, Some(128));
    }

    #[test]
    fn blocks_respect_max_scale() {
        let l = Layout::compute(Backend::Blocks, 600, 400, None, 1);
        assert_eq!(l.px, (128, 128));
        assert_eq!(l.cells, (128, 64));
    }

    #[test]
    fn blocks_survive_a_tiny_terminal() {
        let l = Layout::compute(Backend::Blocks, 2, 1, None, 4);
        assert_eq!(l.cells, (2, 1));
        // No room for a status line on a one-row terminal.
        assert_eq!(l.status_row, None);
    }

    // -- Layout: sixel --

    #[test]
    fn sixel_uses_reported_pixel_size() {
        let l = Layout::compute(Backend::Sixel, 100, 30, Some((800, 600)), 4);
        // Cells are 8x20 px; 29 rows * 20 px = 580 px of height available,
        // capped by max_scale to 512 and snapped to 504 (multiple of 12).
        assert_eq!(l.px, (504, 504));
        assert_eq!(l.cells, (84, 42));
        // 504 px / 20 px per row = 25.2 -> the status line sits on row 26.
        assert_eq!(l.status_row, Some(26));
    }

    #[test]
    fn sixel_falls_back_to_assumed_cell_size() {
        let l = Layout::compute(Backend::Sixel, 80, 24, None, 4);
        // 23 rows * 16 px = 368 px available, snapped to 360.
        assert_eq!(l.px, (360, 360));
        assert_eq!(l.cells, (60, 30));
        assert_eq!(l.status_row, Some(23));
    }

    #[test]
    fn sixel_respects_max_scale() {
        let l = Layout::compute(Backend::Sixel, 400, 200, Some((4000, 4000)), 7);
        // 128*7 = 896, snapped to 888 (multiple of 12); well under the
        // 996 px cap that viuer's xterm workaround imposes.
        assert_eq!(l.px, (888, 888));
        assert_eq!(l.cells, (148, 74));
    }

    #[test]
    fn sixel_survives_a_tiny_terminal() {
        let l = Layout::compute(Backend::Sixel, 10, 3, None, 4);
        // 2 rows * 16 px = 32 px, snapped to 24.
        assert_eq!(l.px, (24, 24));
        assert_eq!(l.cells, (4, 2));
        assert_eq!(l.status_row, Some(2));
    }

    // -- Mouse mapping --

    #[test]
    fn blocks_mouse_maps_center_to_center() {
        let l = Layout::compute(Backend::Blocks, 80, 24, None, 4);
        // The middle of the 46x23-cell image is the middle of the screen.
        assert_eq!(l.cell_to_screen(23, 11), (65, 64));
        assert_eq!(l.cell_to_screen(0, 0), (1, 2));
    }

    #[test]
    fn blocks_mouse_is_exact_at_integer_scale() {
        let l = Layout::compute(Backend::Blocks, 300, 140, None, 4);
        // Two cells per screen pixel horizontally, one per pixel vertically.
        assert_eq!(l.cell_to_screen(0, 0), (0, 0));
        assert_eq!(l.cell_to_screen(255, 127), (127, 127));
    }

    #[test]
    fn sixel_mouse_uses_cell_pixel_size() {
        let l = Layout::compute(Backend::Sixel, 100, 30, Some((800, 600)), 4);
        // 8x20 px cells over a 504 px image: ~2 screen px per column.
        assert_eq!(l.cell_to_screen(0, 0), (1, 2));
        assert_eq!(l.cell_to_screen(62, 24), (126, 124));
    }

    // -- Key mapping --

    #[test]
    fn game_buttons_match_the_windowed_console() {
        assert_eq!(game_button(KeyCode::Left), Some(0));
        assert_eq!(game_button(KeyCode::Right), Some(1));
        assert_eq!(game_button(KeyCode::Up), Some(2));
        assert_eq!(game_button(KeyCode::Down), Some(3));
        for c in ['z', 'c', 'n', 'Z'] {
            assert_eq!(game_button(KeyCode::Char(c)), Some(4), "{c}");
        }
        for c in ['x', 'v', 'm', 'X'] {
            assert_eq!(game_button(KeyCode::Char(c)), Some(5), "{c}");
        }
        assert_eq!(game_button(KeyCode::Char('a')), None);
        assert_eq!(game_button(KeyCode::Enter), None);
    }

    #[test]
    fn shell_keys_cover_the_console_bindings() {
        assert_eq!(shell_key(KeyCode::Char('r')), Some(Key::Char('r')));
        assert_eq!(shell_key(KeyCode::Esc), Some(Key::Escape));
        assert_eq!(shell_key(KeyCode::Enter), Some(Key::Enter));
        assert_eq!(shell_key(KeyCode::BackTab), Some(Key::Tab));
        assert_eq!(shell_key(KeyCode::F(1)), Some(Key::ToggleStats));
        assert_eq!(shell_key(KeyCode::F(6)), Some(Key::CaptureLabel));
        assert_eq!(shell_key(KeyCode::F(2)), None);
        assert_eq!(shell_key(KeyCode::Insert), None);
    }

    #[test]
    fn modifiers_translate() {
        let m = key_mods(KeyModifiers::CONTROL | KeyModifiers::SHIFT);
        assert!(m.ctrl && m.shift && !m.alt);
        let m = key_mods(KeyModifiers::ALT);
        assert!(!m.ctrl && !m.shift && m.alt);
    }

    // -- Button latching --

    #[test]
    fn fresh_press_bridges_the_autorepeat_delay() {
        let mut latch = KeyLatch::new();
        let t0 = Instant::now();
        latch.press(0, t0);
        // Held across a typical 500-660 ms delay before repeats begin...
        assert!(latch.take_expired(t0 + ms(660)).is_empty());
        // ...but released once no repeat ever arrives (the key was tapped).
        assert_eq!(latch.take_expired(t0 + INITIAL_HOLD), vec![0]);
        // Each expiry is reported exactly once.
        assert!(latch.take_expired(t0 + ms(10_000)).is_empty());
    }

    #[test]
    fn autorepeat_shrinks_the_hold() {
        let mut latch = KeyLatch::new();
        let t0 = Instant::now();
        latch.press(2, t0);
        // The first repeat after the OS delay re-arms the full hold...
        latch.press(2, t0 + ms(500));
        // ...then 40 ms repeat gaps shrink it to 2.5x the gap = 100 ms.
        latch.press(2, t0 + ms(540));
        assert!(latch.take_expired(t0 + ms(639)).is_empty());
        assert_eq!(latch.take_expired(t0 + ms(640)), vec![2]);
    }

    #[test]
    fn shrunken_hold_is_bounded() {
        let mut latch = KeyLatch::new();
        let t0 = Instant::now();
        latch.press(4, t0);
        // A 10 ms gap would mean a 25 ms hold; the floor keeps one dropped
        // repeat from blipping the button.
        latch.press(4, t0 + ms(10));
        assert!(latch
            .take_expired(t0 + ms(10) + REPEAT_HOLD_MIN - ms(1))
            .is_empty());
        assert_eq!(latch.take_expired(t0 + ms(10) + REPEAT_HOLD_MIN), vec![4]);
    }

    #[test]
    fn pressing_a_direction_releases_its_opposite() {
        let mut latch = KeyLatch::new();
        let t0 = Instant::now();
        assert_eq!(latch.press(0, t0), None);
        // Right releases the latched left immediately, and only once.
        assert_eq!(latch.press(1, t0 + ms(200)), Some(0));
        assert_eq!(latch.press(1, t0 + ms(230)), None);
        // Left is no longer pending release.
        assert!(latch.take_expired(t0 + ms(10_000)) == vec![1]);
    }

    #[test]
    fn action_buttons_do_not_release_directions() {
        let mut latch = KeyLatch::new();
        let t0 = Instant::now();
        latch.press(1, t0);
        assert_eq!(latch.press(4, t0 + ms(100)), None);
        // Both stay latched until their own holds run out.
        let expired = latch.take_expired(t0 + ms(10_000));
        assert_eq!(expired, vec![1, 4]);
    }

    #[test]
    fn latch_clear_drops_everything() {
        let mut latch = KeyLatch::new();
        latch.press(1, Instant::now());
        latch.clear();
        assert!(latch.take_expired(Instant::now() + ms(10_000)).is_empty());
    }

    fn ms(v: u64) -> Duration {
        Duration::from_millis(v)
    }

    // -- Frame skipping --

    #[test]
    fn fast_prints_never_skip() {
        let mut s = FrameSkipper::new();
        assert!(s.should_present());
        s.presented(Duration::from_millis(10), Duration::from_millis(33));
        assert!(s.should_present());
    }

    #[test]
    fn slow_prints_skip_proportionally() {
        let mut s = FrameSkipper::new();
        // 2.5x over budget: skip the next two ticks, then present again.
        s.presented(Duration::from_millis(83), Duration::from_millis(33));
        assert!(!s.should_present());
        assert!(!s.should_present());
        assert!(s.should_present());
    }

    #[test]
    fn skips_are_capped() {
        let mut s = FrameSkipper::new();
        s.presented(Duration::from_secs(10), Duration::from_millis(33));
        assert_eq!(s.pending_skips, MAX_FRAME_SKIP);
    }

    // -- Frame cache --

    #[test]
    fn identical_frames_print_once() {
        let mut cache = FrameCache::new();
        let mut fb = Framebuffer::new();
        assert!(cache.changed(&fb));
        assert!(!cache.changed(&fb));
        fb.pset(7, 7, palette::col::PINK);
        assert!(cache.changed(&fb));
        assert!(!cache.changed(&fb));
    }

    #[test]
    fn display_palette_changes_are_frames_too() {
        let mut cache = FrameCache::new();
        let mut fb = Framebuffer::new();
        assert!(cache.changed(&fb));
        // Same pixels, different display mapping — must reprint.
        fb.remap_display_color(0, palette::col::RED);
        assert!(cache.changed(&fb));
    }

    #[test]
    fn invalidate_forces_a_reprint() {
        let mut cache = FrameCache::new();
        let fb = Framebuffer::new();
        assert!(cache.changed(&fb));
        cache.invalidate();
        assert!(cache.changed(&fb));
    }

    // -- Frame image --

    #[test]
    fn frame_image_is_a_palette_expansion_at_native_size() {
        let mut fb = Framebuffer::new();
        fb.pset(3, 5, palette::col::RED);
        let img = frame_image(&fb, (128, 128)).into_rgba8();
        assert_eq!(img.dimensions(), (128, 128));
        assert_eq!(img.get_pixel(3, 5).0, palette::rgba(palette::col::RED));
        assert_eq!(img.get_pixel(0, 0).0, palette::rgba(palette::col::BLACK));
    }

    #[test]
    fn frame_image_upscales_with_nearest_neighbor() {
        let mut fb = Framebuffer::new();
        fb.pset(0, 0, palette::col::GREEN);
        let img = frame_image(&fb, (256, 256)).into_rgba8();
        assert_eq!(img.dimensions(), (256, 256));
        // The single green pixel becomes an exact 2x2 block.
        for (x, y) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
            assert_eq!(img.get_pixel(x, y).0, palette::rgba(palette::col::GREEN));
        }
        assert_eq!(img.get_pixel(2, 2).0, palette::rgba(palette::col::BLACK));
    }

    // -- Environment knobs --

    #[test]
    fn kitty_terminals_use_direct_buttons() {
        assert!(matches!(
            button_tracker(true, false),
            Ok(ButtonTracker::Direct)
        ));
        // The opt-out only concerns raw keys; kitty still wins.
        assert!(matches!(
            button_tracker(true, true),
            Ok(ButtonTracker::Direct)
        ));
    }

    #[test]
    fn raw_keys_opt_out_forces_the_latch() {
        assert!(matches!(
            button_tracker(false, true),
            Ok(ButtonTracker::Latched(_))
        ));
    }

    #[test]
    fn raw_keys_opt_out_parses() {
        assert!(!raw_keys_disabled(None));
        assert!(!raw_keys_disabled(Some("")));
        assert!(!raw_keys_disabled(Some("0")));
        assert!(raw_keys_disabled(Some("1")));
        assert!(raw_keys_disabled(Some("yes")));
    }

    #[test]
    fn max_scale_parses_and_clamps() {
        assert_eq!(parse_max_scale(None), DEFAULT_MAX_SCALE);
        assert_eq!(parse_max_scale(Some("2")), 2);
        assert_eq!(parse_max_scale(Some("0")), 1);
        assert_eq!(parse_max_scale(Some("99")), 7);
        assert_eq!(parse_max_scale(Some("junk")), DEFAULT_MAX_SCALE);
    }
}
