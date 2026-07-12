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
    time::Duration,
};

/// One tick's wall-clock budget at a given rate (30 normally, 60 while a
/// 60 fps cart runs).
pub fn frame_duration(fps: u32) -> Duration {
    Duration::from_nanos(1_000_000_000 / fps.max(1) as u64)
}

/// Where the `pixel8` SDK crate lives, for generated project manifests.
/// Defaults to this source tree; override with PIXEL8_SDK for installs.
pub fn sdk_path() -> PathBuf {
    if let Ok(p) = std::env::var("PIXEL8_SDK") {
        return PathBuf::from(p);
    }
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../pixel8")
}
