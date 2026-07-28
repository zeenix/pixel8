//! What a force bends and a body is moved by.

/// How fast something is travelling, in pixels per update on each axis.
///
/// This is the movement one update produces — the pair [`Body::move_by`](crate::Body::move_by)
/// takes — so a `dx` of `0.5` crosses the screen in about four seconds at 60 fps.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Velocity {
    pub dx: f32,
    pub dy: f32,
}

impl Velocity {
    /// A velocity of `dx` pixels an update sideways and `dy` down the screen.
    ///
    /// `const`, so a cart's own speeds can be constants of its own:
    /// `const DRIFT: Velocity = Velocity::new(0.25, 0.0);`. Standing still is
    /// [`Velocity::default`], or `Velocity::new(0.0, 0.0)` where a `const` is wanted.
    pub const fn new(dx: f32, dy: f32) -> Self {
        Self { dx, dy }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_default_velocity_is_a_standstill() {
        const STILL: Velocity = Velocity::new(0.0, 0.0);
        assert_eq!(Velocity::default(), STILL);
        assert_eq!(Velocity::new(1.5, -2.0), Velocity { dx: 1.5, dy: -2.0 });
    }
}
