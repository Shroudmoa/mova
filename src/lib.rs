//! MOVA — a small survival arcade game with a map worth crossing.
//!
//! The crate is split so the simulation can be exercised without a terminal:
//! `game` owns the rules, `renderer` owns every pixel, and the geometry in
//! `solid` carries the one guarantee the map generator has to keep (a single
//! connected floor).

pub mod enemy;
pub mod game;
pub mod goal;
pub mod input;
pub mod pickup;
pub mod player;
pub mod projectile;
pub mod region;
pub mod renderer;
pub mod save;
pub mod score;
pub mod solid;
