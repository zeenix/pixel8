//! A tiny platformer: run and jump across a scrolling level, collect coins,
//! grab the trophy to win, and dodge or stomp the patrolling badie — all
//! before the 30-second clock runs out. Solid tiles carry sprite flag 0;
//! coins (tile 3) and the trophy (tile 4) are collected by rewriting the map,
//! and put back when the game restarts.
//!
//! The best score is kept between runs with [`Context::storage_get`] /
//! [`Context::storage_set`] — loaded once at startup, saved whenever a run
//! beats it — so the high score survives closing the console.
//!
//! The falling, the walls and the badie all come from the SDK's `physics` module
//! (the `physics` feature), and so does everything about where the hero and the
//! badie *are*. The scene is one [`World`](pixel8::physics::World) of two seats: the
//! walls are declared on it once — flag 0, the same one the map editor marks the
//! tiles with — it owns the level's pull, a [`Gravity`](pixel8::physics::Gravity)
//! handed over at the start, and it owns the position, the velocity, the rectangle
//! and the contacts of both actors. Each of them is
//! [enlisted](pixel8::physics::World::enlist) once — one sprite's worth of rectangle,
//! the level itself as the edge the hero may never leave, and, for the badie, the
//! sprite it wears — and keeps the [`Member`](pixel8::physics::Member) handle it gets
//! back beside its own game data.
//! Neither of them detects anything, and neither of them holds a position.
//!
//! One [`step`](pixel8::physics::World::step) an update does the moving, the stopping
//! and the reporting for both; the `below()` of what it leaves in the hero's
//! [`contacts`](pixel8::physics::World::contacts) is what this cart calls *grounded*.
//! Because the world holds the trajectory, a running jump (hold Right + jump) — a
//! sub-pixel diagonal — climbs a clean staircase instead of shimmering, and the cart
//! draws at [`draw_pos`](pixel8::physics::World::draw_pos).
//!
//! The badie is met through that same step: both of its sprites carry the `BADIE`
//! flag in the sprite editor, so [`touches`](pixel8::physics::Contacts::touches)
//! answers for it and nothing here walks a pair of casts. Whether the touch was a
//! ram or a stomp — and what it costs — is settled in this file, where the badie
//! lives; a stomped badie is [retired](pixel8::physics::World::retire) on the spot,
//! its seat free for the next run.
//!
//! The code is split into small modules: `hero` and `badie` (the two moving
//! actors), `taken` (a collected coin or trophy, so it can be scored and put
//! back), `game_mode` (the `Init` → `InGame` → `Ended` state machine), and
//! `constants`.

#![no_std]

mod badie;
mod constants;
mod game_mode;
mod hero;
mod taken;

use heapless::Vec;
use pixel8::{physics::World, *};

use crate::{
    badie::Badie,
    constants::{
        Scene, BADIE_DEAD_SFX, BADIE_KILL_POINTS, COMPLETION_MUSIC, GAME_OVER_MUSIC,
        GAME_OVER_TIMEOUT, GAME_TIMEOUT, GRAVITY, MAX_TAKEN, SOLID,
    },
    game_mode::GameMode,
    hero::Hero,
    taken::Taken,
};

game!(Platformer = Platformer::new());

struct Platformer {
    /// The one thing that holds or moves anything in this cart: the level's pull, the level's
    /// word for a wall, and the two seats the hero and the badie live in.
    scene: Scene,
    hero: Hero,
    badie: Option<Badie>,
    taken: Vec<Taken, MAX_TAKEN>,
    badies_killed: u8,
    /// The best score across runs, loaded from storage at startup and saved
    /// back whenever a run beats it.
    best_score: u16,
    frame: u32,
    mode: GameMode,
}

impl Platformer {
    fn new() -> Self {
        // The walls are the scene's to declare: one flag, said once, and everything the world
        // moves stops at the tiles carrying it.
        let mut scene = World::new().with_solid(SOLID).with_forces(GRAVITY);

        Self {
            // The badie takes the first seat and the hero the second, because seat order is
            // stepping order: the hero meets the badie where it has just walked to.
            badie: Some(Badie::new(&mut scene)),
            hero: Hero::new(&mut scene),
            scene,
            taken: Vec::new(),
            badies_killed: 0,
            best_score: 0,
            frame: 0,
            mode: GameMode::Init,
        }
    }

