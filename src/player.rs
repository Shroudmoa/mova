//! MOVA. A point with momentum — and enough of it to break things.

use crate::game::Arena;
use crate::input::Input;
use crate::score::Fx;
use crate::solid::SolidGrid;

pub const ACCEL: f32 = 300.0;
/// Per-second velocity bleed. Low enough that speed is worth chasing.
pub const DRAG: f32 = 3.6;
pub const MAX_SPEED: f32 = 26.0;
pub const RADIUS: f32 = 1.0;

pub const DASH_SPEED: f32 = 118.0;
pub const DASH_TIME: f32 = 0.15;
pub const DASH_CD: f32 = 0.70;
/// Grace period after a dash during which nothing can touch MOVA.
pub const DASH_IFRAMES: f32 = 0.06;
/// Invulnerability after eating a hit.
pub const HIT_IFRAMES: f32 = 1.1;

/// A ram leaves MOVA over the normal cap for a moment, which is what makes a
/// chain of them feel like one continuous shove instead of three separate hits.
pub const BOOST_TIME: f32 = 0.42;
pub const BOOST_CAP: f32 = 1.34;

pub struct Player {
    pub x: f32,
    pub y: f32,
    pub vx: f32,
    pub vy: f32,
    pub hp: u8,
    pub max_hp: u8,

    pub dash_t: f32,
    pub dash_cd: f32,
    pub dash_dx: f32,
    pub dash_dy: f32,
    pub invuln: f32,
    /// Counts down after a ram, holding the speed cap open above normal.
    pub boost: f32,
    /// Counts down after taking a hit; drives the red flash.
    pub flash: f32,
    /// Counts down while a Surge pickup is live. It is the one thing in the
    /// game that raises the ceiling above the normal cap, and the arena turns
    /// into a racetrack while it lasts.
    pub surge: f32,
    pub alive: bool,

    /// Distance still to cover before the next ember is thrown. See
    /// [`Self::EMBER_GAP`].
    ember_gap: f32,
}

impl Default for Player {
    fn default() -> Self {
        Self::new()
    }
}

impl Player {
    pub fn new() -> Self {
        Self {
            x: 0.0,
            y: 0.0,
            vx: 0.0,
            vy: 0.0,
            hp: 3,
            max_hp: 3,
            dash_t: 0.0,
            dash_cd: 0.0,
            dash_dx: 0.0,
            dash_dy: 1.0,
            invuln: 0.0,
            boost: 0.0,
            flash: 0.0,
            surge: 0.0,
            alive: true,
            ember_gap: 0.0,
        }
    }

    /// Seconds a Surge pickup lasts, and how much extra headroom it buys.
    pub const SURGE_TIME: f32 = 7.0;
    pub const SURGE_CAP: f32 = 1.32;
    /// Drag is scaled by this while surging. Reducing it, not the acceleration,
    /// is what makes the top speed something you can actually hold.
    const SURGE_DRAG: f32 = 0.42;
    /// How close to the cap counts as full speed. A hair under, so holding a
    /// direction at the cap counts rather than flickering on float noise.
    pub const BLAZE_EPS: f32 = 0.05;
    /// World units between embers off a burning MOVA. At the cap of twenty-six
    /// that is about twenty embers a second, and with the lifetime below it
    /// keeps five or so alive: a tail that follows MOVA, rather than a smear
    /// sitting where the run started.
    pub const EMBER_GAP: f32 = 1.3;

    #[inline]
    pub fn surging(&self) -> bool {
        self.surge > 0.0
    }

    /// True once MOVA is travelling at the cap. This is the state the whole
    /// speed game is built around: contact stops landing, and MOVA burns.
    #[inline]
    pub fn blazing(&self) -> bool {
        self.speed() >= self.cap() - Self::BLAZE_EPS
    }

    #[inline]
    pub fn speed(&self) -> f32 {
        (self.vx * self.vx + self.vy * self.vy).sqrt()
    }

