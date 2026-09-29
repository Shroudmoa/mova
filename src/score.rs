//! Score, combo, and the tiny kill feedback effects.

pub const KILL_BASE: u32 = 45;
pub const NEAR_MISS: u32 = 20;
pub const COMBO_CAP: u32 = 10;
/// Seconds a combo survives without a kill before it starts bleeding.
const COMBO_GRACE: f32 = 2.6;
/// Seconds between decay steps once the grace period is up.
const COMBO_DECAY: f32 = 0.45;
/// Speed ratio above which a kill is labelled and scored as a fast kill.
pub const FAST_AT: f32 = 0.55;

pub struct Score {
    pub points: u32,
    pub combo: u32,
    pub best_combo: u32,
    timer: f32,
}

impl Default for Score {
    fn default() -> Self {
        Self::new()
    }
}

impl Score {
    pub fn new() -> Self {
        Self {
            points: 0,
            combo: 0,
            best_combo: 0,
            timer: 0.0,
        }
    }

    /// How long the combo may breathe between kills. It tightens as the
    /// combo climbs, so a big multiplier has to be earned every few seconds.
    fn window(&self) -> f32 {
        (COMBO_GRACE - self.combo as f32 * 0.14).max(1.1)
    }

    /// As `kill_at`, for a kill with an extra multiplier because it cost speed
    /// to make. `mult` is the per-kind weight.
    pub fn kill_at(&mut self, speed_ratio: f32, mult: f32) -> (u32, bool) {
        let fast = speed_ratio >= FAST_AT;
        // Capped so a long combo still reads as a number, not an exponent.
        let speed_mult = 1.0 + speed_ratio * 0.5;
        let combo = self.combo.max(1) as f32;
        let pts = ((KILL_BASE as f32) * speed_mult * combo * mult).round() as u32;

        self.points += pts;
        self.combo = (self.combo + 1).min(COMBO_CAP);
        self.best_combo = self.best_combo.max(self.combo);
        self.timer = self.window();
        (pts, fast)
    }

    pub fn near_miss(&mut self) -> u32 {
        let pts = NEAR_MISS * self.combo.max(1);
        self.points += pts;
        self.timer = self.window();
        pts
    }

    /// A pickup core. Scored off the current combo rather than the kill path,
    /// and it refreshes the combo timer without advancing the multiplier, so
    /// grabbing one mid-chain extends the chain instead of costing it.
    pub fn core(&mut self, base: u32) -> u32 {
        let pts = base * self.combo.max(1);
        self.points += pts;
        self.timer = self.timer.max(self.window());
        pts
    }

    pub fn break_combo(&mut self) {
        self.combo = 0;
        self.timer = 0.0;
    }

    pub fn update(&mut self, dt: f32) {
        if self.combo == 0 {
            return;
        }
        self.timer -= dt;
        if self.timer <= 0.0 {
            self.timer = COMBO_DECAY;
            self.combo -= 1;
        }
    }

    /// Charge left before the combo starts bleeding, normalised to 0..1.
    pub fn charge(&self) -> f32 {
        (self.timer / self.window()).clamp(0.0, 1.0)
    }
}

/// What a score popup says.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum PopKind {
    Kill,
    Fast,
    Near,
    Ram,
    /// A pickup. The word is the reward; the number is the payout.
    Core,
    Surge,
    Heal,
    /// A sector MOVA had not stood in before.
    Found,
}

pub struct Pop {
    pub x: f32,
    pub y: f32,
    pub value: u32,
    pub kind: PopKind,
    pub life: f32,
}

pub struct Particle {
    pub x: f32,
    pub y: f32,
    pub vx: f32,
    pub vy: f32,
    pub life: f32,
    pub max_life: f32,
    /// 0 = kill debris, 1 = dash trail, 2 = spark from taking a hit,
    /// 3 = ember off a MOVA at full speed.
    pub kind: u8,
}

/// A word that rises off the floor: a pickup name, or a sector MOVA just
/// walked into for the first time.
pub struct Note {
    pub x: f32,
    pub y: f32,
    pub text: &'static str,
    pub kind: PopKind,
    pub life: f32,
}

/// An expanding circle drawn on the floor.
pub struct Ring {
    pub x: f32,
    pub y: f32,
    pub life: f32,
    pub max_life: f32,
    /// Radius in world units at the moment it dies.
    pub r: f32,
}

impl Ring {
    /// Current radius and how opaque it should read.
    #[inline]
    pub fn extent(&self) -> (f32, f32) {
        let t = 1.0 - (self.life / self.max_life).clamp(0.0, 1.0);
        (self.r * t, 1.0 - t)
    }
}

/// Pre-sized so a normal run never allocates here.
pub struct Fx {
    pub parts: Vec<Particle>,
    pub pops: Vec<Pop>,
    /// Expanding rings. Purely cosmetic, but they are what makes a ram feel
    /// like it landed somewhere rather than just deleting a character.
    pub rings: Vec<Ring>,
    /// Word popups, kept separately from score popups so they can linger and
    /// hold their own line length without fighting the numbers for space.
    pub notes: Vec<Note>,
}

