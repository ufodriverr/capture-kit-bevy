//! The game content (`content/*.ron`, docs/CONTENT.md) as a Bevy resource.

use std::sync::Arc;

use bevy::prelude::*;
use ck_core::data::{GameData, Rules};

use crate::tuning::MoveTuning;

/// The job ids of content/jobs.ron, in display order (UI columns and task words).
pub const JOBS: [&str; 12] = ["kindle", "water", "plant", "power", "craft", "gather", "chop", "mine", "brew", "chill", "haul", "ranch"];

#[derive(Resource, Debug, Clone)]
pub struct Db {
    pub data: Arc<GameData>,
    /// Sandbox mode (`--sandbox`): every recipe and building costs at most 1 of
    /// each material and the player starts at the max level.
    pub easy: bool,
}

impl std::ops::Deref for Db {
    type Target = GameData;
    fn deref(&self) -> &GameData {
        &self.data
    }
}

/// The `assets` folder (sounds, icons): next to the executable, in the working directory or in the repo.
pub fn assets_dir() -> std::path::PathBuf {
    let candidates = [std::env::current_exe().ok().and_then(|p| p.parent().map(|p| p.join("assets"))), Some("assets".into()), Some(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets"))];
    let found = candidates.into_iter().flatten().find(|p| p.join("ui/icons").is_dir()).unwrap_or_else(|| "assets".into());
    std::path::absolute(&found).unwrap_or(found)
}

/// Sandbox edits to the loaded content: material costs capped at 1.
fn sandbox(data: &mut GameData) {
    for r in &mut data.recipes {
        for m in &mut r.materials {
            m.1 = m.1.min(1);
        }
    }
    for b in &mut data.builds {
        for m in &mut b.materials {
            m.1 = m.1.min(1);
        }
    }
}

impl Db {
    pub fn load(easy: bool) -> anyhow::Result<Self> {
        let mut data = GameData::load(&GameData::default_dir())?;
        // Not fatal, but the game can't be played through as it stands.
        for p in data.progression_problems() {
            eprintln!("content warning: {p}");
        }
        if easy {
            sandbox(&mut data);
        }
        Ok(Self { data: Arc::new(data), easy })
    }

    pub fn rules(&self) -> &Rules {
        &self.data.rules
    }

    /// Movement tuning: the engine defaults with the stamina costs from rules.ron.
    pub fn move_tuning(&self) -> MoveTuning {
        let r = &self.data.rules;
        MoveTuning { sprint_sp: r.sprint_stamina, jump_sp: r.jump_stamina, glider_sp: r.glide_stamina, swim_idle_sp: 0.0, swim_sp: r.swim_stamina, swim_dash_sp: r.swim_dash_stamina, ..MoveTuning::default() }
    }
}
