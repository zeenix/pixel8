//! True key press/release state from the kernel's input devices (evdev),
//! for terminals that can't report key releases.
//!
//! A terminal without the kitty keyboard protocol only ever says "key
//! pressed", and the OS autorepeats just the most recently pressed key —
//! so chords (diagonals, fire-while-moving) are unrecoverable from the
//! escape stream alone. Reading `/dev/input` sidesteps the terminal
//! entirely and works under Wayland, X11 and the bare console alike
//! (it is the same source the handheld player uses). The cost is a
//! permission: `/dev/input` is readable only by root and the `input`
//! group, so most desktop users must opt in once with
//! `sudo usermod -aG input $USER` (applied to the current shell with
//! `newgrp input`; new logins pick it up automatically).
//!
//! Privacy: only the ten keys that map to game buttons are tracked, the
//! state never leaves the process, and the caller gates it on terminal
//! focus so keys typed into other windows don't drive a cart.

#[cfg(target_os = "linux")]
pub use linux::RawKeys;

#[cfg(target_os = "linux")]
mod linux {
    use crate::tui::BUTTONS;
    use evdev::{Device, EventSummary, KeyCode};

    /// All readable keyboards, polled non-blocking for game-button state.
    pub struct RawKeys {
        devices: Vec<Device>,
        down: [bool; BUTTONS],
    }

    impl RawKeys {
        /// Open every readable keyboard-capable device. `None` when there
        /// is none — no `/dev/input` access (not in the `input` group, an
        /// SSH session, a container) — in which case the caller falls
        /// back to inferring releases from autorepeat.
        pub fn start() -> Option<RawKeys> {
            let mut devices: Vec<Device> = evdev::enumerate()
                .map(|(_, d)| d)
                .filter(is_keyboard)
                .collect();
            for d in &mut devices {
                let _ = d.set_nonblocking(true);
            }
            (!devices.is_empty()).then_some(RawKeys {
                devices,
                down: [false; BUTTONS],
            })
        }

        /// Drain pending key events and return which buttons are held.
        pub fn poll(&mut self) -> [bool; BUTTONS] {
            let mut events = Vec::new();
            // A device that errors for real (e.g. unplugged) is dropped.
            self.devices.retain_mut(|dev| match dev.fetch_events() {
                Ok(iter) => {
                    events.extend(iter);
                    true
                }
                Err(e) => e.kind() == std::io::ErrorKind::WouldBlock,
            });
            for ev in events {
                // 1 = press, 0 = release; 2 = autorepeat, meaningless here.
                if let EventSummary::Key(_, code, val) = ev.destructure() {
                    if val != 2 {
                        if let Some(b) = button_for(code) {
                            self.down[b] = val == 1;
                        }
                    }
                }
            }
            self.down
        }
    }

    /// A keyboard for our purposes: something with arrows and letters.
    /// Excludes mice, power buttons, lid switches and most gamepads.
    fn is_keyboard(dev: &Device) -> bool {
        dev.supported_keys()
            .is_some_and(|keys| keys.contains(KeyCode::KEY_LEFT) && keys.contains(KeyCode::KEY_Z))
    }

    /// The six game buttons, mirroring the windowed console's physical-key
    /// bindings (and `game_button` for terminal key events).
    fn button_for(key: KeyCode) -> Option<usize> {
        Some(match key {
            KeyCode::KEY_LEFT => 0,
            KeyCode::KEY_RIGHT => 1,
            KeyCode::KEY_UP => 2,
            KeyCode::KEY_DOWN => 3,
            KeyCode::KEY_Z | KeyCode::KEY_C | KeyCode::KEY_N => 4,
            KeyCode::KEY_X | KeyCode::KEY_V | KeyCode::KEY_M => 5,
            _ => return None,
        })
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn buttons_match_the_windowed_console() {
            assert_eq!(button_for(KeyCode::KEY_LEFT), Some(0));
            assert_eq!(button_for(KeyCode::KEY_DOWN), Some(3));
            for k in [KeyCode::KEY_Z, KeyCode::KEY_C, KeyCode::KEY_N] {
                assert_eq!(button_for(k), Some(4));
            }
            for k in [KeyCode::KEY_X, KeyCode::KEY_V, KeyCode::KEY_M] {
                assert_eq!(button_for(k), Some(5));
            }
            assert_eq!(button_for(KeyCode::KEY_A), None);
            assert_eq!(button_for(KeyCode::KEY_ENTER), None);
        }
    }
}

#[cfg(not(target_os = "linux"))]
pub use fallback::RawKeys;

#[cfg(not(target_os = "linux"))]
mod fallback {
    use crate::tui::BUTTONS;

    /// No raw keyboard source on this platform; `start` always declines
    /// and the caller falls back to autorepeat-inferred releases.
    pub struct RawKeys;

    impl RawKeys {
        pub fn start() -> Option<RawKeys> {
            None
        }

        pub fn poll(&mut self) -> [bool; BUTTONS] {
            [false; BUTTONS]
        }
    }
}
