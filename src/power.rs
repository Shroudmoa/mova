//! Special powers. The roster lives here; the effects live with the rules.

/// Slots on the rack. A power is just an index into [`CHARGE`], [`NAMES`] and
/// [`KEYS`], so adding the next one is a line in each list.
pub const COUNT: usize = 2;
pub const VIRUS: usize = 0;
pub const OVERCHARGE: usize = 1;

/// Seconds between firings, per slot.
pub const CHARGE: [f32; COUNT] = [15.0, 20.0];
pub const NAMES: [&str; COUNT] = ["VIRUS", "OVERCHARGE"];
pub const KEYS: [char; COUNT] = ['E', 'B'];

/// How far a virus burst reaches, before an enemy's own radius.
pub const VIRUS_REACH: f32 = 28.0;
/// What a virus kill pays, before the enemy's own multiplier.
pub const VIRUS_MULT: f32 = 2.4;

/// Charge state of every equipped power. Runs start ready, so the first press
/// shows what a power does instead of asking the player to wait for it.
pub struct Power {
    charge: [f32; COUNT],
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
    pub fn ready(&self, slot: usize) -> bool {
        self.charge[slot] >= CHARGE[slot]
    }

    /// 0..1, for the HUD.
    #[inline]
    pub fn ratio(&self, slot: usize) -> f32 {
        (self.charge[slot] / CHARGE[slot]).clamp(0.0, 1.0)
    }

    pub fn update(&mut self, dt: f32) {
        for (charge, full) in self.charge.iter_mut().zip(CHARGE) {
            *charge = (*charge + dt).min(full);
        }
    }

    /// Spends one slot's charge. Returns false while it is still filling.
    pub fn fire(&mut self, slot: usize) -> bool {
        if !self.ready(slot) {
            return false;
        }
        self.charge[slot] = 0.0;
        true
    }
}
