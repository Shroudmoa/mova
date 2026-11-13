//! Special powers. The roster lives here; the effects live with the rules.
//!
//! A slot is just an index into [`CHARGE`], [`NAMES`] and [`KEYS`], so adding the
//! next one is a line in each list plus an effect somewhere. The [`Power`] itself
//! only tracks charge; which slots exist is decided by the meta layer through the
//! unlock bitmask, so a run starts with the rack it has earned.

/// Slots on the rack. A power is just an index into [`CHARGE`], [`NAMES`] and
/// [`KEYS`], so adding the next one is a line in each list.
pub const COUNT: usize = 4;
pub const VIRUS: usize = 0;
pub const OVERCHARGE: usize = 1;
pub const CHAIN: usize = 2;
pub const MAGNET: usize = 3;

/// Seconds between firings, per slot.
pub const CHARGE: [f32; COUNT] = [15.0, 20.0, 12.0, 14.0];
pub const NAMES: [&str; COUNT] = ["VIRUS", "OVERCHARGE", "CHAIN", "MAGNET"];
pub const KEYS: [char; COUNT] = ['E', 'B', 'C', 'N'];

/// How far a virus burst reaches, before an enemy's own radius.
pub const VIRUS_REACH: f32 = 28.0;
/// What a virus kill pays, before the enemy's own multiplier.
pub const VIRUS_MULT: f32 = 2.4;

/// How far a chain lightning bolt arcs, before the next thing it finds. Enough
/// to skip a lane, short enough that a crowd answers to one press.
pub const CHAIN_REACH: f32 = 18.0;
/// How many times a single bolt keeps jumping after it lands. A kill at full
/// speed deserves a chain; a kill in open ground should not become a nuke.
pub const CHAIN_HOPS: usize = 3;
/// What one chain hop costs, before the enemy's own multiplier. A hop is a
/// bullet pair, so it should be the finishing blow on the grunts the gun needs
/// two bullets to put down.
pub const CHAIN_DMG: f32 = 2.0;

/// How long a magnet holds pickups within reach.
pub const MAGNET_TIME: f32 = 6.0;

pub struct Power {
    charge: [f32; COUNT],
    unlocks: u32,
}

impl Default for Power {
    fn default() -> Self {
        // A fresh player owns the two base slots and nothing else.
        Self::new((1 << VIRUS) | (1 << OVERCHARGE))
    }
}

impl Power {
    /// The rack the player owns. Bit `i` of `unlocks` is slot `i`; a slot whose
    /// bit is clear does not exist yet, which is what a locked power looks like
    /// to the player: the key is simply not theirs.
    pub fn new(unlocks: u32) -> Self {
        Self {
            charge: CHARGE,
            unlocks,
        }
    }

    #[inline]
    pub fn ready(&self, slot: usize) -> bool {
        // The base two are the player's from the start; CHAIN and MAGNET arrive
        // through the shop and live in the save's unlock bitmask.
        let unlocked = crate::meta::power_unlocked(slot, self.unlocks);
        unlocked && self.charge[slot] >= CHARGE[slot]
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

    /// Spends one slot's charge. Returns false while it is still filling, and
    /// for a slot the player has not unlocked yet.
    pub fn fire(&mut self, slot: usize) -> bool {
        if !self.ready(slot) {
            return false;
        }
        self.charge[slot] = 0.0;
        true
    }
}
