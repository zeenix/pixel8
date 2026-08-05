use pixel8::{
    physics::{Bounds, Contacts, Kinetic, Velocity},
    BitFlags, Body, Button, Context, Graphics, SpriteFlag, SpriteId, SCREEN_WIDTH,
};

use crate::{
    constants::{
        BADIE, COIN_SFX, COIN_SPRITE, HERO_HAPPY_SPRITE, HERO_HEIGHT, HERO_LEGS_EXTEND_SPRITE,
        HERO_SPEED, HERO_SPRITE, HERO_WIDTH, JUMP_SFX, LEVEL_HEIGHT, LEVEL_WIDTH, TROPHY_SPRITE,
    },
    GameMode, Taken,
};

/// The level itself, which is as far as the hero is allowed to get. A rectangle the cart knows
/// at compile time, so it is written down once rather than built every update.
const LEVEL: Bounds = Bounds::new(0, 0, LEVEL_WIDTH, LEVEL_HEIGHT);

#[derive(Debug)]
pub struct Hero {
    body: Body,
    velocity: Velocity,
    /// What the world's last step ran into: the floor under the hero, the walls beside it, the
    /// edge of the level and the badie, all in one answer. The world writes it; everything below
    /// reads it.
    contacts: Contacts,
    /// Whether the buttons are asking the hero to walk, which is what the walk cycle is
    /// drawn from. Not the velocity: the step zeroes an axis that ran into a wall, and a
    /// hero leaning on one is still walking as far as the animation is concerned.
    walking: bool,
    flip: bool,
    dead: bool,
}

impl Hero {
    pub fn new() -> Self {
        Self {
            body: Body::new(16.0, 80.0),
            velocity: Velocity::default(),
            contacts: Contacts::default(),
            walking: false,
            flip: false,
            dead: false,
        }
    }

    /// Whether the hero is standing on something: floor tiles, and the bottom of the level for a
    /// level not floored across its whole width. This one is, so only the tiles ever answer here.
    pub fn grounded(&self) -> bool {
        self.contacts.below()
    }

    /// Whether the hero's last step met the badie. `BADIE` is deliberately no wall of the hero's,
    /// so the world reports it and never stops at it; what it costs the hero is decided in
    /// `lib.rs`, which is the one that knows whose badie it was.
    pub fn met_badie(&self) -> bool {
        self.contacts.touches(BADIE)
    }

    /// What the buttons ask for, written into the velocity before the world runs. Nothing here
    /// moves the hero — that is the world's, once, for the whole cast.
    pub fn steer(&mut self, ctx: &mut Context) {
        // Horizontal movement (pixels per frame), written afresh every update: a wall
        // the step walks into zeroes it, and the buttons put it straight back.
        if ctx.is_button_down(Button::Left) {
            self.velocity.dx = -HERO_SPEED;
            self.flip = true;
        } else if ctx.is_button_down(Button::Right) {
            self.velocity.dx = HERO_SPEED;
            self.flip = false;
        } else {
            self.velocity.dx = 0.0;
        }
        // Taken here, from the buttons, rather than from what survives the step.
        self.walking = self.velocity.dx != 0.0;
        // Jump before the world runs, so the push is part of the same update's movement.
        if self.grounded()
            && (ctx.is_button_pressed(Button::O) || ctx.is_button_pressed(Button::Up))
        {
            self.jump(ctx);
        }
    }

    /// Whatever the hero is standing on gets picked up, now that the world has put it there: a
    /// coin, or the trophy. The walls and the level's edges are already settled by now; what this
    /// level leans on for them is the pull's terminal velocity — its floors are one tile thick,
    /// and a fall fast enough to clear one in a single update would land inside it.
    pub fn pick_up(&mut self, ctx: &mut Context) -> Option<Taken> {
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

        let sprite = if !self.grounded() || (self.walking && is_alt_frame) {
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

// Everything the SDK's physics needs to move the hero, and nothing more: the body, the velocity,
// the slot it is told what it met in, one sprite's worth of rectangle and the level for the edge
// of the world. Nothing here detects anything — the world does all of it, for the whole cast, in
// one call.
//
// The hero says nothing about what it wears, which is the default: its sprites carry no flags, so
// there is nothing in it for anybody else to meet. Nor about what stops it — the walls are the
// scene's to declare, and `lib.rs` declares them on the world, once, for everybody.
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

    fn contacts(&self) -> &Contacts {
        &self.contacts
    }

    fn contacts_mut(&mut self) -> &mut Contacts {
        &mut self.contacts
    }

    // One sprite's worth: what the walls stop, what the badie is judged against, and what
    // the level's edges hold.
    fn bounds(&self) -> Bounds {
        Bounds::of(&self.body, HERO_WIDTH, HERO_HEIGHT)
    }

    // The badie is the one thing the hero wants to hear about beyond its walls, so the world
    // spends nothing telling it about anything else.
    fn heeds(&self) -> BitFlags<SpriteFlag> {
        BADIE.into()
    }

    // The last column has nothing but sky past it, and the level's edges are not tiles, so the
    // walls above cannot hold the hero in. The level itself does.
    fn confines(&self) -> Option<Bounds> {
        Some(LEVEL)
    }
}