    /// The ceiling MOVA is held to right now.
    #[inline]
    pub fn cap(&self) -> f32 {
        let mut c = MAX_SPEED;
        if self.boost > 0.0 {
            c *= BOOST_CAP;
        }
        if self.surge > 0.0 {
            c *= Self::SURGE_CAP;
        }
        c
    }

    /// Current speed as a 0..1 fraction of the cap. Drives ram, score and glow.
    #[inline]
    pub fn speed_ratio(&self) -> f32 {
        (self.speed() / self.cap()).min(1.0)
    }

    /// True while the speed is high enough to shatter an enemy on contact.
    #[inline]
    pub fn armed(&self) -> bool {
        self.speed_ratio() >= crate::game::RAM_AT
    }

    #[inline]
    pub fn dashing(&self) -> bool {
        self.dash_t > 0.0
    }

    #[inline]
    pub fn dash_ready(&self) -> bool {
        self.dash_cd <= 0.0
    }

    pub fn update(
        &mut self,
        dt: f32,
        input: &Input,
        arena: &Arena,
        solids: &SolidGrid,
        fx: &mut Fx,
    ) {
        self.dash_cd = (self.dash_cd - dt).max(0.0);
        self.invuln = (self.invuln - dt).max(0.0);
        self.flash = (self.flash - dt).max(0.0);
        self.boost = (self.boost - dt).max(0.0);
        self.surge = (self.surge - dt).max(0.0);

        let (ix, iy) = input.axis();

        if self.dashing() {
            self.dash_t -= dt;
            self.vx = self.dash_dx * DASH_SPEED;
            self.vy = self.dash_dy * DASH_SPEED;
            // Trail: a short line of dots directly behind the dash direction.
            fx.particle(
                self.x,
                self.y,
                -self.dash_dx * 12.0,
                -self.dash_dy * 12.0,
                0.2,
                1,
            );
        } else {
            if input.dash && self.dash_ready() {
                // Dash along the stick, falling back to current travel.
                let (dx, dy) = if ix != 0.0 || iy != 0.0 {
                    (ix, iy)
                } else {
                    let s = self.speed();
                    if s > 0.5 {
                        (self.vx / s, self.vy / s)
                    } else {
                        (0.0, 1.0)
                    }
                };
                self.dash_dx = dx;
                self.dash_dy = dy;
                self.dash_t = DASH_TIME;
                self.dash_cd = DASH_CD;
                self.invuln = self.invuln.max(DASH_IFRAMES);
                fx.particle(self.x, self.y, 0.0, 0.0, 0.18, 1);
            }

            self.vx += ix * ACCEL * dt;
            self.vy += iy * ACCEL * dt;

            let drag = if self.surge > 0.0 {
                DRAG * Self::SURGE_DRAG
            } else {
                DRAG
            };
            let bleed = (1.0 - drag * dt).max(0.0);
            self.vx *= bleed;
            self.vy *= bleed;

            // Cap keeps the game readable; the overshoot is preserved as
            // direction so a hard turn still feels sharp.
            let s = self.speed();
            if s > self.cap() {
                let k = self.cap() / s;
                self.vx *= k;
                self.vy *= k;
            }
        }

        let (dx, dy) = (self.vx * dt, self.vy * dt);
        self.x += dx;
        self.y += dy;
        arena.clamp(&mut self.x, &mut self.y);

        // A pillar eats momentum instead of stopping it dead. Killing the dash
        // outright would feel like a bug on a corridor run; bleeding the speed
        // means a pillar is a place you slow down, not a wall you bounce off.
        if solids.push_out(&mut self.x, &mut self.y, RADIUS).is_some() {
            self.vx *= 0.62;
            self.vy *= 0.62;
            self.dash_t = 0.0;
        }

        // Moving fast leaves a faint wake even without dashing.
        if self.speed_ratio() > 0.78 {
            fx.particle(self.x, self.y, -self.vx * 0.12, -self.vy * 0.12, 0.14, 1);
        }
        self.burn(fx, (dx * dx + dy * dy).sqrt());
    }

