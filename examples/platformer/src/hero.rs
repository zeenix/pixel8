use pixel8::{
    physics::{Bounds, Gravity, Kinetic, Velocity},
    BitFlags, Body, Button, Context, Graphics, SpriteFlag, SpriteId, SCREEN_WIDTH,
};

use crate::{
    constants::{
        COIN_SFX, COIN_SPRITE, HERO_HAPPY_SPRITE, HERO_HEIGHT, HERO_LEGS_EXTEND_SPRITE, HERO_SPEED,
        HERO_SPRITE, HERO_WIDTH, JUMP_SFX, LEVEL_HEIGHT, LEVEL_WIDTH, SOLID, TROPHY_SPRITE,
    },
    GameMode, Taken,
};

/// The level's pull, and the whole of the weather the hero walks around in — a constant,
/// so nothing holds a copy of it and no level has to pass one around.
const GRAVITY: Gravity = Gravity::new();

/// The level itself, which is as far as the hero is allowed to get. A rectangle the cart knows
/// at compile time, so it is written down once rather than built every update.
const LEVEL: Bounds = Bounds::new(0, 0, LEVEL_WIDTH, LEVEL_HEIGHT);

#[derive(Debug)]
pub struct Hero {
    body: Body,
    velocity: Velocity,
    /// Whether the buttons are asking the hero to walk, which is what the walk cycle is
    /// drawn from. Not the velocity: the step zeroes an axis that ran into a wall, and a
    /// hero leaning on one is still walking as far as the animation is concerned.
    walking: bool,
    flip: bool,
    dead: bool,
    grounded: bool,
}

impl Hero {
    pub fn new() -> Self {
        Self {
            body: Body::new(16.0, 80.0),
            velocity: Velocity::default(),
            walking: false,
            flip: false,
            dead: false,
            grounded: false,
        }
    }

    pub fn update(&mut self, ctx: &mut Context) -> Option<Taken> {
        // Horizontal movement (pixels per frame), written afresh every update: a wall
        // the step below walks into zeroes it, and the buttons put it straight back.
        if ctx.is_button_down(Button::Left) {
            self.velocity.dx = -HERO_SPEED;
            self.flip = true;
        } else if ctx.is_button_down(Button::Right) {
            self.velocity.dx = HERO_SPEED;
            self.flip = false;
        } else {
            self.velocity.dx = 0.0;
        }
        // Taken here, from the buttons, rather than from what survives the step below.
        self.walking = self.velocity.dx != 0.0;
        // Jump first, so the push is part of the same update's movement.
        if self.grounded && (ctx.is_button_pressed(Button::O) || ctx.is_button_pressed(Button::Up))
        {
            self.jump(ctx);
        }

        // One call moves the hero: the pull it is handed, the tiles it calls solid,
        // and the body moved by what survives — diagonals included, so a running
        // jump climbs a clean staircase. The fall the pull builds up is capped by the
        // gravity's terminal velocity, without which a long drop would clear a whole
        // tile in one update and land inside the floor.
        let contacts = self.step(ctx, &[&GRAVITY]);
        // The walls are only where the level has tiles; its own edges are not walls at all, and
        // the last column has nothing but sky past it. This level is floored across its whole
        // width, so the bottom edge never comes up — `held.below()` is there for a level that
        // is not, where being pinned at the bottom should still count as standing on something.
        let held = self.keep_within(LEVEL);
        self.grounded = contacts.below() || held.below();

        // Coins & trophy: sample the hitbox center.
        let cx = (self.body.x() as i16 + 4) / 8;
        let cy = (self.body.y() as i16 + 4) / 8;
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

    pub fn jump(&mut self, ctx: &mut Context) {
        self.velocity.dy = -3.25;
        ctx.sfx(JUMP_SFX);
    }

    // Camera follows the player across the level.
    pub fn center(&self, gfx: &mut Graphics) {
        let cam = (self.body.x() - 60.0).clamp(8.0, (LEVEL_WIDTH - SCREEN_WIDTH) as f32);
        gfx.camera(cam as i16, 0);
    }

    pub fn draw(&self, gfx: &mut Graphics, frame: u32, mode: &GameMode) {
        let is_alt_frame = (frame / 4).is_multiple_of(2);
        if self.dead && is_alt_frame {
            // If hero dies, we show them flashing in & out of existence.
            return;
        }

        let sprite = if !self.grounded || (self.walking && is_alt_frame) {
            match mode {
                GameMode::Ended { won, .. } if *won => HERO_HAPPY_SPRITE,
                GameMode::InGame { .. } | GameMode::Ended { .. } => HERO_LEGS_EXTEND_SPRITE,
                GameMode::Init => unreachable!(),
            }
        } else {
            HERO_SPRITE
        };
        // The body's coherent pixel — a running jump climbs cleanly, no zigzag.
        gfx.sprite_ext(
            sprite,
            self.body.draw_x(),
            self.body.draw_y(),
            8,
            8,
            self.flip,
            false,
        )
        .unwrap();
    }

    pub fn die(&mut self) {
        self.dead = true;
    }
}

// Everything the SDK's physics needs to move the hero: the body it occupies, the
// velocity forces bend, the rectangle it covers and the tiles that stop it. `step`
// does the rest — no gravity to add by hand, no corners to check against the map.
impl Kinetic for Hero {
    fn body(&self) -> &Body {
        &self.body
    }

    fn body_mut(&mut self) -> &mut Body {
        &mut self.body
    }

    fn velocity_mut(&mut self) -> &mut Velocity {
        &mut self.velocity
    }

    // One sprite's worth: what the walls stop, what the badie is judged against, and what
    // the level's edges hold.
    fn bounds(&self) -> Bounds {
        Bounds::of(&self.body, HERO_WIDTH, HERO_HEIGHT)
    }

    fn solid(&self) -> BitFlags<SpriteFlag> {
        SOLID.into()
    }
}
