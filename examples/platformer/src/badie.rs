use pixel8::{
    physics::{Member, MemberId},
    BitFlags, SpriteFlag,
};

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
    member: MemberId,
}

impl Badie {
    /// Seats the badie in `scene`, at the start of a run.
    ///
    /// Both of its walk-cycle sprites carry the `BADIE` flag in the sprite editor, so either one
    /// answers for it. What a meeting costs is settled in `lib.rs`, off the hero's contacts, so
    /// the badie itself listens for nothing.
    pub fn new(scene: &mut Scene) -> Self {
        Self {
            member: Member::builder(BADIE_START_X, BADIE_Y, BADIE_WIDTH, BADIE_HEIGHT)
                .moving(-BADIE_SPEED, 0.0)
                .wearing(BADIE_SPRITE)
                .heeding(BitFlags::<SpriteFlag>::empty())
                .enlist(scene)
                .expect("a seat for the badie"),
        }
    }

    /// The badie's seat: its rectangle is what the hero's ram-or-stomp is told apart by.
    pub fn member(&self) -> MemberId {
        self.member
    }

    /// Gives the seat back — stomped, or the run over.
    pub fn retire(self, scene: &mut Scene) {
        scene.member_mut(self.member).retire();
    }

    /// Our badie patrols horizontally back and forth between two points, turning at each end.
    /// What it means to do goes into its velocity, exactly like the hero's steering.
    pub fn patrol(&self, scene: &mut Scene) {
        // The badie faces the way it walks; the world's flip is the one copy of that fact,
        // so the patrol reads it back instead of keeping one of its own.
        let mut badie = scene.member_mut(self.member);
        let (mut heading_right, _) = badie.flip();
        let x = badie.pos().0;
        if x < BADIE_END_X {
            heading_right = true;
        } else if x > BADIE_START_X {
            heading_right = false;
        }
        badie.set_flip(heading_right, false);
        let mut velocity = badie.velocity();
        velocity.dx = if heading_right {
            BADIE_SPEED
        } else {
            -BADIE_SPEED
        };
        badie.set_velocity(velocity);
    }

    /// What the badie looks like this frame, written into its seat for the world to draw.
    ///
    /// Both cells carry the `BADIE` flag in the sprite editor, so switching between them never
    /// changes what the hero meets.
    pub fn animate(&self, scene: &mut Scene, frame: u32, mode: &GameMode) {
        let sprite = match mode {
            GameMode::InGame { .. } if (frame / 4).is_multiple_of(2) => BADIE_ALT_SPRITE,
            GameMode::Ended { .. } | GameMode::InGame { .. } => BADIE_SPRITE,
        };
        scene.member_mut(self.member).set_sprite(Some(sprite));
    }
}
