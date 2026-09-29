//! Things worth crossing the map for.
//!
//! Free roam only means something if there is a reason to take the long way.
//! Each of these is a landmark you can spot from a distance and a detour you
//! have to survive.

/// Generous on purpose. Walking over something should count as collecting it;
/// pixel-perfect pickup on a scrolling map is just frustration.
pub const PICK_RADIUS: f32 = 2.6;
/// Inside this radius a pickup slides toward MOVA, so grabbing one never comes
/// down to a frame of timing.
pub const MAGNET: f32 = 7.0;
pub const MAGNET_PULL: f32 = 30.0;
/// Vertical bob amplitude, in world units. Enough to read as floating.
pub const BOB: f32 = 0.9;
/// How many are out at once, and the gap between refills. Set against the map
/// area so a roaming run crosses one every few seconds: sparse enough that
/// finding one means something, dense enough that there is always a next.
pub const MAX: usize = 20;
pub const RESPAWN: f32 = 1.2;
/// Base value of a core before the combo multiplier.
pub const CORE_VALUE: u32 = 60;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// Always worth money. The bread-and-butter of a detour.
    Core,
    /// A few seconds above the speed cap with the drag turned down. Turns the
    /// map into a racetrack for as long as it lasts.
    Surge,
    /// One hit point. Only ever spawns while MOVA is actually hurt.
    Heal,
}

impl Kind {
    #[inline]
    pub fn glyph(self) -> char {
        match self {
            Kind::Core => 'o',
            Kind::Surge => '*',
            Kind::Heal => '+',
        }
    }

    /// Three cores for every one healer: cores are the reason to roam, heal is
    /// the safety net that makes roaming survivable.
    #[inline]
    pub fn weight(self) -> u32 {
        match self {
            Kind::Core => 6,
            Kind::Surge => 2,
            Kind::Heal => 3,
        }
    }
}

#[derive(Clone, Copy)]
pub struct Pickup {
    pub kind: Kind,
    /// Where it sits now, including the bob and any magnet pull.
    pub x: f32,
    pub y: f32,
    /// Home position. The bob is measured from here so a magnet pull slides
    /// the pickup and it drifts back, rather than teleporting each frame.
    pub hx: f32,
    pub hy: f32,
    pub phase: f32,
    /// Counts up since it appeared, for the fade-in.
    pub age: f32,
}

impl Pickup {
    pub fn new(kind: Kind, x: f32, y: f32, phase: f32) -> Self {
        Self {
            kind,
            x,
            y,
            hx: x,
            hy: y,
            phase,
            age: 0.0,
        }
    }

    /// The bob offset for this frame, positive when it rides high.
    #[inline]
    pub fn lift(&self, time: f32) -> f32 {
        (time * 2.0 + self.phase).sin() * BOB
    }

    /// How bright it should read: 0 the instant it appears, 1 after a moment.
    #[inline]
    pub fn settle(&self) -> f32 {
        (self.age / 0.5).clamp(0.0, 1.0)
    }
}
