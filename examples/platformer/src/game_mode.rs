use pixel8::{Context, PlayingMusic};

use crate::constants::GAME_TIMEOUT;

#[derive(Debug)]
pub enum GameMode {
    InGame {
        start_time: f32,
        time_left: u8,
    },
    Ended {
        time: f32,
        flash: bool,
        _music: PlayingMusic,
        won: bool,
    },
}

impl GameMode {
    /// The mode the cart ships in: a run whose clock has not been read yet.
    ///
    /// A constant, like the rest of the cart's opening state — missing only the time, which no
    /// constant can ask for. `boot` stamps it with
    /// [`start`](Self::start) before the first update, so nothing ever plays a run that began at
    /// zero.
    pub const fn fresh() -> Self {
        Self::InGame {
            start_time: 0.0,
            time_left: GAME_TIMEOUT,
        }
    }

    /// Starts a run, on the clock as it stands: at boot, and again at every restart.
    pub fn start(&mut self, ctx: &mut Context) {
        *self = Self::InGame {
            start_time: ctx.time(),
            time_left: GAME_TIMEOUT,
        };
    }
}
