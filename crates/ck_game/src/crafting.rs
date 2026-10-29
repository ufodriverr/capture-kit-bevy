//! Crafting stations (Workbench, Campfire): a queue of recipe jobs, materials taken when a job
//! is queued and given back when it's cancelled, progress from work points, finished items waiting
//! on the station until collected (F). Work speed: the player = rules.ron `player_craft_speed` +
//! `point_work_speed` per status point; a creature = its job's jobs.ron speed at its rank x species
//! `craft_speed`/100.

use bevy::prelude::*;

use crate::data::Db;
use crate::input::PlayerInput;
use crate::interact::Focus;
use crate::inventory::Inventory;
use crate::log::{GameLog, Say};
use crate::player::Player;
use crate::progress::Progress;

#[derive(Debug, Clone)]
pub struct Job {
    pub recipe: String,
    pub remaining: u32,
    pub work: f32,
    pub need: f32,
}

#[derive(Component, Debug, Clone, Default)]
pub struct Station {
    pub queue: Vec<Job>,
    pub output: Vec<(String, u32)>,
}

impl Station {
    pub fn busy(&self) -> bool {
        !self.queue.is_empty()
    }
}

/// Which station crafts a recipe (lower-case build id, recipes.ron `station`).
pub fn station_for(db: &Db, recipe: &str) -> String {
    db.recipe(recipe).map_or_else(|| "workbench".into(), |r| r.station.to_ascii_lowercase())
}

/// Work points per second the player adds (rules.ron `player_craft_speed` plus status points).
pub fn player_work_speed(db: &Db, progress: &Progress) -> f32 {
    db.rules().player_craft_speed + progress.alloc[4] as f32 * db.rules().point_work_speed
}

/// A creature's work speed for a job (`craft`, `haul`, ...): jobs.ron speed at its rank x its species'
/// `craft_speed` / 100; 0 when it can't do the job.
pub fn creature_work_speed(db: &Db, species: &str, job: &str) -> f32 {
    let sp = &db.creatures[species];
    let rank = sp.job_rank(job) as usize;
    if rank == 0 {
        return 0.0;
    }
    let base = db.job(job).and_then(|j| j.speeds.get(rank.min(j.speeds.len().saturating_sub(1)))).copied().unwrap_or(50.0);
    base * sp.craft_speed.max(1.0) / 100.0
}

/// Queues `n` of a recipe, taking the materials now. Returns a reason when it can't.
/// Free crafting (sandbox cheat, F2): missing materials don't block crafting or building; what the bag has
/// is still used. On by default in sandbox mode.
#[derive(Resource, Debug, Clone, Copy)]
pub struct FreeCraft(pub bool);

pub fn queue(db: &Db, station: &mut Station, inv: &mut Inventory, recipe: &str, n: u32, free: bool) -> Result<(), String> {
    let r = db.recipe(recipe).ok_or("unknown recipe")?;
    for (m, c) in &r.materials {
        if !free && inv.count(m) < c * n {
            return Err(format!("not enough {} ({}/{}; F2 = free crafting)", db.item(m).map_or(m.as_str(), |i| i.name.as_str()), inv.count(m), c * n));
        }
    }
    for (m, c) in &r.materials {
        let take = (c * n).min(inv.count(m));
        inv.bag.remove(m, take);
    }
    match station.queue.iter_mut().find(|j| j.recipe == r.id) {
        Some(j) => j.remaining += n,
        None => station.queue.push(Job { recipe: r.id.clone(), remaining: n, work: 0.0, need: r.work.max(1.0) }),
    }
    Ok(())
}

/// Cancels a job and returns its unused materials to the bag (what doesn't fit is reported).
pub fn cancel(db: &Db, station: &mut Station, inv: &mut Inventory, index: usize) -> Vec<(String, u32)> {
    if index >= station.queue.len() {
        return Vec::new();
    }
    let j = station.queue.remove(index);
    let mut lost = Vec::new();
    if let Some(r) = db.recipe(&j.recipe) {
        for (m, c) in &r.materials {
            let left = inv.give(db, m, c * j.remaining);
            if left > 0 {
                lost.push((m.clone(), left));
            }
        }
    }
    lost
}

pub struct CraftingPlugin;

impl Plugin for CraftingPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, (work_stations, toggle_free).in_set(crate::Sim::Last));
    }
}

fn toggle_free(time: Res<Time>, input: Res<PlayerInput>, mut free: ResMut<FreeCraft>, mut log: ResMut<crate::log::GameLog>) {
    if input.cheat_pressed {
        free.0 = !free.0;
        log.say(&time, format!("free crafting {}", if free.0 { "ON: missing materials are ignored" } else { "OFF" }));
    }
}

fn work_stations(
    time: Res<Time>,
    input: Res<PlayerInput>,
    db: Res<Db>,
    focus: Res<Focus>,
    mut progress: ResMut<Progress>,
    mut log: ResMut<GameLog>,
    workers: Res<crate::base::WorkContributions>,
    mut q: Query<(Entity, &crate::building::Building, &mut Station)>,
    player: Query<&Inventory, With<Player>>,
) {
    let dt = time.delta_secs();
    let _ = player;
    for (e, b, mut st) in &mut q {
        if st.queue.is_empty() {
            continue;
        }
        let mut rate = workers.rate(e);
        // A station whose job slot isn't player-workable (the Smelter) is worked by creatures only.
        if input.interact_held && focus.0.as_ref().is_some_and(|(f, _)| *f == e) && crate::work::assign_rows(&db, &b.id).first().is_none_or(|r| r.player) {
            rate += player_work_speed(&db, &progress);
        }
        let job = &mut st.queue[0];
        job.work += rate * dt;
        if job.work < job.need {
            continue;
        }
        job.work -= job.need;
        job.remaining -= 1;
        let recipe = job.recipe.clone();
        if job.remaining == 0 {
            st.queue.remove(0);
        }
        if let Some(r) = db.recipe(&recipe) {
            let (product, count) = (r.product.clone(), r.count);
            match st.output.iter_mut().find(|(i, _)| *i == product) {
                Some(o) => o.1 += count,
                None => st.output.push((product.clone(), count)),
            }
            let xp = db.rules().craft_exp * r.exp;
            progress.pending_exp.push((xp.round() as u32, format!("crafted {}", db.item(&product).map_or(product.as_str(), |i| i.name.as_str()))));
            log.say(&time, format!("{}: {} x{count} ready", b.name, db.item(&product).map_or(product.as_str(), |i| i.name.as_str())));
        }
    }
}

/// Takes a station's finished items into the bag.
pub fn collect(db: &Db, st: &mut Station, inv: &mut Inventory) -> Vec<(String, u32)> {
    let mut got = Vec::new();
    let out = std::mem::take(&mut st.output);
    for (item, n) in out {
        let left = inv.give(db, &item, n);
        if n - left > 0 {
            got.push((item.clone(), n - left));
        }
        if left > 0 {
            st.output.push((item, left));
        }
    }
    got
}
