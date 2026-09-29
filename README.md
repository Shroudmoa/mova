# MOVA

A very small survival arcade game for the terminal. Move, dodge, and let the gun
do the rest.

```
cargo run --release
```

## Controls

| Key | Action |
| --- | --- |
| `W A S D` / arrows | move (momentum, not grid steps) |
| `SPACE` | dash — brief burst of speed, short invulnerability |
| `P` | pause, without leaving the run |
| `M` | show / hide the corner radar |
| `G` | goal ledger, from the menu or the game-over screen |
| `ENTER` | start / retry |
| `Q` / `ESC` / `CTRL-C` | quit |

## The loop

You are the `●`. The gun fires on its own at whatever is closest, so all that
matters is where you stand and how fast you are moving when things die.

- **Speed pays.** Kills score more the faster you are moving, and a dash that
  slices through an enemy pays a near-miss bonus.
- **Full speed cannot be touched.** At the cap MOVA burns — a tail of `▲`
  embers, a white-hot body, the speed gauge burned out — and nothing lands.
  Just under the cap is still just under, so holding the line is the whole
  skill.
- **Combo caps at x10.** It decays on its own if you stop killing, and the window
  gets tighter the higher it goes. Taking a hit resets it to zero.
- **Waves only get worse.** More enemies, faster enemies, tighter spawns.

Three hit points. The flash means you got hit.

## v2 — free roam

v1 was an arena you survived in. v2 is a map you cross.

**Every run is a new map.** Pillars are generated from a fresh seed each run, so
no two roams are the same walk. The generator guarantees one connected floor
either way: it raises the fill density until almost all the free space is
flood-reachable from the plaza, then fills in whatever pockets are left, so
there are no traps, and the spawn plaza is never built on. The seed is the only
input, which is what lets a bad layout be reported and replayed, and
`tests/map.rs` checks the guarantee across a spread of seeds.

**Six named sectors.** *Lantern Yard*, *The Spine*, *Deep Well*, *Ash Mile*,
*Quiet Gate*, *Hollow Mile*. Which one you are in is on the permanent HUD
readout, and finding one pays a discovery bonus that scales with how much of the
map you have already charted. The map you have seen is saved between runs, so it
fills in over time.

**Pickups worth the detour.** Up to twenty at a time, scattered across the floor
and refilling, so a roaming run crosses one every few seconds:

| | |
| --- | --- |
| `o` **Core** | score, and it refreshes the combo — the reason to leave cover |
| `*` **Surge** | 7 seconds above the speed cap with the drag turned down |
| `+` **Heal** | one hit point; only ever spawns while you are actually hurt |

**Three enemies instead of one.** `x` Grunt weaves and takes two hits. `>` Runner
sprints and dies to anything, including a ram. `O` Brute takes six and is too
heavy to ram at all — it has to be shot, and the gun will prefer anything softer
that is in range.

**A radar, because a scrolling camera lies by omission.** The disc in the top
corner shows the pillars around you, the loot in range, and every enemy with the
far ones pulled onto the rim so you get a bearing as well as a warning. `M`
toggles it; it is dropped automatically on terminals too narrow to hold it
without covering the sector banner.

**Juice.** Screen shake on impacts, hit-stop on kills, shockwave rings and word
popups on pickups.

## Layout

```
src/
  main.rs        fixed-step loop, terminal setup, overlay state, teardown
  lib.rs         module list, so tests can drive the game without a terminal
  game.rs        world state, waves, spawning, collision resolution, sectors
  solid.rs       map generation and the collision resolve
  region.rs      the six sector names and the lookup
  pickup.rs      pickup kinds, magnet, bob
  player.rs      movement, dash, surge, the full-speed burn, damage
  enemy.rs       the three enemy kinds and how they steer around pillars
  projectile.rs  auto-fire and target selection
  score.rs       score, combo, short-lived feedback effects
  goal.rs        the sixteen goals and the per-run stat block
  save.rs        the save file, one line per field
  renderer.rs    all drawing, palette, camera, HUD, radar
  input.rs       key state, one-shot actions, and their debounce
tests/
  map.rs         map invariants: connectivity across seeds, determinism, push-out
  play.rs        the run itself: camera, the speed cap, a fresh map per run
  render.rs      frame layout: the radar, one body, and the screens that overdraw
examples/
  preview.rs     renders frames to plain text, no TTY required
```

The simulation is a plain fixed-step update; `renderer::render` writes into a
buffer and `renderer::draw` is the thin ratatui wrapper around it. The crate is
split into a library and a binary so the tests can drive a real `Game` and check
real frames.

## Checking it

```
cargo test                      # map invariants, the run, and frame layout
cargo run --example preview -- 104 30 600   # a live frame as plain text
```

The preview takes `cols rows steps [mode]`, where mode is `auto` (the running
game), `goals`, or `over`.
