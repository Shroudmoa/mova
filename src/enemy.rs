//! Enemies. Six shapes, six jobs: close the distance, chase it down, refuse to
//! move at all, commit to the run, hold the line when the waves get serious, and
//! — once the run earns it — commit harder still.

use crate::game::Arena;
use crate::solid::SolidGrid;

pub const RADIUS: f32 = 1.0;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// Weaves in at a walk. Two hits, which makes the gun the bottleneck rather
    /// than the spawner, so the screen can actually fill up.
    Grunt,
    /// One hit, quicker than you, and it sprints in bursts. Cheap, so it comes
    /// in numbers and it is what makes the open floor feel unsafe.
    Runner,
    /// Too heavy to shove. Only the gun moves it. This is the reason the gun
    /// exists in a game about running into things, so it has to matter.
    Brute,
    /// The wave's first mind of scheme. It does not weave and it does not orbit:
    /// it closes, and once it closes it *commits*, which is a different thing
    /// from timing a burst off a timer.
    Harrier,
    /// The late-wave siege. Slow to close, hard to move, worth more to kill.
    /// It does not scheme — it holds — and the threat is that holding is the
    /// plan rather than the accident.
    Warden,
    /// The mid-run siege. Slow to close even by the warden's measure, then
    /// commits with more haste than a harrier should, which is the whole threat
    /// of a boss wearing the enemy's vocabulary.
    Harbinger,
}

impl Kind {
    #[inline]
    pub fn hp(self) -> u8 {
        match self {
            Kind::Grunt => 2,
            Kind::Runner => 1,
            Kind::Brute => 6,
            Kind::Harrier => 2,
            Kind::Warden => 14,
            Kind::Harbinger => 30,
        }
    }

    #[inline]
    pub fn r(self) -> f32 {
        match self {
            Kind::Grunt => RADIUS,
            Kind::Runner => 0.85,
            Kind::Brute => 2.0,
            Kind::Harrier => 0.9,
            Kind::Warden => 2.2,
            Kind::Harbinger => 3.0,
        }
    }

    /// Multiplier on the wave's base speed.
    #[inline]
    pub fn speed(self) -> f32 {
        match self {
            Kind::Grunt => 1.0,
            Kind::Runner => 1.15,
            Kind::Brute => 0.58,
            Kind::Harrier => 1.45,
            Kind::Warden => 0.5,
            Kind::Harbinger => 0.75,
        }
    }

    #[inline]
    pub fn glyph(self) -> char {
        match self {
            Kind::Grunt => 'x',
            // The runner points where it is going, which is most of the tell.
            Kind::Runner => '>',
            Kind::Brute => 'O',
            // The harrier reads as an incoming decision: a small mark that is
            // about to commit somewhere.
            Kind::Harrier => '!',
            // The warden reads as a wall that happens to be alive: heavy, and
            // therefore not something you shove — something you burn down.
            Kind::Warden => 'W',
            Kind::Harbinger => 'H',
        }
    }

    /// Score multiplier. A brute is meant to feel like a prize.
    #[inline]
    pub fn mult(self) -> f32 {
        match self {
            Kind::Grunt => 1.0,
            Kind::Runner => 1.35,
            Kind::Brute => 3.6,
            Kind::Harrier => 1.7,
            Kind::Warden => 4.5,
            Kind::Harbinger => 6.0,
        }
    }

    /// Heavy things ignore a ram entirely. Momentum is not a universal answer,
    /// and the player should learn which enemies to stop running at. The warden
    /// is heavy by trade, which is the whole point of the elite.
    #[inline]
    pub fn heavy(self) -> bool {
        matches!(self, Kind::Brute | Kind::Warden | Kind::Harbinger)
    }
}

pub struct Enemy {
    pub x: f32,
    pub y: f32,
    pub kind: Kind,
    pub r: f32,
    pub hp: u8,
    pub spd: f32,
    /// Phase offset for the weave and the sprint timer, so a crowd never moves
    /// in lockstep.
    pub wob: f32,
    /// Which way round a pillar this one prefers to go. Fixed per enemy so a
    /// swarm fans out across a block instead of queueing behind it.
    pub turn: f32,
    /// Counts down after touching a block; blends the heading toward the
    /// surface tangent while it does.
    pub slide: f32,
    pub slide_n: (f32, f32),
    /// Runner sprint state: time until the next burst, and burst remaining.
    pub burst_t: f32,
    pub rush: f32,
    /// Counts down after a bullet connects; drives the white-hot flash.
    pub hit: f32,
    /// Cleared when MOVA brushes past: stops near-miss points from repeating.
    pub grazed: bool,
    /// Counts down after MOVA's wake cuts this one. An enemy has to be able to
    /// be standing in a *different* part of a trail a moment later, so this
    /// cannot be a latch — but it cannot be nothing either, or a grunt that
    /// clips the tail on the way past dies in a single frame and the trail reads
    /// as a beam rather than as a line.
    pub wake_cd: f32,
    pub dead: bool,
}

