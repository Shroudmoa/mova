//! Headless checks on the parts of v2 that can be verified without a terminal:
//! the map generator's connectivity guarantee and the collision resolve.
//!
//! Run with `cargo test`.

use mova::solid::{SolidGrid, CELL};

/// The generator promises one connected floor with no trapped pockets, so the
/// only place that can be sealed is the plaza itself. If the map ever came out
/// over-dense, this fails loudly instead of quietly stranding a run.
///
/// The map is rolled fresh every run now, so one seed proves nothing about the
/// next one. This sweeps a spread of them.
#[test]
fn map_floor_is_connected() {
    for seed in SEEDS {
        connected(seed);
    }
}

/// The seeds swept above. Arbitrary but fixed, so a failure can be reproduced by
/// pasting the number into `SolidGrid::new`.
const SEEDS: [u32; 8] = [
    0x4D4F_5641,
    0x0000_0001,
    0x7FFF_FFFF,
    0xA5A5_5A5A,
    0x0BAD_F00D,
    0xDEAD_BEEF,
    0x1357_9BDF,
    0xFFFF_FFFF,
];

fn connected(seed: u32) {
    let grid = SolidGrid::new(seed);
    assert!(!grid.inside(0.0, 0.0), "spawn plaza must be open");

    // Walk outward from the plaza on the lattice, same way the generator's own
    // flood does, and confirm every free cell is reachable.
    let cols = 19i32;
    let rows = 11i32;
    let half_w = cols / 2;
    let half_h = rows / 2;

    let idx = |i: i32, j: i32| ((j + half_h) * cols + (i + half_w)) as usize;

    let mut occupied = vec![false; (cols * rows) as usize];
    for i in -half_w..=half_w {
        for j in -half_h..=half_h {
            occupied[idx(i, j)] = grid.inside(i as f32 * CELL, j as f32 * CELL);
        }
    }

    let free: Vec<usize> = (0..occupied.len()).filter(|&k| !occupied[k]).collect();
    assert!(
        free.len() > 40,
        "seed {seed:#x} produced a map too sparse to be a map: {}",
        free.len()
    );

    let mut seen = vec![false; occupied.len()];
    let mut stack = vec![idx(0, 0)];
    seen[idx(0, 0)] = true;
    while let Some(k) = stack.pop() {
        let i = (k as i32 % cols) - half_w;
        let j = (k as i32 / cols) - half_h;
        for dj in -1..=1i32 {
            for di in -1..=1i32 {
                let (ni, nj) = (i + di, j + dj);
                if ni < -half_w || nj < -half_h || ni > half_w || nj > half_h {
                    continue;
                }
                let m = idx(ni, nj);
                if seen[m] || occupied[m] {
                    continue;
                }
                seen[m] = true;
                stack.push(m);
            }
        }
    }

    let reached = seen.iter().filter(|s| **s).count();
    assert_eq!(
        reached,
        free.len(),
        "seed {seed:#x} sealed off part of the floor: {reached} of {} cells reached",
        free.len()
    );
}

/// Push-out has to work in every direction, including from deep inside a block
/// where there is no nearest-edge vector to speak of.
///
/// The radius is 1.5 rather than the player's 1.0 so the offsets used here are
/// unambiguously overlapping: at offset 3 on a block of half-extent 4 a radius
/// of 1.0 is exactly tangent, which is correct but proves nothing.
#[test]
fn push_out_escapes_from_every_direction() {
    let grid = SolidGrid::new(0x4D4F_5641);
    let s = grid.solids[0];
    let r = 1.5f32;

    for (dx, dy) in [
        (0.0f32, 0.0f32),
        (3.0, 0.0),
        (-3.0, 0.0),
        (0.0, 3.0),
        (0.0, -3.0),
        (2.0, 2.0),
        (-2.0, -2.0),
    ] {
        let (mut x, mut y) = (s.x + dx, s.y + dy);
        assert!(
            grid.push_out(&mut x, &mut y, r).is_some(),
            "expected a contact starting at offset {dx},{dy}"
        );
        assert!(
            !grid.inside(x, y),
            "still buried after push_out at offset {dx},{dy}"
        );
        // Escaping means clearing the block by the radius, not merely leaving
        // it, or a body can be pushed into the neighbouring block next frame.
        assert!(
            !grid.hit(x, y, r),
            "resolvable but still overlapping at offset {dx},{dy}"
        );
    }
}

/// A body sliding along a face has to end up outside the block while staying
/// inside the arena. This is the case that actually happens in play.
#[test]
fn push_out_along_a_face_lands_clear() {
    let grid = SolidGrid::new(0x4D4F_5641);
    let s = grid.solids[0];
    let (mut x, mut y) = (s.x + s.hw + 0.2, s.y + 1.5);
    assert!(grid.push_out(&mut x, &mut y, 1.0).is_some());
    assert!(!grid.hit(x, y, 1.0));
    assert!(
        y > s.y - s.hh && y < s.y + s.hh,
        "stayed within the block's span"
    );
}

/// A body that starts clear must not be moved at all, or everything would
/// drift every frame.
#[test]
fn push_out_leaves_clear_bodies_alone() {
    let grid = SolidGrid::new(0x4D4F_5641);
    let (mut x, mut y) = (7.0f32, 3.0f32);
    assert!(grid.push_out(&mut x, &mut y, 1.0).is_none());
    assert_eq!((x, y), (7.0, 3.0));
}

/// Bullets and pickups both need an honest overlap test, and both are much
/// smaller than a player.
#[test]
fn hit_detects_small_overlaps() {
    let grid = SolidGrid::new(0x4D4F_5641);
    let s = grid.solids[0];

    // Dead centre.
    assert!(grid.hit(s.x, s.y, 0.55));
    // Just clear of the face.
    assert!(!grid.hit(s.x + s.hw + 0.6, s.y, 0.55));
    // Overlapping the face by a hair.
    assert!(grid.hit(s.x + s.hw - 0.1, s.y, 0.55));
}

/// One seed in, one arena out. Every run now gets its own seed, so this is no
/// longer "the map never changes" — it is the property the rest of the file is
/// built on, since a generator that was not a pure function of its seed could
/// not be tested at all.
#[test]
fn map_is_deterministic() {
    for seed in SEEDS {
        let a = SolidGrid::new(seed);
        let b = SolidGrid::new(seed);
        assert_eq!(a.solids.len(), b.solids.len(), "seed {seed:#x}");
        for (x, y) in a.solids.iter().zip(b.solids.iter()) {
            assert_eq!((x.x, x.y), (y.x, y.y), "seed {seed:#x}");
        }
    }
}

/// And the flip side: the seed has to actually reach the generator. If the map
/// were built from a constant, every run would be the same arena and none of the
/// sweep above would be covering anything.
#[test]
fn different_seeds_make_different_maps() {
    let base = SolidGrid::new(SEEDS[0]);
    let sig = |g: &SolidGrid| -> Vec<(i32, i32)> {
        g.solids.iter().map(|s| (s.x as i32, s.y as i32)).collect()
    };
    let reference = sig(&base);
    for seed in SEEDS.iter().skip(1) {
        assert_ne!(
            reference,
            sig(&SolidGrid::new(*seed)),
            "seed {seed:#x} built the same arena as {:#x}",
            SEEDS[0]
        );
    }
}
