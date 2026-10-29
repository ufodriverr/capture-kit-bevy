//! Player progression: level and XP (rules.ron `exp_base` / `exp_power`), technology points
//! (`tech_points_per_level` per level), status points, unlocked technologies; the active companion
//! shares kill XP.

use std::collections::BTreeSet;

use bevy::prelude::*;
use ck_core::creature::add_exp;

use crate::combat::Combatant;
use crate::data::Db;
use crate::log::{GameLog, Say};
use crate::creatures::Creatures;
use crate::player::Player;

#[derive(Resource, Debug, Clone)]
pub struct Progress {
    pub level: u32,
    pub exp: u32,
    pub tech_points: u32,
    pub status_points: u32,
    pub unlocked: BTreeSet<String>,
    /// XP to hand out this frame: (amount, reason).
    pub pending_exp: Vec<(u32, String)>,
    /// Captures per species (double experience for the first rules.ron `capture_bonus_count`).
    pub captures: std::collections::BTreeMap<String, u32>,
    /// Status points spent: HP, SP, Attack, Weight, Work speed.
    pub alloc: [u32; 5],
    /// Index of the current mission in the tutorial chain (quests.rs).
    pub mission: usize,
}

impl Progress {
    pub fn new(db: &Db) -> Self {
        let unlocked: BTreeSet<String> = db.rules().start_unlocked.iter().cloned().collect();
        Self { level: 1, exp: 0, tech_points: ck_core::consts::START_TECH_POINTS, status_points: 0, unlocked, pending_exp: Vec::new(), captures: Default::default(), alloc: [0; 5], mission: 0 }
    }

    /// The starting progress; in sandbox mode the player starts at the max level with the technology and
    /// status points those levels give.
    pub fn start(db: &Db) -> Self {
        let mut p = Self::new(db);
        if db.easy {
            let gained = ck_core::creature::add_exp(db.rules(), &mut p.level, &mut p.exp, u32::MAX / 4);
            p.exp = 0;
            p.tech_points += gained * db.rules().tech_points_per_level;
            p.status_points += gained * db.rules().status_points_per_level;
        }
        p
    }

    pub fn recipe_unlocked(&self, db: &Db, recipe: &str) -> bool {
        db.technologies.iter().any(|t| self.unlocked.contains(&t.id) && t.recipes.iter().any(|r| r.eq_ignore_ascii_case(recipe)))
    }

    pub fn build_unlocked(&self, db: &Db, build: &str) -> bool {
        db.technologies.iter().any(|t| self.unlocked.contains(&t.id) && t.builds.iter().any(|r| r.eq_ignore_ascii_case(build)))
    }
}

pub struct ProgressPlugin;

impl Plugin for ProgressPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, give_exp.in_set(crate::Sim::Last).after(crate::creatures::creature_deaths));
    }
}

fn give_exp(time: Res<Time>, db: Res<Db>, mut progress: ResMut<Progress>, mut roster: ResMut<Creatures>, party: Res<crate::companion::Party>, mut log: ResMut<GameLog>, mut player: Query<&mut Combatant, With<Player>>) {
    if progress.pending_exp.is_empty() {
        return;
    }
    let pending = std::mem::take(&mut progress.pending_exp);
    for (xp, why) in pending {
        let p = &mut *progress;
        let gained = add_exp(db.rules(), &mut p.level, &mut p.exp, xp);
        log.say(&time, format!("+{xp} XP ({why})"));
        if gained > 0 {
            p.tech_points += gained * db.rules().tech_points_per_level;
            p.status_points += gained * db.rules().status_points_per_level;
            log.say(&time, format!("player level {} (+{} technology points)", p.level, gained * db.rules().tech_points_per_level));
            if let Ok(mut c) = player.single_mut() {
                c.level = p.level;
            }
        }
        if let Some(id) = party.active_id
            && let Some(ind) = roster.0.get_mut(id)
        {
            let before = ind.level;
            let ind_level = &mut ind.level;
            let g = add_exp(db.rules(), ind_level, &mut ind.exp, xp);
            if g > 0 {
                let sp = &db.creatures[&ind.species];
                let new: Vec<String> = sp.learnset.iter().filter(|(lv, w)| *lv > before && *lv <= ind.level && !ind.learned.contains(w)).map(|(_, w)| w.clone()).collect();
                for w in new {
                    log.say(&time, format!("{} learned {}", sp.name, db.skills.get(&w).map_or(w.as_str(), |z| z.name.as_str())));
                    ind.learned.push(w.clone());
                    if ind.equipped.len() < ck_core::creature::EQUIP_SLOTS {
                        ind.equipped.push(w);
                    }
                }
                log.say(&time, format!("{} reached level {}", sp.name, ind.level));
            }
        }
    }
}
