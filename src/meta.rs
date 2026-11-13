//! Meta-progression: the permanent layer that sits between runs.
//!
//! A run earns **cores** — the game's currency — from score and from how the
//! run went. Between runs the player walks a small shop and spends them on
//! permanent things: drive upgrades that move the feel of the speed game, and
//! unlocks for the powers that sit on the rack.
//!
//! The shop is deliberately small. This module is the whole catalog, and every
//! item is just an index into a bitmask on the save file, so adding the next
//! tier of unlocks is a new constant here rather than a new system. Nothing here
//! owns game behaviour; the effects read the flags through the accessors at the
//! bottom, which is what keeps "bought" and "applied" two honest steps apart.

use crate::save::Save;

/// How many things the shop can sell. The catalog is fixed at six for the first
/// slice; later tiers add constants, not new code paths.
pub const ITEM_COUNT: usize = 6;

// ---- the catalog ----------------------------------------------------------

/// Permanent overcharge: the drive's burn tier opens three seconds longer.
pub const ITEM_OVERCHARGE: usize = 0;
/// Permanent surge: the speed window holds four seconds longer.
pub const ITEM_SURGE: usize = 1;
/// Passive: the top speed is a little higher, so the line you can hold is longer.
pub const ITEM_MOMENTUM: usize = 2;
/// Permanent dash kit: shorter cooldown, faster burst.
pub const ITEM_DASH: usize = 3;
/// Unlocks the Chain Lightning power on the rack.
pub const ITEM_CHAIN: usize = 4;
/// Unlocks the Magnet power on the rack.
pub const ITEM_MAGNET: usize = 5;

/// Cost of each shop item, in cores. Flat, and the same for every player: the
/// shop is a vending machine, not a personal economy.
pub fn cost(item: usize) -> u32 {
    match item {
        ITEM_OVERCHARGE => 120,
        ITEM_SURGE => 100,
        ITEM_MOMENTUM => 200,
        ITEM_DASH => 90,
        ITEM_CHAIN => 150,
        ITEM_MAGNET => 140,
        _ => 0,
    }
}

/// Display name of a shop item, for the ledger and the shop screen.
pub fn item_name(item: usize) -> &'static str {
    match item {
        ITEM_OVERCHARGE => "OVERCHARGE CELL",
        ITEM_SURGE => "SURGE CELL",
        ITEM_MOMENTUM => "MOMENTUM CORE",
        ITEM_DASH => "DASH COIL",
        ITEM_CHAIN => "CHAIN CABLE",
        ITEM_MAGNET => "MAGNET COIL",
        _ => "?",
    }
}

/// One-line effect of a shop item, for the ledger and the shop screen.
pub fn item_desc(item: usize) -> &'static str {
    match item {
        ITEM_OVERCHARGE => "The drive's burn tier lasts 3s longer.",
        ITEM_SURGE => "Surges hold 4s longer.",
        ITEM_MOMENTUM => "Top speed +9%.",
        ITEM_DASH => "Cooldown -20%, burst +10%.",
        ITEM_CHAIN => "Kills chain lightning to nearby enemies.",
        ITEM_MAGNET => "Rare magnets hold pickups close.",
        _ => "?",
    }
}

/// True when the player owns (and so may use) the item. Owning a power unlock
/// is the same bit as buying the item that unlocks it, which keeps the save
/// file to one bitmask instead of two.
#[inline]
pub fn owns(save: &Save, item: usize) -> bool {
    has(save.unlocks, item)
}

#[inline]
fn has(unlocks: u32, item: usize) -> bool {
    unlocks & (1u32 << item as u32) != 0
}

/// Whether the rack has a slot for the power at `slot`. The base two — VIRUS
/// and OVERCHARGE — are the player's from the start; CHAIN and MAGNET are shop
/// items that set their bits.
pub fn power_unlocked(slot: usize, unlocks: u32) -> bool {
    match slot {
        crate::power::VIRUS | crate::power::OVERCHARGE => true,
        crate::power::CHAIN => has(unlocks, ITEM_CHAIN),
        crate::power::MAGNET => has(unlocks, ITEM_MAGNET),
        _ => false,
    }
}

/// Spends the player's last cores on an item. Returns false without touching
/// the save when the item is already owned or the player cannot pay.
pub fn buy(save: &mut Save, item: usize) -> bool {
    let bit = 1u32 << item as u32;
    if save.unlocks & bit != 0 || save.cores < cost(item) {
        return false;
    }
    save.unlocks |= bit;
    save.cores -= cost(item);
    true
}

// ---- drive modifiers ------------------------------------------------------

/// Seconds of Overcharge the drive grants, bought or base. The Capacitor
/// widens the burn tier from two minutes to three.
pub fn overcharge_secs(save: &Save) -> f32 {
    if owns(save, ITEM_OVERCHARGE) {
        crate::player::Player::OVERCHARGE_TIME + 3.0
    } else {
        crate::player::Player::OVERCHARGE_TIME
    }
}

/// Seconds of Surge the drive grants, bought or base. The Capacitor widens the
/// window from eight to twelve.
pub fn surge_secs(save: &Save) -> f32 {
    if owns(save, ITEM_SURGE) {
        crate::player::Player::SURGE_TIME + 4.0
    } else {
        crate::player::Player::SURGE_TIME
    }
}

/// The ceiling the drive holds, bought or base. The Momentum Core is a touch
/// higher, so the line the player can hold is a touch longer.
pub fn max_speed(save: &Save) -> f32 {
    if owns(save, ITEM_MOMENTUM) {
        crate::player::MAX_SPEED * 1.09
    } else {
        crate::player::MAX_SPEED
    }
}

/// Seconds between dashes, bought or base. The Coil trims the cooldown.
pub fn dash_cd(save: &Save) -> f32 {
    if owns(save, ITEM_DASH) {
        crate::player::DASH_CD * 0.8
    } else {
        crate::player::DASH_CD
    }
}

/// Burst speed of a dash, bought or base. The Coil pushes it a little harder.
pub fn dash_speed(save: &Save) -> f32 {
    if owns(save, ITEM_DASH) {
        crate::player::DASH_SPEED * 1.1
    } else {
        crate::player::DASH_SPEED
    }
}

/// Cores a finished run pays out, before the shop takes its cut.
///
/// Score is the base economy; a run that finished clean — no hits taken — pays a
/// small performance bonus, and the wave reached sets a floor so early deaths
/// still pay something for the attempt.
pub fn run_payout(score: u32, wave: u32, clean: bool) -> u32 {
    (score / 25) + if clean { 25 } else { 0 } + wave as u32 * 10
}