    fn in_game_update(&mut self, ctx: &mut Context) {
        let GameMode::InGame {
            start_time,
            time_left,
        } = &mut self.mode
        else {
            unreachable!();
        };

        if *time_left == 0 {
            self.game_over(ctx);

            return;
        }
        let elapsed = (ctx.time() - *start_time).max(0.0) as u8;
        *time_left = GAME_TIMEOUT - elapsed;

        // What each of the two means to do this update, written into its own velocity.
        self.hero.steer(ctx, &mut self.scene);
        if let Some(badie) = &mut self.badie {
            badie.patrol(&mut self.scene);
        }

        // The pull, the walls, the level's edges and the hero-meets-badie, all settled by the
        // time this returns.
        self.scene.step(ctx);

        if let Some(taken) = self.hero.pick_up(ctx, &self.scene) {
            let took_trophy = taken.is_trophy();
            self.taken.push(taken).unwrap();
            if took_trophy {
                self.record_best(ctx);
                // Another music can't be playing becase `PlayingMusic` instace has had to have been
                // dropped when the game mode switch away from `Ended`.
                let music = ctx.music(COMPLETION_MUSIC).play().unwrap();
                self.mode = GameMode::Ended {
                    time: ctx.time(),
                    flash: false,
                    _music: music,
                    won: true,
                };

                return;
            }
        }

        // The hero against the badie, off what the world left in the hero's contacts: nothing
        // here walks a pair, and what the meeting costs is settled here, which is where the badie
        // actually lives.
        //
        // The badie walked its patrol and the hero was stepped against it there, so both
        // rectangles compared here are exactly the ones that met.
        let (mut rammed, mut stomped) = (false, false);
        if let (true, Some(badie)) = (self.hero.met_badie(&self.scene), &self.badie) {
            // Level with the badie is a ram; anything else is the hero coming down on it.
            if self.scene.bounds(self.hero.member()).y() == self.scene.bounds(badie.member()).y() {
                rammed = true;
            } else {
                stomped = true;
            }
        }
        if rammed {
            // Hero ramming into badie horizontally is a suicide.
            self.hero.die();
            self.game_over(ctx);

            return;
        }
        if stomped {
            // Hero hitting the badie from the top, kills the badie and gives hero a boost. It can
            // leave the cast here and now: the world steps the cast where it stands, so the
            // meeting has already happened and nothing is waiting on a picture of it.
            if let Some(badie) = self.badie.take() {
                badie.retire(&mut self.scene);
            }
            self.badies_killed += 1;
            self.hero.jump(ctx, &mut self.scene);
            ctx.sfx(BADIE_DEAD_SFX);
        }
    }

    fn restart_game(&mut self, ctx: &mut Context) {
        // Both seats given back and taken again, badie first, so the cast is seated in the order
        // the scene works whether or not the last run ended with the badie stomped.
        if let Some(badie) = self.badie.take() {
            badie.retire(&mut self.scene);
        }
        self.hero.retire(&mut self.scene);
        self.badie = Some(Badie::new(&mut self.scene));
        self.hero = Hero::new(&mut self.scene);
        self.frame = 0;
        self.mode.start(ctx);
        self.badies_killed = 0;
        for taken in self.taken.drain(..) {
            taken.put_back(ctx);
        }
    }

    fn game_over(&mut self, ctx: &mut Context) {
        self.record_best(ctx);
        let music = ctx.music(GAME_OVER_MUSIC).play().unwrap();
        self.mode = GameMode::Ended {
            time: ctx.time(),
            flash: false,
            _music: music,
            won: false,
        };
    }

    /// This run's score: coins and the trophy taken, plus stomped badies.
    fn score(&self) -> u16 {
        self.taken.iter().map(|t| t.points() as u16).sum::<u16>()
            + self.badies_killed as u16 * BADIE_KILL_POINTS as u16
    }

    /// Save the score if it beats the stored best, so the high score
    /// survives a restart. Called once as each run ends.
    fn record_best(&mut self, ctx: &mut Context) {
        let score = self.score();
        if score > self.best_score {
            self.best_score = score;
            // One u16 can't approach the 128 KiB store cap, so this never fails.
            ctx.storage_set("best", score).unwrap();
        }
    }
}

impl Game for Platformer {
    fn update(&mut self, ctx: &mut Context) {
        self.frame += 1;

        match &mut self.mode {
            GameMode::Init => {
                // First frame: pick up the best score from a previous session.
                self.best_score = ctx
                    .storage_get("best")
                    .and_then(|v| v.as_i64())
                    .unwrap_or(0) as u16;
                self.mode.start(ctx);
            }
            GameMode::InGame { .. } => self.in_game_update(ctx),
            GameMode::Ended { time, .. } if ctx.time() - *time > GAME_OVER_TIMEOUT => {
                self.restart_game(ctx)
            }
            // Flash on every 16th frame if game ended with winning.
            GameMode::Ended {
                flash, won: true, ..
            } => *flash = self.frame.is_multiple_of(16),
            GameMode::Ended { .. } => (),
        }
    }

    fn draw(&self, gfx: &mut Graphics) {
        if matches!(self.mode, GameMode::Ended { flash: true, .. }) {
            gfx.clear(Color::WHITE);
        } else {
            gfx.clear(Color::DARK_BLUE);
        }

        self.hero.center(gfx, &self.scene);
        gfx.map(0, 0, 0, 0, 32, 16, BitFlags::empty()).unwrap();

        self.hero.draw(gfx, &self.scene, self.frame, &self.mode);
        if let Some(badie) = &self.badie {
            badie.draw(gfx, &self.scene, self.frame, &self.mode);
        }

        gfx.camera(0, 0);

        let score = self.score();
        printf!(gfx, 2, 2, Color::YELLOW, "Score {}", score);
        // Track the record live so it ticks up the moment you beat it.
        printf!(
            gfx,
            2,
            9,
            Color::LIGHT_GREY,
            "Best {}",
            self.best_score.max(score)
        );

        if let GameMode::InGame { time_left, .. } = self.mode {
            let color = if time_left < 5 {
                Color::RED
            } else {
                Color::YELLOW
            };
            printf!(
                gfx,
                (SCREEN_WIDTH - 3 * 4) as i16,
                2,
                color,
                "{:>2}s",
                time_left
            );
        }
    }
}
