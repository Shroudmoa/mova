//! MOVA. A point with momentum — and enough of it to break things.

use crate::game::Arena;
use crate::input::Input;
use crate::score::Fx;
use crate::solid::SolidGrid;

pub const ACCEL: f32 = 320.0;
/// Per-second velocity bleed.
///
/// v3 cut this from 3.6 to 0.5, and that one number is most of what changed
/// about how the game plays. Speed used to leak away faster than most players
/// could replace it, so the cap was a thing you arrived at rather than a thing
/// you held, and every corner cost you the run you had built. Now it leaks
/// slowly enough that speed is a resource: you spend it on direction changes,
/// and going straight is how you keep it.
pub const DRAG: f32 = 0.5;
/// The ceiling. Higher than v2's 26 because the drag no longer decides how fast
/// you can go — this does, and a cap you can actually reach in a couple of
/// seconds is the difference between chasing it and waiting for it.
pub const MAX_SPEED: f32 = 44.0;
pub const RADIUS: f32 = 1.0;

pub const DASH_SPEED: f32 = 190.0;
pub const DASH_TIME: f32 = 0.16;
pub const DASH_CD: f32 = 0.52;
/// Grace period after a dash during which nothing can touch MOVA.
pub const DASH_IFRAMES: f32 = 0.06;
/// Invulnerability after eating a hit.
pub const HIT_IFRAMES: f32 = 1.0;

/// A bounce leaves MOVA over the normal cap for a moment, which is what makes a
/// chain of them feel like one continuous shove instead of separate hits.
pub const BOOST_TIME: f32 = 0.55;
pub const BOOST_CAP: f32 = 1.28;

/// What a bounce off a wall keeps of the speed you came in with. Below one, so
/// geometry costs you a little; the bounce itself is the reward, not the
/// preservation.
const RICOCHET_KEEP: f32 = 0.97;
/// And what it adds on top. A wall you bounce off clean should feel like it
/// gave you something.
const RICOCHET_GAIN: f32 = 1.06;

/// How long a bounce keeps asking the world for a nova. A single step's worth and
/// no more: the game layer consumes it as an event rather than as a state, so two
/// bounces inside one step are one nova — which is the right answer, since there
/// was one reflection.
const BOUNCE_SIGNAL: f32 = 1.0 / 120.0;

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
    /// Set for one step by a reflection, and consumed by the game layer as a
    /// nova. Not a countdown in the usual sense — it is an event flag, and the
    /// only reason it is a field rather than a return value is that the bounce
    /// happens a layer below the one that owns the enemies it is about to hit.
    pub bounce_t: f32,
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
            bounce_t: 0.0,
            flash: 0.0,
            surge: 0.0,
            alive: true,
            ember_gap: 0.0,
        }
    }

    /// Seconds a Surge pickup lasts, and how much extra headroom it buys. With
    /// the drag already low, the extra cap is what a surge is for — the ceiling
    /// moves, not the handling.
    pub const SURGE_TIME: f32 = 8.0;
    pub const SURGE_CAP: f32 = 1.45;
    /// Drag is scaled by this while surging, so a surge run holds its line
    /// rather than merely reaching a higher number once.
    const SURGE_DRAG: f32 = 0.25;
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
        // Decayed before the move, so a bounce raised later this step survives to
        // be seen and one raised last step does not.
        self.bounce_t = (self.bounce_t - dt).max(0.0);
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

        let (ox, oy) = (self.x, self.y);
        let (dx, dy) = (self.vx * dt, self.vy * dt);
        self.x += dx;
        self.y += dy;
        // No wall. The arena wraps, so leaving one side is arriving on the other
        // and nothing about it needs resolving.
        (self.x, self.y) = arena.wrap(self.x, self.y);

        // A wall bounces. v2 bled the speed and called that a pillar; here the
        // surface gives the speed straight back, which turns the furniture from
        // something that punishes you for running into it into the main way to
        // change direction without losing your run. Reflecting off the normal
        // means a glancing hit barely turns you and a square one flips you,
        // which is the whole skill of threading a corridor at speed.
        if let Some((nx, ny)) = solids.push_out(&mut self.x, &mut self.y, RADIUS) {
            let d = self.vx * nx + self.vy * ny;
            if d < 0.0 {
                // Only the into-the-wall component is reflected and rescaled; the
                // along-wall component is untouched, so a bounce skims.
                self.vx -= 2.0 * d * nx;
                self.vy -= 2.0 * d * ny;
                let s = self.speed();
                if s > 0.001 {
                    let k = (s * RICOCHET_KEEP * RICOCHET_GAIN).min(self.cap());
                    self.vx = self.vx / s * k;
                    self.vy = self.vy / s * k;
                }
                self.boost = self.boost.max(BOOST_TIME);
                self.bounce_t = BOUNCE_SIGNAL;
                // A bounce does not cancel a dash — you committed to the line,
                // and coming off the wall at dash speed is the point.
            }
            fx.burst(self.x, self.y, 6.0);
        }

        // Moving fast leaves a faint wake even without dashing.
        if self.speed_ratio() > 0.78 {
            fx.particle(self.x, self.y, -self.vx * 0.12, -self.vy * 0.12, 0.14, 1);
        }
        // Ground actually covered, measured from where the step started rather
        // than from how far the velocity said to go.
        //
        // v2 passed `|v| * dt` straight through, which counts ground MOVA never
        // reaches whenever something stops him: pinned against a wall at the cap
        // he covers none at all but was handed the full step every frame, so he
        // stood in one spot throwing an unbounded tail of flames. Measuring the
        // move that happened also makes a bounce count for the ground it ate
        // rather than the ground he meant to cross, which is what a trail is.
        let moved = (Arena::delta_axis(ox, self.x, arena.hx).powi(2)
            + Arena::delta_axis(oy, self.y, arena.hy).powi(2))
        .sqrt();
        self.burn(fx, moved);
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

    /// A ram connects: MOVA keeps the momentum and gains a good deal more.
    /// Generous on purpose — a chain of kills should end faster than it began,
    /// so the reward for committing is that the run gets easier.
    pub fn kick(&mut self) {
        self.boost = BOOST_TIME;
        let s = self.speed();
        if s > 0.001 {
            let k = (s * 1.22).min(self.cap());
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

        // Shove MOVA clear so the same enemy cannot chain-hit. Scaled off the cap
        // rather than written as a number, so it stays a meaningful recovery
        // when the cap moves.
        let (dx, dy) = (self.x - from_x, self.y - from_y);
        let len = (dx * dx + dy * dy).sqrt().max(0.001);
        let shove = self.cap() * 0.85;
        self.vx = dx / len * shove;
        self.vy = dy / len * shove;

        if self.hp == 0 {
            self.alive = false;
        }
        true
    }
}