    /// Throws embers while MOVA is at the cap. This is the visible half of
    /// [`Self::blazing`]: the invulnerability is worth having precisely because
    /// you can see it switch on, and a burning body is that signal from across
    /// the screen.
    ///
    /// `moved` is the distance covered this step. Embers are spaced by ground
    /// rather than by step, so the tail is a measure of how far MOVA has come
    /// instead of a measure of the frame clock — and throwing one every step
    /// would leave seventy of them on screen and smear the flame into a cloud.
    fn burn(&mut self, fx: &mut Fx, moved: f32) {
        if !self.blazing() {
            return;
        }
        // `ember_gap` counts down the ground still to cover before the next
        // ember, and the overshoot is carried rather than dropped. Snapping back
        // to a whole gap throws away up to one step of travel each time, which
        // is a fifth of the gap at the step the game runs — so the tail would
        // quietly thin with every step the loop happened to take. Draining the
        // counter covers the other extreme: a long step can be several gaps at
        // once, and it owes that many embers, not one. Either way the spacing is
        // exactly `EMBER_GAP` units of ground at any step size.
        self.ember_gap -= moved;
        if self.ember_gap > 0.0 {
            return;
        }
        let due = 1 + (-self.ember_gap / Self::EMBER_GAP) as usize;
        self.ember_gap += Self::EMBER_GAP * due as f32;

        let s = self.speed().max(0.001);
        let (ux, uy) = (self.vx / s, self.vy / s);
        for i in 0..due {
            // Alternating off the player's own position rather than a random
            // number, so the tail flicks from side to side as MOVA covers
            // ground instead of sitting as a fixed pair. Indexed by which ember
            // this is of the batch, so a batch of several keeps alternating
            // rather than printing the same pair twice.
            let flick = if (self.x * 0.7 + self.y * 0.3 + i as f32 * 1.7).sin() > 0.0 {
                1.0
            } else {
                -1.0
            };
            for side in [-1.0f32, 1.0] {
                // Thrown backward off the direction of travel, spread to either
                // side of it. The backward push is small: an ember that keeps its
                // own velocity leaves a trail in the air, which is not what a
                // body on fire looks like.
                fx.ember(
                    self.x - ux * 0.7 - uy * side * flick * 0.7,
                    self.y - uy * 0.7 + ux * side * flick * 0.7,
                    -ux * 1.6 - uy * side * 0.9,
                    -uy * 1.6 + ux * side * 0.9,
                );
            }
        }
    }

    /// A Surge pickup connects.
    pub fn surge(&mut self) {
        self.surge = Self::SURGE_TIME;
    }

    /// A ram connects: MOVA keeps the momentum and gains a little more.
    pub fn kick(&mut self) {
        self.boost = BOOST_TIME;
        let s = self.speed();
        if s > 0.001 {
            let k = (s * 1.14).min(self.cap());
            self.vx = self.vx / s * k;
            self.vy = self.vy / s * k;
        }
    }

    /// Returns true if the hit actually landed.
    pub fn damage(&mut self, from_x: f32, from_y: f32) -> bool {
        if self.invuln > 0.0 || !self.alive {
            return false;
        }
        // Full speed is past contact. The same bargain as a ram, one tier up:
        // hold the line and nothing lands. Making it a real, visible state is
        // what turns the speed cap into a thing to chase rather than a number
        // on a gauge.
        if self.blazing() {
            return false;
        }
        self.hp = self.hp.saturating_sub(1);
        self.invuln = HIT_IFRAMES;
        self.flash = 0.22;
        self.dash_t = 0.0;
        // The shove is meant to be painful: you keep most of it, but the boost
        // does not, so a ram chain cannot be used to shrug off contact.
        self.boost = 0.0;

        // Shove MOVA clear so the same enemy cannot chain-hit.
        let (dx, dy) = (self.x - from_x, self.y - from_y);
        let len = (dx * dx + dy * dy).sqrt().max(0.001);
        self.vx = dx / len * 34.0;
        self.vy = dy / len * 34.0;

        if self.hp == 0 {
            self.alive = false;
        }
        true
    }
}
