//! Progress that outlives a single run.
//!
//! One small text file of `key=value` lines. No serialisation crate, no
//! database: if the file is missing or mangled, MOVA simply starts over.

use std::fs;
use std::path::PathBuf;

use std::fmt::Write as _;

/// Rank names, ascending. Reaching the last one is the only endgame there is.
pub const RANKS: [&str; 6] = ["NULL", "SCRIPT", "RUNNER", "BREAKER", "BURNER", "GHOST"];
/// Lifetime score needed to reach each rank.
pub const RANK_STEPS: [u32; 6] = [0, 25_000, 120_000, 350_000, 900_000, 2_000_000];
pub const RANK_MAX: usize = RANKS.len() - 1;

#[derive(Clone, Copy, Default)]
pub struct Save {
    /// Best single-run score.
    pub high: u32,
    /// Deepest wave ever reached.
    pub best_wave: u32,
    pub runs: u32,
    /// Sum of every run's score. This is what the rank ladder reads.
    pub total_score: u32,
    pub total_kills: u32,
    pub total_rams: u32,
    pub best_combo: u32,
    /// One bit per completed goal.
    pub goals: u32,
    /// One bit per sector MOVA has ever stood in. This is the map's memory and
    /// it outlives any single run, so the arena fills in over time.
    pub sectors: u32,
}

impl Save {
    /// Folds a finished run into the permanent record.
    pub fn absorb(&mut self, stats: &crate::goal::RunStats, score: u32) -> bool {
        self.runs += 1;
        self.total_score += score;
        self.total_kills += stats.kills;
        self.total_rams += stats.rams;
        self.best_wave = self.best_wave.max(stats.wave);
        self.best_combo = self.best_combo.max(stats.best_combo);
        let high = score > self.high;
        if high {
            self.high = score;
        }
        high
    }
}

/// Index into `RANKS` for a lifetime score.
pub fn rank_index(total: u32) -> usize {
    let mut i = 0;
    while i < RANKS.len() && total >= RANK_STEPS[i] {
        i += 1;
    }
    i - 1
}

pub fn rank_name(total: u32) -> &'static str {
    RANKS[rank_index(total)]
}

/// Fraction of the way from the current rank to the next. `None` at the top.
pub fn rank_progress(total: u32) -> Option<f32> {
    let i = rank_index(total);
    if i >= RANK_MAX {
        return None;
    }
    let lo = RANK_STEPS[i] as f32;
    let hi = RANK_STEPS[i + 1] as f32;
    Some(((total as f32 - lo) / (hi - lo)).clamp(0.0, 1.0))
}

// ---- disk ------------------------------------------------------------------

fn path() -> PathBuf {
    let base = std::env::var_os("XDG_DATA_HOME")
        .filter(|s| !s.is_empty())
        .map_or_else(
            || {
                std::env::var_os("HOME").map_or_else(
                    || PathBuf::from("."),
                    |h| PathBuf::from(h).join(".local").join("share"),
                )
            },
            PathBuf::from,
        );
    base.join("mova").join("save")
}

/// Reads the save file, falling back to a fresh record on any problem.
pub fn load() -> Save {
    let mut s = Save::default();
    let Ok(text) = fs::read_to_string(path()) else {
        return s;
    };
    for line in text.lines() {
        let Some((k, v)) = line.split_once('=') else {
            continue;
        };
        let Ok(v) = v.trim().parse::<u32>() else {
            continue;
        };
        match k {
            "high" => s.high = v,
            "wave" => s.best_wave = v,
            "runs" => s.runs = v,
            "score" => s.total_score = v,
            "kills" => s.total_kills = v,
            "rams" => s.total_rams = v,
            "combo" => s.best_combo = v,
            "goals" => s.goals = v,
            "sectors" => s.sectors = v,
            _ => {}
        }
    }
    s
}

/// Writes the record. Failure is never fatal; the run still counted.
pub fn store(s: &Save) {
    let p = path();
    if let Some(dir) = p.parent() {
        if fs::create_dir_all(dir).is_err() {
            return;
        }
    }
    let mut text = String::with_capacity(160);
    let fields = [
        ("high", s.high),
        ("wave", s.best_wave),
        ("runs", s.runs),
        ("score", s.total_score),
        ("kills", s.total_kills),
        ("rams", s.total_rams),
        ("combo", s.best_combo),
        ("goals", s.goals),
        ("sectors", s.sectors),
    ];
    for (k, v) in fields {
        let _ = writeln!(text, "{k}={v}");
    }
    let _ = fs::write(p, text);
}