impl Default for Fx {
    fn default() -> Self {
        Self::new()
    }
}

impl Fx {
    pub fn new() -> Self {
        Self {
            parts: Vec::with_capacity(256),
            pops: Vec::with_capacity(16),
            rings: Vec::with_capacity(12),
            notes: Vec::with_capacity(8),
        }
    }

    pub fn clear(&mut self) {
        self.parts.clear();
        self.pops.clear();
        self.rings.clear();
        self.notes.clear();
    }

    /// Spawns a shockwave. `big` is for rams and anything else that should
    /// visibly shove the world around.
    pub fn ring(&mut self, x: f32, y: f32, big: bool) {
        if self.rings.len() >= 12 {
            return;
        }
        let life = if big { 0.42 } else { 0.26 };
        self.rings.push(Ring {
            x,
            y,
            life,
            max_life: life,
            r: if big { 16.0 } else { 8.0 },
        });
    }

    #[inline]
    pub fn particle(&mut self, x: f32, y: f32, vx: f32, vy: f32, life: f32, kind: u8) {
        if self.parts.len() < 512 {
            self.parts.push(Particle {
                x,
                y,
                vx,
                vy,
                life,
                max_life: life,
                kind,
            });
        }
    }

    /// A burning ember. Only the speed cap makes these, so the lifetime is
    /// fixed rather than chosen per ember: long enough to cool through the whole
    /// ramp, short enough that the body stays the brightest thing on screen.
    /// Velocity is passed in rather than assumed, because the only thing that
    /// makes an ember is the direction MOVA was travelling and they should all
    /// be going the same way.
    #[inline]
    pub fn ember(&mut self, x: f32, y: f32, vx: f32, vy: f32) {
        self.particle(x, y, vx, vy, 0.30, 3);
    }

    #[inline]
    pub fn pop(&mut self, x: f32, y: f32, value: u32, kind: PopKind) {
        if self.pops.len() < 16 {
            self.pops.push(Pop {
                x,
                y,
                value,
                kind,
                life: 0.85,
            });
        }
    }

    /// A text popup that is not a score. Used for pickups and sector names,
    /// where the word is the whole message.
    #[inline]
    pub fn note(&mut self, x: f32, y: f32, text: &'static str, kind: PopKind) {
        if self.notes.len() < 8 {
            self.notes.push(Note {
                x,
                y,
                text,
                kind,
                life: 1.5,
            });
        }
    }

    /// Enemies always come apart into the same handful of dots.
    pub fn burst(&mut self, x: f32, y: f32, speed: f32) {
        for i in 0..6 {
            let a = i as f32 * (std::f32::consts::TAU / 6.0) + 0.4;
            let s = speed * (0.55 + 0.45 * ((i * 7) % 5) as f32 / 4.0);
            self.particle(x, y, a.cos() * s, a.sin() * s * 0.55, 0.34, 0);
        }
    }

    pub fn update(&mut self, dt: f32) {
        let mut i = 0;
        while i < self.parts.len() {
            let p = &mut self.parts[i];
            p.life -= dt;
            if p.life <= 0.0 {
                self.parts.swap_remove(i);
                continue;
            }
            // Debris keeps drifting, trail sparks do not. Embers drift a little
            // and then stop, so a tail stays a tail instead of washing backwards
            // across the arena.
            match p.kind {
                1 => {}
                3 => {
                    p.x += p.vx * dt;
                    p.y += p.vy * dt;
                    p.vx *= 1.0 - 6.0 * dt;
                    p.vy *= 1.0 - 6.0 * dt;
                }
                _ => {
                    p.x += p.vx * dt;
                    p.y += p.vy * dt;
                    p.vx *= 1.0 - 5.0 * dt;
                    p.vy *= 1.0 - 5.0 * dt;
                }
            }
            i += 1;
        }

        let mut i = 0;
        while i < self.pops.len() {
            self.pops[i].life -= dt;
            if self.pops[i].life <= 0.0 {
                self.pops.swap_remove(i);
                continue;
            }
            self.pops[i].y += 9.0 * dt;
            i += 1;
        }

        let mut i = 0;
        while i < self.rings.len() {
            self.rings[i].life -= dt;
            if self.rings[i].life <= 0.0 {
                self.rings.swap_remove(i);
            } else {
                i += 1;
            }
        }

        let mut i = 0;
        while i < self.notes.len() {
            self.notes[i].life -= dt;
            if self.notes[i].life <= 0.0 {
                self.notes.swap_remove(i);
                continue;
            }
            // Notes drift up and hold still, unlike popups which fall.
            self.notes[i].y += 3.5 * dt;
            i += 1;
        }
    }
}
