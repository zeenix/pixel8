//! The Pixel8 console as a library: the shell (boot prompt, mode machine,
//! run loop), the five editors, build orchestration and web export —
//! everything except presentation and input, which each frontend supplies.
//!
//! Two frontends drive it: the `pixel8` binary in this crate — whose
//! windowed frontend (winit + wgpu) sits behind the default-on `window`
//! feature, while its headless subcommands always build — and the
//! `pixel8-tui` crate (sixel/half-block terminal rendering). Frontends
//! that don't want the GPU stack depend on this crate with
//! `default-features = false`; everything a frontend needs — ticking the
//! [`shell::Shell`], feeding it keys and mouse state, presenting the
//! framebuffer it draws — is exported here.

pub mod builder;
mod clipboard;
mod editor;
#[cfg(feature = "window")]
pub mod gpu;
pub mod shell;
pub mod ui;
mod watch;
pub mod webexport;

use std::{
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

/// One tick's wall-clock budget at a given rate (30 normally, 60 while a
/// 60 fps cart runs).
pub fn frame_duration(fps: u32) -> Duration {
    Duration::from_nanos(1_000_000_000 / fps.max(1) as u64)
}

/// Paces a fixed-rate tick loop against the wall clock, one pass at a time.
///
/// A frontend alternates between handling input and running the ticks that are due. Each pass
/// reads the clock once and runs the ticks owed at that instant, however long they take to run;
/// whatever falls due meanwhile waits for the next pass, after the frontend has handled the input
/// that arrived in between — a key release, `Esc`, a resize. A pass that asked the clock again
/// after every tick would keep finding another one due for as long as ticks run slower than real
/// time (a heavy cart, an unoptimized build), and would never hand control back.
pub struct TickPacer {
    next: Instant,
}

impl TickPacer {
    /// A pacer whose first tick is due at `now`.
    pub fn new(now: Instant) -> Self {
        Self { next: now }
    }

    /// Whether a tick is due by `now`, the instant the calling pass began, counting it as run if
    /// so.
    ///
    /// A pass asks with the same `now` until the answer is `false`, running one tick per `true`.
    /// A pass still more than ten frames behind after its first tick drops the rest of the
    /// backlog rather than running it back to back: that tick is all it runs, and the next falls
    /// due a frame after `now`.
    pub fn tick_due(&mut self, now: Instant, frame: Duration) -> bool {
        if now < self.next {
            return false;
        }
        self.next += frame;
        // Too far behind to catch up without stalling the frontend: drop the backlog.
        if now > self.next + frame * 10 {
            self.next = now + frame;
        }
        true
    }

    /// The instant the next tick falls due.
    pub fn next_tick(&self) -> Instant {
        self.next
    }
}

/// Where the `pixel8` SDK crate lives, for generated project manifests.
/// Defaults to this source tree; override with PIXEL8_SDK for installs.
pub fn sdk_path() -> PathBuf {
    if let Ok(p) = std::env::var("PIXEL8_SDK") {
        return PathBuf::from(p);
    }
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../pixel8")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_pass_runs_only_the_ticks_already_due() {
        let t0 = Instant::now();
        let frame = Duration::from_millis(10);
        let mut pacer = TickPacer::new(t0);
        // 2.5 frames behind: three ticks are owed, then none, however many
        // more times the pass asks with the same `now`.
        let now = t0 + frame * 2 + frame / 2;
        let mut ticks = 0;
        while pacer.tick_due(now, frame) {
            ticks += 1;
        }
        assert_eq!(ticks, 3);
        assert!(!pacer.tick_due(now, frame));
    }

    #[test]
    fn a_long_stall_skips_ahead_with_a_single_tick() {
        let t0 = Instant::now();
        let frame = Duration::from_millis(10);
        let mut pacer = TickPacer::new(t0);
        // Far past the 10-frame grace period.
        let now = t0 + frame * 15;
        assert!(pacer.tick_due(now, frame));
        assert_eq!(pacer.next_tick(), now + frame);
        assert!(!pacer.tick_due(now, frame));
    }

    #[test]
    fn next_tick_reports_the_scheduled_instant() {
        let t0 = Instant::now();
        let frame = Duration::from_millis(10);
        let mut pacer = TickPacer::new(t0);
        assert_eq!(pacer.next_tick(), t0);
        assert!(pacer.tick_due(t0, frame));
        assert_eq!(pacer.next_tick(), t0 + frame);
    }
}
