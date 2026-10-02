//! Headless checks on the parts of v2 that can be verified without a terminal:
//! the map generator's connectivity guarantee and the collision resolve.
//!
//! Run with `cargo test`.

use mova::game::{ARENA_HX, ARENA_HY};
use mova::solid::{SolidGrid, CELL, COLS, PERIOD_X, PERIOD_Y, ROWS, SIGHT};

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

/// Flat index of a lattice cell. Wraps, because the arena does.
fn idx(i: i32, j: i32) -> usize {
    let i = (i + COLS / 2).rem_euclid(COLS);
    let j = (j + ROWS / 2).rem_euclid(ROWS);
    (j * COLS + i) as usize
}

fn connected(seed: u32) {
    let grid = SolidGrid::new(seed);
    assert!(!grid.inside(0.0, 0.0), "spawn plaza must be open");

    let mut occupied = vec![false; (COLS * ROWS) as usize];
    for i in -COLS / 2..=COLS / 2 {
        for j in -ROWS / 2..=ROWS / 2 {
            occupied[idx(i, j)] = grid.inside(i as f32 * CELL, j as f32 * CELL);
        }
    }

    let free: Vec<usize> = (0..occupied.len()).filter(|&k| !occupied[k]).collect();
    assert!(
        free.len() > COLS as usize * ROWS as usize / 3,
        "seed {seed:#x} produced a map too sparse to be a map: {} of {} cells open",
        free.len(),
        occupied.len()
    );

    let mut seen = vec![false; occupied.len()];
    let mut stack = vec![idx(0, 0)];
    seen[idx(0, 0)] = true;
    while let Some(k) = stack.pop() {
        let i = (k as i32 % COLS) - COLS / 2;
        let j = (k as i32 / COLS) - ROWS / 2;
        for dj in -1..=1i32 {
            for di in -1..=1i32 {
                let m = idx(i + di, j + dj);
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

/// The spawn has to be somewhere you can see, and somewhere you can run.
///
/// The one thing the generator kept getting wrong was crowding the plaza. A
/// landmark stamped one cell east of the origin put sixty units of wall twenty
/// units from MOVA's nose — a third of the screen — so the first three seconds of
/// every run were spent looking at the side of a building, and the "borderless
/// arena, keep moving, never cornered" pitch was a lie you noticed before you had
/// pressed anything.
///
/// The bug was the shape of the keep-out, not the intent. It read
/// `|i| <= r && |j| <= r`: a square, so it rejected a spot only when *both*
/// coordinates were close, and anything off to one side walked straight through.
/// Two fixes now, and this pins the one that decides the outcome. `open_sight`
/// clears the disc after the last stamp whatever asked for it, because a centre
/// is not a silhouette and a ring or a bar can reach across a line its own spot
/// respected. `spot` also keeps each stamp's centre outside `SIGHT` plus the
/// stamp's own reach, so nothing is *aimed* at the plaza — but that is intent
/// rather than guarantee, and this deliberately does not pin it. Replacing
/// `spot`'s keep-out with the square it used to be leaves every assertion here
/// passing, because `open_sight` is doing the work. Testing it anyway would mean
/// a test that fails when the map is built a different way for a reason nobody
/// asked about.
///
/// Read from the generator's own constant, not a copy of it. A test that spells
/// the number out is a test that goes on passing after the spawn is narrowed to
/// fit the map again, which is the mistake being guarded against here.
#[test]
fn the_spawn_opens_onto_open_ground() {
    for &seed in &SEEDS {
        let grid = SolidGrid::new(seed);
        for j in -SIGHT..=SIGHT {
            for i in -SIGHT..=SIGHT {
                // A disc, not the square. The square is the shape of the bug: it
                // is blind to anything off to one side, which is where the
                // offending landmark went.
                if i * i + j * j > SIGHT * SIGHT {
                    continue;
                }
                assert!(
                    !grid.inside(i as f32 * CELL, j as f32 * CELL),
                    "seed {seed:#x} put furniture at cell ({i},{j}), {SIGHT} cells \
                     from the spawn: a landmark is crowding the plaza and the run \
                     opens against a wall"
                );
            }
        }
    }
}

/// The wrap is only seamless if the arena's geometry actually repeats. v3 reads
/// every neighbour through a wrapped lattice index, so a query has to give the
/// same answer a whole period away as it does here — otherwise there is a
/// discontinuity somewhere on the seam, and since the camera keeps following,
/// MOVA finds it a few seconds in and it becomes the most memorable thing on the
/// map.
///
/// Both axes, and sampled away from the lattice points themselves, so this is a
/// statement about the collision surface rather than about the block centres.
#[test]
fn the_seam_is_not_a_discontinuity() {
    let period = (PERIOD_X, PERIOD_Y);
    let r = 1.0f32;
    for seed in SEEDS {
        let grid = SolidGrid::new(seed);
        for i in 0..40 {
            for j in 0..29 {
                // A pseudo-random sample, deterministic in `i`/`j`.
                let x = (i as f32 * 7.31 + j as f32 * 3.17).sin() * (ARENA_HX - 10.0);
                let y = (j as f32 * 5.53 - i as f32 * 2.29).cos() * (ARENA_HY - 10.0);

                assert_eq!(
                    grid.hit(x, y, r),
                    grid.hit(x + period.0, y, r),
                    "seed {seed:#x}: x={x:.1} y={y:.1} is solid on one side of the \
                     vertical seam and open on the other"
                );
                assert_eq!(
                    grid.hit(x, y, r),
                    grid.hit(x, y + period.1, r),
                    "seed {seed:#x}: x={x:.1} y={y:.1} is solid on one side of the \
                     horizontal seam and open on the other"
                );
                assert_eq!(
                    grid.inside(x, y),
                    grid.inside(x + period.0, y + period.1),
                    "seed {seed:#x}: burial differs across the corner of the seam"
                );
            }
        }
    }
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
