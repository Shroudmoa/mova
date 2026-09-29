//! Sixteen things worth doing. They persist, they are all visible on the menu,
//! and none of them need a tutorial.

use crate::region;
use crate::save::{Save, RANK_MAX, RANK_STEPS};

pub const COUNT: usize = 16;

pub struct Goal {
    /// Bit position in `Save::goals`.
    pub bit: u32,
    pub label: &'static str,
}

pub const GOALS: [Goal; COUNT] = [
    Goal {
        bit: 1 << 0,
        label: "FIRST BLOOD",
    },
    Goal {
        bit: 1 << 1,
        label: "RAM x10",
    },
    Goal {
        bit: 1 << 2,
        label: "TOP SPEED",
    },
    Goal {
        bit: 1 << 3,
        label: "COMBO x10",
    },
    Goal {
        bit: 1 << 4,
        label: "WAVE 03",
    },
    Goal {
        bit: 1 << 5,
        label: "WAVE 06",
    },
    Goal {
        bit: 1 << 6,
        label: "WAVE 10",
    },
    Goal {
        bit: 1 << 7,
        label: "SCORE 50K",
    },
    Goal {
        bit: 1 << 8,
        label: "SCORE 150K",
    },
    Goal {
        bit: 1 << 9,
        label: "SURVIVE 2:00",
    },
    Goal {
        bit: 1 << 10,
        label: "UNTOUCHED 05",
    },
    Goal {
        bit: 1 << 11,
        label: "RANK GHOST",
    },
    // The v2 half: the map is somewhere to go, and these are what make going
    // there a goal rather than a detour.
    Goal {
        bit: 1 << 12,
        label: "CARTOGRAPHER",
    },
    Goal {
        bit: 1 << 13,
        label: "CORE HUNTER",
    },
    Goal {
        bit: 1 << 14,
        label: "DEEP WELL",
    },
    Goal {
        bit: 1 << 15,
        label: "SIX SECTORS, ONE RUN",
    },
];

/// What a single run managed. Filled in as it happens, checked once it ends.
#[derive(Clone, Copy, Default)]
pub struct RunStats {
    pub score: u32,
    pub wave: u32,
    pub kills: u32,
    pub rams: u32,
    pub best_combo: u32,
    /// Hits taken. Zero is a run worth talking about.
    pub hits: u32,
    pub seconds: f32,
    pub peak_speed: f32,
    /// Sectors entered for the first time, this run.
    pub sectors: u32,
    /// Cores collected.
    pub cores: u32,
    /// Surge pickups taken.
    pub surges: u32,
    /// Repairs taken.
    pub heals: u32,
    /// Brutes killed.
    pub brutes: u32,
}

impl RunStats {
    pub fn summary(&self) -> String {
        let m = (self.seconds / 60.0) as u32;
        let s = self.seconds - m as f32 * 60.0;
        format!(
            "{}:{:02}.{:02}   {} KILLS   {} RAMS",
            m,
            s as u32,
            ((s - s.floor()) * 10.0) as u32,
            self.kills,
            self.rams
        )
    }
}

/// True when goal `i` is satisfied. Most read a single run; the last one reads
/// the permanent record.
pub fn done(i: usize, r: &RunStats, s: &Save) -> bool {
    match i {
        0 => r.kills >= 1,
        1 => r.rams >= 10,
        2 => r.peak_speed >= 0.999,
        3 => r.best_combo >= 10,
        4 => r.wave >= 3,
        5 => r.wave >= 6,
        6 => r.wave >= 10,
        7 => r.score >= 50_000,
        8 => r.score >= 150_000,
        9 => r.seconds >= 120.0,
        10 => r.wave >= 5 && r.hits == 0,
        11 => s.total_score >= RANK_STEPS[RANK_MAX],
        12 => s.sectors & region::ALL == region::ALL,
        13 => r.cores >= 40,
        // Chart the opposite corner from the spawn plaza. The map is wide, so
        // crossing it is the achievement.
        14 => s.sectors & (1 << 2) != 0 && s.sectors & (1 << 5) != 0,
        15 => r.sectors >= region::COUNT as u32,
        _ => false,
    }
}

pub fn count_done(s: &Save) -> usize {
    (0..COUNT).filter(|&i| s.goals & GOALS[i].bit != 0).count()
}
