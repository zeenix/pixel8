use pixel8::{
    physics::{Bounds, Contacts, Kinetic, Velocity},
    BitFlags, Body, Graphics, SpriteFlag, SpriteId,
};

use crate::{
    constants::{
        BADIE_ALT_SPRITE, BADIE_END_X, BADIE_HEIGHT, BADIE_SPEED, BADIE_SPRITE, BADIE_START_X,
        BADIE_WIDTH, BADIE_Y,
    },
    GameMode,
};

#[derive(Debug)]
pub struct Badie {
    body: Body,
    velocity: Velocity,
    contacts: Contacts,
    flip: bool,
}

impl Badie {
    pub fn new() -> Self {
        Self {
            body: Body::new(BADIE_START_X, BADIE_Y),
            velocity: Velocity::new(-BADIE_SPEED, 0.0),
            contacts: Contacts::default(),
            flip: false,
        }
    }

    /// Our badie patrols horizontally back & forth between two points: turn at each end, ask for
    /// a step. What it means to do goes into its velocity, exactly like the hero's steering —
    /// the world does the walking, and the floor under it does the holding.
    pub fn patrol(&mut self) {
        if self.body.x() < BADIE_END_X {
            self.flip = true;
        } else if self.body.x() > BADIE_START_X {
            self.flip = false;
        }
        self.velocity.dx = if self.flip { BADIE_SPEED } else { -BADIE_SPEED };
    }

    pub fn draw(&self, gfx: &mut Graphics, frame: u32, mode: &GameMode) {
        let sprite = match mode {
            GameMode::InGame { .. } if (frame / 4).is_multiple_of(2) => BADIE_ALT_SPRITE,
            GameMode::Ended { .. } | GameMode::InGame { .. } => BADIE_SPRITE,
            GameMode::Init => unreachable!(),
        };
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
}

// The badie is as ordinary a cast member as the hero: the pull reaches it, the level's floor
// holds it up, and the hero meets it wherever the world has just put it. The patrol is the one
// thing that stays its own — a velocity written each update, like the hero's steering.
impl Kinetic for Badie {
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

    /// The badie's hitbox, full stop: the sprite's whole square — what the floor holds up, where
    /// the cast meets it, and what the hero's ram-or-stomp is told apart by.
    fn bounds(&self) -> Bounds {
        Bounds::of(&self.body, BADIE_WIDTH, BADIE_HEIGHT)
    }

    /// What the badie is made of. Both of its walk-cycle sprites carry the `BADIE` flag in the
    /// sprite editor, so either one answers for it — and that flag is what the hero's step comes
    /// back with. What stops the badie it leaves to the world's word, which is exactly right for
    /// something that walks a floor; the hero wears nothing, so it is never in the way.
    fn sprite(&self) -> Option<SpriteId> {
        Some(BADIE_SPRITE)
    }

    // The badie reads nothing of what it meets — what a meeting costs is settled in `lib.rs`, off
    // the hero's contacts — so the world need not work out a word of it.
    fn heeds(&self) -> BitFlags<SpriteFlag> {
        BitFlags::empty()
    }
}
