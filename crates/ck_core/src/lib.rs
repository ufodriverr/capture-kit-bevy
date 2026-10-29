//! Capture Kit core: game content (`content/*.ron`, see docs/CONTENT.md) and the rules built on it, with
//! no Bevy dependency so they can be unit tested and reused.

pub mod capture;
pub mod consts;
pub mod creature;
pub mod data;
pub mod inventory;
pub mod rng;
pub mod stats;
