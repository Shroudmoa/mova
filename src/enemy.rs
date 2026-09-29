//! Enemies. Three shapes, three jobs: close the distance, chase it down, and
//! refuse to move at all.

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
}

impl Kind {
    #[inline]
    pub fn hp(self) -> u8 {
        match self {
            Kind::Grunt => 2,
            Kind::Runner => 1,
            Kind::Brute => 6,
        }
    }

    #[inline]
    pub fn r(self) -> f32 {
        match self {
            Kind::Grunt => RADIUS,
            Kind::Runner => 0.85,
            Kind::Brute => 2.0,
        }
    }

    /// Multiplier on the wave's base speed.
    #[inline]
    pub fn speed(self) -> f32 {
        match self {
            Kind::Grunt => 1.0,
            Kind::Runner => 1.15,
            Kind::Brute => 0.58,
        }
    }

    #[inline]
    pub fn glyph(self) -> char {
        match self {
            Kind::Grunt => 'x',
            // The runner points where it is going, which is most of the tell.
            Kind::Runner => '>',
            Kind::Brute => 'O',
        }
    }

    /// Score multiplier. A brute is meant to feel like a prize.
    #[inline]
    pub fn mult(self) -> f32 {
        match self {
            Kind::Grunt => 1.0,
            Kind::Runner => 1.35,
            Kind::Brute => 3.6,
        }
    }

    /// Heavy things ignore a ram entirely. Momentum is not a universal answer,
    /// and the player should learn which enemies to stop running at.
    #[inline]
    pub fn heavy(self) -> bool {
        matches!(self, Kind::Brute)
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
            dead: false,
        }
    }

    pub fn update(&mut self, dt: f32, px: f32, py: f32, time: f32, solids: &SolidGrid) {
        self.hit = (self.hit - dt).max(0.0);
        let (dx, dy) = (px - self.x, py - self.y);
        let len = (dx * dx + dy * dy).sqrt().max(0.0001);
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
            // Brutes just walk at you. The weight is the point.
            Kind::Brute => {}
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

        let step = self.spd * rush * dt;
        self.x += ux * step;
        self.y += uy * step;

        if let Some(n) = solids.push_out(&mut self.x, &mut self.y, self.r) {
            self.slide = 0.45;
            self.slide_n = n;
        }
    }
}
