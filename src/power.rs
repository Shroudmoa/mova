//! Special powers. One on the rack so far, built to hold the rest.

/// Seconds between firings.
pub const CHARGE: f32 = 15.0;
/// How far a burst reaches, before an enemy's own radius. Wider than the view
/// on a small terminal, so the power is a purge rather than an aimed shot.
pub const REACH: f32 = 28.0;
/// What a power kill pays, before the enemy's own multiplier.
pub const MULT: f32 = 2.4;
/// The first of the roster.
pub const NAME: &str = "VIRUS";

/// Charge state of the equipped power. Runs start ready, so the first press
/// shows what the power does instead of asking the player to wait for it.
pub struct Power {
    charge: f32,
}

impl Default for Power {
    fn default() -> Self {
        Self::new()
    }
}

impl Power {
    pub fn new() -> Self {
        Self { charge: CHARGE }
    }

    #[inline]
    pub fn ready(&self) -> bool {
        self.charge >= CHARGE
    }

    /// 0..1, for the HUD.
    #[inline]
    pub fn ratio(&self) -> f32 {
        (self.charge / CHARGE).clamp(0.0, 1.0)
    }

    pub fn update(&mut self, dt: f32) {
        self.charge = (self.charge + dt).min(CHARGE);
    }

    /// Spends the charge. Returns false while it is still filling.
    pub fn fire(&mut self) -> bool {
        if !self.ready() {
            return false;
        }
        self.charge = 0.0;
        true
    }
}