impl Enemy {
    pub fn new(x: f32, y: f32, kind: Kind, spd: f32, wob: f32, turn: f32) -> Self {
        Self {
            x,
            y,
            r: kind.r(),
            kind,
            hp: kind.hp(),
            spd,
            wob,
            turn,
            slide: 0.0,
            slide_n: (0.0, 0.0),
            burst_t: 0.0,
            rush: 0.0,
            hit: 0.0,
            grazed: false,
            wake_cd: 0.0,
            dead: false,
        }
    }

    pub fn update(
        &mut self,
        dt: f32,
        px: f32,
        py: f32,
        time: f32,
        arena: &Arena,
        solids: &SolidGrid,
    ) {
        self.hit = (self.hit - dt).max(0.0);
        // Chased across the seam, so a swarm that has MOVA cornered against the
        // wrap does not simply give up and walk away from him.
        let dist = arena.distance(self.x, self.y, px, py).max(0.0001);
        let (dx, dy) = (
            Arena::delta_axis(self.x, px, arena.hx),
            Arena::delta_axis(self.y, py, arena.hy),
        );
        let len = dist;
        let mut ux = dx / len;
        let mut uy = dy / len;
        let mut rush = 1.0;

        match self.kind {
            Kind::Grunt => {
                // A gentle weave. Close enough to aim at, loose enough that a
                // whole crowd does not stack into a single line.
                let a = uy.atan2(ux) + (time * 2.4 + self.wob).sin() * 0.30;
                ux = a.cos();
                uy = a.sin();
                // Close the last bit faster once inside the danger ring, so
                // contact happens instead of enemies orbiting MOVA forever.
                if len < 6.0 {
                    rush = 1.35;
                }
            }
            Kind::Runner => {
                self.burst_t -= dt;
                if self.burst_t <= 0.0 {
                    self.burst_t = 1.5 + self.wob * 0.25;
                    self.rush = 0.5;
                }
                self.rush = (self.rush - dt).max(0.0);
                if self.rush > 0.0 {
                    rush = 2.2;
                }
            }
            // Brutes just walk at you. The weight is the point. The warden is
            // the same creed with a slower cadence and a thicker hull; the
            // harbinger is that creed with a longer fuse and a harder commit.
            Kind::Brute | Kind::Warden => {}
            Kind::Harbinger => {
                self.burst_t -= dt;
                if self.burst_t <= 0.0 {
                    self.burst_t = 1.2 + self.wob * 0.3;
                    self.rush = 0.5;
                }
                self.rush = (self.rush - dt).max(0.0);
                if self.rush > 0.0 {
                    rush = 3.5;
                }
                // Proximity, not a clock — the same as the harrier, only with a
                // longer fuse and a harder commit, because the boss is the thing
                // the player has been learning to fear.
                if len < 14.0 {
                    self.burst_t = 0.0;
                    self.rush = 3.5;
                }
            }
            // The harrier's trick is that its burst is a commitment. It does not
            // weave into the charge and it does not decide halfway through: the
            // moment it closes within burst range it plants itself on a heading,
            // which is a different gamble from the runner's free-running timer and
            // one the player reads off the direction of travel rather than off a
            // countdown.
            Kind::Harrier => {
                self.burst_t -= dt;
                if self.burst_t <= 0.0 {
                    self.burst_t = 0.8 + self.wob * 0.3;
                    self.rush = 0.5;
                }
                self.rush = (self.rush - dt).max(0.0);
                if self.rush > 0.0 {
                    rush = 2.6;
                }
                // Proximity, not a clock: closing in arms the charge, and while
                // it holds the harrier holds its heading.
                if len < 9.0 {
                    self.burst_t = 0.0;
                    self.rush = 2.6;
                }
            }
        }

        // Steering: blend the heading toward the tangent of whatever we just
        // scraped. Without this a swarm piles into a pillar and mills about on
        // the near side, which reads as broken rather than alive.
        self.slide = (self.slide - dt).max(0.0);
        if self.slide > 0.0 && self.slide_n != (0.0, 0.0) {
            let (nx, ny) = self.slide_n;
            let (tx, ty) = (-ny * self.turn, nx * self.turn);
            let w = (self.slide * 3.0).min(0.9);
            ux = ux * (1.0 - w) + tx * w;
            uy = uy * (1.0 - w) + ty * w;
            let l = (ux * ux + uy * uy).sqrt().max(0.0001);
            ux /= l;
            uy /= l;
        }

        // The warden is a siege engine, not a sprinter. It walks slow and hits
        // like one, and once its hull starts burning it stops pretending
        // otherwise: the enrage is read off the hull rather than latched, so it
        // is a threshold on the hp and not a state the spawner has to remember
        // to set. Same trick as the chain's hop count — derive, don't store.
        let spd = if (self.kind == Kind::Warden || self.kind == Kind::Harbinger)
            && self.hp * 2 < self.kind.hp()
        {
            self.spd * 1.8
        } else {
            self.spd
        };
        let step = spd * rush * dt;
        self.x += ux * step;
        self.y += uy * step;

        if let Some(n) = solids.push_out(&mut self.x, &mut self.y, self.r) {
            self.slide = 0.45;
            self.slide_n = n;
        }
        (self.x, self.y) = arena.wrap(self.x, self.y);
    }
}
