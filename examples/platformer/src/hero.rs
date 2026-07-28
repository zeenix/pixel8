use pixel8::{
    physics::{Collider, Gravity, Kinetic, Velocity},
    Body, Button, Context, Graphics, SpriteId, SCREEN_WIDTH,
};

use crate::{
    constants::{
        COIN_SFX, COIN_SPRITE, HERO_HAPPY_SPRITE, HERO_HITBOX, HERO_LEGS_EXTEND_SPRITE, HERO_SPEED,
        HERO_SPRITE, JUMP_SFX, SOLID, TROPHY_SPRITE,
    },
    GameMode, Taken,
};

/// The level's pull, and the whole of the weather the hero walks around in — a constant,
/// so nothing holds a copy of it and no level has to pass one around.
const GRAVITY: Gravity = Gravity::new();

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

        // One call moves the hero: the pull it is handed, the tiles its collider stops
        // at, and the body moved by what survives — diagonals included, so a running
        // jump climbs a clean staircase. The fall the pull builds up is capped by the
        // gravity's terminal velocity, without which a long drop would clear a whole
        // tile in one update and land inside the floor.
        let contacts = self.step(ctx, &[&GRAVITY]);
        self.grounded = contacts.below();

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

    // Camera follows the player across the 32-tile-wide level.
    pub fn center(&self, gfx: &mut Graphics) {
        let cam = (self.body.x() - 60.0).clamp(8.0, (32 * 8 - SCREEN_WIDTH as i16) as f32);
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

    pub fn draw_x(&self) -> i16 {
        self.body.draw_x()
    }

    pub fn draw_y(&self) -> i16 {
        self.body.draw_y()
    }

    pub fn die(&mut self) {
        self.dead = true;
    }
}

// Everything the SDK's physics needs to move the hero: the body it occupies, the
// velocity forces bend, and the shape it is when it meets a tile. `step` does the rest
// — no gravity to add by hand, no corners to check against the map.
impl Kinetic for Hero {
    fn body_mut(&mut self) -> &mut Body {
        &mut self.body
    }

    fn velocity_mut(&mut self) -> &mut Velocity {
        &mut self.velocity
    }

    fn collider(&self) -> Option<Collider> {
        // One sprite's worth of hitbox, stopping at the tiles the level flags solid.
        Collider::new(HERO_HITBOX, HERO_HITBOX, SOLID).ok()
    }
}
