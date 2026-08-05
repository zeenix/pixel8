use pixel8::{physics::Member, BitFlags, Graphics, SpriteFlag};

use crate::{
    constants::{
        Scene, BADIE_ALT_SPRITE, BADIE_END_X, BADIE_HEIGHT, BADIE_SPEED, BADIE_SPRITE,
        BADIE_START_X, BADIE_WIDTH, BADIE_Y,
    },
    GameMode,
};

#[derive(Debug)]
pub struct Badie {
    /// The badie's seat in the scene.
    member: Member,
    flip: bool,
}

impl Badie {
    /// Seats the badie in `scene`, at the start of a run.
    ///
    /// Both of its walk-cycle sprites carry the `BADIE` flag in the sprite editor, so either one
    /// answers for it. What a meeting costs is settled in `lib.rs`, off the hero's contacts, so
    /// the badie itself listens for nothing.
    pub fn new(scene: &mut Scene) -> Self {
        Self {
            member: scene
                .enlist(BADIE_START_X, BADIE_Y, BADIE_WIDTH, BADIE_HEIGHT)
                .expect("a seat for the badie")
                .moving(-BADIE_SPEED, 0.0)
                .wearing(BADIE_SPRITE)
                .heeding(BitFlags::<SpriteFlag>::empty())
                .member(),
            flip: false,
        }
    }

    /// The badie's seat: its rectangle is what the hero's ram-or-stomp is told apart by.
    pub fn member(&self) -> Member {
        self.member
    }

    /// Gives the seat back — stomped, or the run over.
    pub fn retire(self, scene: &mut Scene) {
        scene.retire(self.member);
    }

    /// Our badie patrols horizontally back and forth between two points, turning at each end.
    /// What it means to do goes into its velocity, exactly like the hero's steering.
    pub fn patrol(&mut self, scene: &mut Scene) {
        let x = scene.pos(self.member).0;
        if x < BADIE_END_X {
            self.flip = true;
        } else if x > BADIE_START_X {
            self.flip = false;
        }
        let mut velocity = scene.velocity(self.member);
        velocity.dx = if self.flip { BADIE_SPEED } else { -BADIE_SPEED };
        scene.set_velocity(self.member, velocity);
    }

    pub fn draw(&self, gfx: &mut Graphics, scene: &Scene, frame: u32, mode: &GameMode) {
        let sprite = match mode {
            GameMode::InGame { .. } if (frame / 4).is_multiple_of(2) => BADIE_ALT_SPRITE,
            GameMode::Ended { .. } | GameMode::InGame { .. } => BADIE_SPRITE,
            GameMode::Init => unreachable!(),
        };
        let (x, y) = scene.draw_pos(self.member);
        gfx.sprite_ext(sprite, x, y, 8, 8, self.flip, false)
            .unwrap();
    }
}
