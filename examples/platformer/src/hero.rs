use pixel8::{
    physics::{Bounds, Member},
    Button, Context, Graphics, SpriteId, SCREEN_WIDTH,
};

use crate::{
    constants::{
        Scene, BADIE, COIN_SFX, COIN_SPRITE, HERO_HAPPY_SPRITE, HERO_HEIGHT,
        HERO_LEGS_EXTEND_SPRITE, HERO_SPEED, HERO_SPRITE, HERO_WIDTH, JUMP_SFX, LEVEL_HEIGHT,
        LEVEL_WIDTH, TROPHY_SPRITE,
    },
    GameMode, Taken,
};

/// The level itself, which is as far as the hero is allowed to get. A rectangle the cart knows
/// at compile time, so it is written down once rather than built every update.
const LEVEL: Bounds = Bounds::new(0, 0, LEVEL_WIDTH, LEVEL_HEIGHT);

/// Where a run starts.
const START_X: f32 = 16.0;
const START_Y: f32 = 80.0;

#[derive(Debug)]
pub struct Hero {
    /// The hero's seat in the scene.
    member: Member,
    /// Whether the buttons are asking the hero to walk, which is what the walk cycle is
    /// drawn from. Not the velocity: the step zeroes an axis that ran into a wall, and a
    /// hero leaning on one is still walking as far as the animation is concerned.
    walking: bool,
    flip: bool,
    dead: bool,
}

impl Hero {
    /// Seats the hero in `scene` at the start of a run, listening for the badie and nothing
    /// else.
    ///
    /// One sprite's worth of rectangle — what the walls stop, where the badie is met, and what the
    /// level's edges hold. The last column has nothing but sky past it, and the level's edges are
    /// no tiles at all, so the level itself is what holds the hero in.
    ///
    /// Its sprites carry no flags, so there is nothing in it for the badie to meet; the walls it
    /// stops at are the ones `lib.rs` declares on the world.
    pub fn new(scene: &mut Scene) -> Self {
        Self {
            member: scene
                .enlist(START_X, START_Y, HERO_WIDTH, HERO_HEIGHT)
                .expect("a seat for the hero")
                .confined_to(LEVEL)
                .heeding(BADIE)
                .member(),
            walking: false,
            flip: false,
            dead: false,
        }
    }

    /// The hero's seat, for the comparison in `lib.rs` that tells a stomp from a ram.
    pub fn member(&self) -> Member {
        self.member
    }

    /// Gives the seat back, at the end of a run.
    pub fn retire(&self, scene: &mut Scene) {
        scene.retire(self.member);
    }

    /// Whether the hero is standing on something: floor tiles, and the bottom of the level for a
    /// level not floored across its whole width. This one is, so only the tiles ever answer here.
    pub fn grounded(&self, scene: &Scene) -> bool {
        scene.contacts(self.member).below()
    }

    /// Whether the hero's last step met the badie. `BADIE` is deliberately no wall of the
    /// hero's; what the meeting costs is decided in `lib.rs`, where the badie lives.
    pub fn met_badie(&self, scene: &Scene) -> bool {
        scene.contacts(self.member).touches(BADIE)
    }

    /// What the buttons ask for, written into the hero's velocity before the world runs.
    pub fn steer(&mut self, ctx: &mut Context, scene: &mut Scene) {
        let mut velocity = scene.velocity(self.member);
        // Horizontal movement (pixels per frame), written afresh every update: a wall
        // the step walks into zeroes it, and the buttons put it straight back.
        if ctx.is_button_down(Button::Left) {
            velocity.dx = -HERO_SPEED;
            self.flip = true;
        } else if ctx.is_button_down(Button::Right) {
            velocity.dx = HERO_SPEED;
            self.flip = false;
        } else {
            velocity.dx = 0.0;
        }
        // Taken here, from the buttons, rather than from what survives the step.
        self.walking = velocity.dx != 0.0;
        scene.set_velocity(self.member, velocity);

        // Jump before the world runs, so the push is part of the same update's movement.
        if self.grounded(scene)
            && (ctx.is_button_pressed(Button::O) || ctx.is_button_pressed(Button::Up))
        {
            self.jump(ctx, scene);
        }
    }

    /// Whatever the hero is standing on gets picked up, now that the world has put it there: a
    /// coin, or the trophy. The walls and the level's edges are already settled by now; what this
    /// level leans on for them is the pull's terminal velocity — its floors are one tile thick,
    /// and a fall fast enough to clear one in a single update would land inside it.
    pub fn pick_up(&mut self, ctx: &mut Context, scene: &Scene) -> Option<Taken> {
        // Coins & trophy: sample the hitbox center.
        let (x, y) = scene.pos(self.member);
        let cx = (x as i16 + 4) / 8;
        let cy = (y as i16 + 4) / 8;
        match ctx.map_tile(cx, cy) {
            Some(COIN_SPRITE) => {
                ctx.set_map_tile(cx, cy, SpriteId(0)).unwrap();
                ctx.sfx(COIN_SFX);
                Some(Taken::new_coin(cx, cy))
            }
            Some(TROPHY_SPRITE) => {
                ctx.set_map_tile(cx, cy, SpriteId(0)).unwrap();
                Some(Taken::new_trophy(cx, cy))
            }
            _ => None,
        }
    }

    pub fn jump(&mut self, ctx: &mut Context, scene: &mut Scene) {
        let mut velocity = scene.velocity(self.member);
        velocity.dy = -3.25;
        scene.set_velocity(self.member, velocity);
        ctx.sfx(JUMP_SFX);
    }

    // Camera follows the player across the level.
    pub fn center(&self, gfx: &mut Graphics, scene: &Scene) {
        let cam = (scene.pos(self.member).0 - 60.0).clamp(8.0, (LEVEL_WIDTH - SCREEN_WIDTH) as f32);
        gfx.camera(cam as i16, 0);
    }

    pub fn draw(&self, gfx: &mut Graphics, scene: &Scene, frame: u32, mode: &GameMode) {
        let is_alt_frame = (frame / 4).is_multiple_of(2);
        if self.dead && is_alt_frame {
            // If hero dies, we show them flashing in & out of existence.
            return;
        }

        let sprite = if !self.grounded(scene) || (self.walking && is_alt_frame) {
            match mode {
                GameMode::Ended { won, .. } if *won => HERO_HAPPY_SPRITE,
                GameMode::InGame { .. } | GameMode::Ended { .. } => HERO_LEGS_EXTEND_SPRITE,
                GameMode::Init => unreachable!(),
            }
        } else {
            HERO_SPRITE
        };
        // The world's coherent pixel — a running jump climbs cleanly, no zigzag.
        let (x, y) = scene.draw_pos(self.member);
        gfx.sprite_ext(sprite, x, y, 8, 8, self.flip, false)
            .unwrap();
    }

    pub fn die(&mut self) {
        self.dead = true;
    }
}
