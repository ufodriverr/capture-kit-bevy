//! The camp: the Camp Core's area, worker limit and camp level (world.ron `camp_levels`), camp creatures
//! and their AI, ranch production, feeding, mood, healing in storage and HP regeneration.
//!
//! Workers pick work by their own job priorities, then the fewest creatures already on it, then the task
//! order (rules.ron `task_order`), then distance, if their species has the job at the rank the building's
//! job slot asks for and the slot isn't full (work.rs). Hunger interrupts work (go to a feed box and eat);
//! night interrupts work (sleep).

use std::collections::HashMap;

use bevy::prelude::*;
use ck_core::creature::Place;
use ck_core::rng::GameRng;

use crate::building::{BaseCamp, Building, Construction, Storage, shape_of};
use crate::crafting::{Station, creature_work_speed};
use crate::data::Db;
use crate::drops::{DropAssets, WorldItem, spawn_drop};
use crate::kinematic::Mover;
use crate::log::{GameLog, Say};
use crate::creatures::{AiState, Brain, CreatureActor, CreatureAssets, CreatureBody, Creatures, Role, Steer, WorldClock, spawn_creature_actor};
use crate::player::Health;

/// Work per second each station/blueprint gets from creatures this frame.
#[derive(Resource, Debug, Default)]
pub struct WorkContributions(pub HashMap<Entity, f32>);

impl WorkContributions {
    pub fn rate(&self, e: Entity) -> f32 {
        self.0.get(&e).copied().unwrap_or(0.0)
    }
}

/// A ranch: creatures with the ranch job live in it and produce their `ranch` loot.
#[derive(Component, Debug, Clone, Default)]
pub struct Ranch {
    pub timers: HashMap<u64, f32>,
}

/// What a worker is doing (for the inspector and transport state).
#[derive(Component, Debug, Clone, Default)]
pub struct WorkerTask {
    pub job: Option<Entity>,
    pub kind: &'static str,
    pub carrying: Option<(String, u32)>,
    /// Fixed assignment (carried there and put down, or set on the Job Board): works only there and waits
    /// there ("Stopped") while it has no work, until the assignment is cancelled.
    pub forced: Option<Entity>,
    /// Its nest (kept from night to night).
    pub bed: Option<Entity>,
    /// Asleep in a nest (mood recovers faster).
    pub in_bed: bool,
    /// The jobs (stations, blueprints, ranches) that were open when it last chose: it keeps its job until
    /// that job ends or a job not in this list appears.
    pub known: Vec<Entity>,
    /// Job changes so far (for the checks).
    pub switches: u32,
}

#[derive(Resource, Debug)]
pub struct BaseRng(pub GameRng);

pub struct BasePlugin;

impl Plugin for BasePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<WorkContributions>()
            .insert_resource(BaseRng(GameRng::new(31)))
            .add_systems(Update, (sync_workers, worker_ai).chain().in_set(crate::Sim::Creatures).before(crate::skills::run_skills))
            .add_systems(Update, (needs, ranch, camp_core_recovery, regen, base_level).in_set(crate::Sim::Last));
    }
}

pub fn worker_limit(world: &mut World, db: &Db) -> usize {
    let level = world.query::<&BaseCamp>().iter(world).map(|b| b.level).max().unwrap_or(1);
    max_workers(db, level) as usize
}

/// Workers allowed at a camp level.
pub fn max_workers(db: &Db, level: u32) -> u32 {
    db.camp_level(level).map_or(1, |l| l.workers)
}

/// Whether a higher camp level exists.
pub fn has_level(db: &Db, level: u32) -> bool {
    db.camp_level(level).is_some()
}

/// Camp creatures get an actor near the Camp Core; creatures no longer at the camp lose theirs.
fn sync_workers(mut commands: Commands, db: Res<Db>, roster: Res<Creatures>, assets: Option<Res<CreatureAssets>>, bases: Query<&GlobalTransform, With<BaseCamp>>, actors: Query<(Entity, &CreatureActor)>) {
    let Some(a) = assets else { return };
    let Some(base) = bases.iter().next().map(|t| t.translation()) else { return };
    let workers = roster.0.in_place(Place::Camp);
    for (e, actor) in &actors {
        if actor.role == Role::Worker && !workers.contains(&actor.id) {
            commands.entity(e).despawn();
        }
    }
    for id in workers {
        if actors.iter().any(|(_, a)| a.id == id) || roster.0.get(id).is_some_and(|p| p.incapacitated) {
            continue;
        }
        let at = base + Vec3::new(2.0, 0.2, 2.0);
        let e = spawn_creature_actor(&mut commands, &db, &roster.0, &a, id, at, Role::Worker, 0.0);
        let name = roster.0.get(id).and_then(|p| db.creatures.get(&p.species)).map_or("Creature".to_owned(), |s| s.name.clone());
        commands.entity(e).insert((WorkerTask::default(), crate::interact::Interactable { prompt: format!("{name}: lift (F)"), radius: 1.6 }));
    }
}

fn in_base(p: Vec3, base: Vec3, r: f32) -> bool {
    p.with_y(0.0).distance(base.with_y(0.0)) < r
}

fn worker_ai(
    time: Res<Time>,
    db: Res<Db>,
    clock: Res<WorldClock>,
    mut commands: Commands,
    mut roster: ResMut<Creatures>,
    mut contrib: ResMut<WorkContributions>,
    mut log: ResMut<GameLog>,
    bases: Query<(&BaseCamp, &GlobalTransform)>,
    blueprints: Query<(Entity, &GlobalTransform), With<Construction>>,
    jobs: Query<(Entity, &GlobalTransform, &Building, Option<&Station>, Option<&crate::work::Producer>, Option<&crate::work::Farm>), Without<Construction>>,
    ranches: Query<(Entity, &GlobalTransform), (With<Ranch>, Without<Construction>)>,
    mut storages: Query<(Entity, &GlobalTransform, &mut Storage), Without<Construction>>,
    items: Query<(Entity, &GlobalTransform, &WorldItem)>,
    (carry, beds, all_buildings): (Res<crate::base_tools::Carry>, Query<(Entity, &GlobalTransform, &crate::base_tools::Bed), Without<Construction>>, Query<(&Building, &GlobalTransform)>),
    mut q: Query<(Entity, &CreatureActor, &Mover, &mut Brain, &mut Steer, &CreatureBody, &mut WorkerTask)>,
) {
    contrib.0.clear();
    let Some((bc, btf)) = bases.iter().next() else { return };
    let base = btf.translation();
    let dt = time.delta_secs();
    let mut taken_items: Vec<Entity> = Vec::new();
    // Ranch slots: the ranch building's job slot.
    let ranch_max = db.build("ranch").and_then(|b| b.jobs.first()).map_or(4, |j| if j.workers == 0 { usize::MAX } else { j.workers });
    // Open station work this frame: (entity, where, reach, task order, job slot).
    let offers: Vec<(Entity, Vec3, f32, usize, crate::work::AssignRow)> = jobs
        .iter()
        .filter(|(_, t, ..)| in_base(t.translation(), base, bc.radius))
        .filter_map(|(e, t, b, st, pr, fm)| {
            let rows = crate::work::assign_rows(&db, &b.id);
            let row = if let Some(f) = fm {
                let s = f.stage.suit()?;
                rows.into_iter().find(|r| r.job == s)?
            } else if st.is_some_and(Station::busy) || pr.is_some() {
                rows.into_iter().next()?
            } else {
                return None;
            };
            let size = shape_of(&b.id).size;
            Some((e, t.translation(), size.x.max(size.z) * 0.5 + 1.0, crate::work::priority(&db, &row.task), row))
        })
        .collect();
    // Creatures on each job (station, blueprint, ranch), for the slot limits and for spreading out.
    let mut on_job: HashMap<Entity, usize> = HashMap::new();
    for (.., t) in q.iter() {
        if let Some(j) = t.job
            && (crate::work::is_facility_work(t.kind) || t.kind == "build" || t.kind == "ranch")
        {
            *on_job.entry(j).or_default() += 1;
        }
    }
    let mut claimed_beds: Vec<Entity> = Vec::new();
    for (pe, a, m, mut b, mut steer, body, mut task) in &mut q {
        if a.role != Role::Worker {
            continue;
        }
        if carry.creature == Some(pe) {
            b.note = "being carried".into();
            task.kind = "carried";
            continue;
        }
        *steer = Steer::default();
        let Some(ind) = roster.0.get(a.id).cloned() else { continue };
        let sp = &db.creatures[&ind.species];
        let go = |steer: &mut Steer, to: Vec3, speed: f32| -> f32 {
            let d = (to - m.feet).with_y(0.0);
            if d.length() > 0.3 {
                steer.dir = d.normalize();
                steer.speed = speed;
                steer.goal = Some(to);
            }
            d.length()
        };
        let walk = body.run * 0.5;
        // Night: sleep in a free nest in the camp (the same one as before if it is free), else on the spot.
        task.in_bed = false;
        if clock.is_night() && !sp.nocturnal {
            task.kind = "sleep";
            let near = |e: &Entity| beds.get(*e).map_or(f32::MAX, |(_, t, _)| t.translation().distance(m.feet));
            let bed = task.bed.filter(|e| beds.get(*e).is_ok() && !claimed_beds.contains(e)).or_else(|| {
                beds.iter().filter(|(e, t, bd)| bd.creature && in_base(t.translation(), base, bc.radius) && !claimed_beds.contains(e)).map(|(e, ..)| e).min_by(|x, y| near(x).total_cmp(&near(y)))
            });
            task.bed = bed;
            match bed.and_then(|e| beds.get(e).ok()) {
                Some((e, t, _)) => {
                    claimed_beds.push(e);
                    if go(&mut steer, t.translation(), walk) < 0.6 {
                        *steer = Steer::default();
                        b.state = AiState::Sleep;
                        b.note = "sleeping in a nest".into();
                        task.in_bed = true;
                    } else {
                        b.state = AiState::Return;
                        b.note = "going to bed".into();
                    }
                }
                None => {
                    b.state = AiState::Sleep;
                    b.note = "sleeping on the ground (no free nest)".into();
                }
            }
            continue;
        }
        // Hunger interrupts work: eat from a feed box below EAT_BELOW of a full stomach.
        let max_full = sp.stomach.max(1.0);
        if ind.fullness < max_full * ck_core::consts::EAT_BELOW || b.state == AiState::Eat {
            let food_box = storages.iter_mut().filter(|(_, _, s)| s.food_only && !s.items.is_empty()).min_by(|x, y| x.1.translation().distance(m.feet).total_cmp(&y.1.translation().distance(m.feet)));
            match food_box {
                Some((_, ftf, mut s)) => {
                    b.state = AiState::Eat;
                    task.kind = "eat";
                    if go(&mut steer, ftf.translation(), walk) < 1.6 {
                        let food = s.items.slots.iter().flatten().next().map(|st| st.item.clone());
                        if let Some(f) = food
                            && s.items.remove(&f, 1)
                        {
                            let gain = db.item(&f).map_or(10.0, |i| i.food) * 2.0;
                            if let Some(p) = roster.0.get_mut(a.id) {
                                p.fullness = (p.fullness + gain).min(max_full);
                                if p.fullness >= max_full * 0.95 {
                                    b.state = AiState::Idle;
                                }
                            }
                            log.say(&time, format!("{} ate {}", sp.name, db.item(&f).map_or(f.as_str(), |i| i.name.as_str())));
                        } else {
                            b.state = AiState::Idle;
                        }
                    }
                    b.note = "hungry: going to eat".into();
                    continue;
                }
                None => {
                    b.note = "hungry, but no food in a feed box".into();
                    if b.state == AiState::Eat {
                        b.state = AiState::Idle;
                    }
                }
            }
        }
        // Transport in progress.
        if let Some((item, n)) = task.carrying.clone() {
            let chest = storages.iter_mut().filter(|(_, _, s)| !s.food_only || db.item(&item).is_some_and(|i| i.kind == "Food")).min_by(|x, y| x.1.translation().distance(m.feet).total_cmp(&y.1.translation().distance(m.feet)));
            match chest {
                Some((_, ctf, mut s)) => {
                    task.kind = "transport";
                    b.note = format!("carrying {item} x{n}");
                    if go(&mut steer, ctf.translation(), walk * sp.haul_speed / 100.0) < 1.4 {
                        let left = s.items.add(&item, n, &*db.data);
                        task.carrying = (left > 0).then(|| (item.clone(), left));
                        if left < n {
                            log.say(&time, format!("{} stored {} x{}", sp.name, db.item(&item).map_or(item.as_str(), |i| i.name.as_str()), n - left));
                        }
                    }
                }
                None => b.note = "carrying, but no chest".into(),
            }
            continue;
        }
        // Work: every job this creature can do now, best first by its own priority number (1 first, 0 never),
        // then the fewest creatures already on it (a group spreads over the jobs instead of all piling into
        // the ranch), then the task order, then distance. A creature with a fixed assignment only works there.
        #[derive(Clone, Copy)]
        enum Job {
            Build(Entity, Vec3),
            Ranch(Entity, Vec3),
            Facility(usize),
            Fetch(Entity, Vec3),
        }
        let own = task.job;
        let crowd = |e: Entity| on_job.get(&e).copied().unwrap_or(0).saturating_sub(usize::from(own == Some(e)));
        let mut cands: Vec<((u8, usize, usize, f32), Entity, Job)> = Vec::new();
        let handcraft = creature_work_speed(&db, &ind.species, "craft");
        if handcraft > 0.0 && sp.hands {
            for (e, tf) in blueprints.iter().filter(|(_, t)| in_base(t.translation(), base, bc.radius)) {
                cands.push(((ind.work_priority("craft"), crowd(e), crate::work::priority(&db, "build"), tf.translation().distance(m.feet)), e, Job::Build(e, tf.translation())));
            }
        }
        if sp.job_rank("ranch") > 0 {
            for (e, tf) in ranches.iter().filter(|(_, t)| in_base(t.translation(), base, bc.radius)) {
                if own == Some(e) || on_job.get(&e).copied().unwrap_or(0) < ranch_max {
                    cands.push(((ind.work_priority("ranch"), crowd(e), crate::work::priority(&db, "ranch"), tf.translation().distance(m.feet)), e, Job::Ranch(e, tf.translation())));
                }
            }
        }
        for (i, (e, pos, _, pr, r)) in offers.iter().enumerate() {
            if sp.job_rank(&r.job) >= r.rank && (own == Some(*e) || r.workers == 0 || on_job.get(e).copied().unwrap_or(0) < r.workers) {
                cands.push(((ind.work_priority(&r.job), crowd(*e), *pr, pos.distance(m.feet)), *e, Job::Facility(i)));
            }
        }
        if sp.job_rank("haul") > 0
            && !storages.is_empty()
            && let Some((ie, itf, _)) = items.iter().filter(|(e, t, _)| in_base(t.translation(), base, bc.radius) && !taken_items.contains(e)).min_by(|x, y| x.1.translation().distance(m.feet).total_cmp(&y.1.translation().distance(m.feet)))
        {
            cands.push(((ind.work_priority("haul"), 0, crate::work::priority(&db, "haul"), itf.translation().distance(m.feet)), ie, Job::Fetch(ie, itf.translation())));
        }
        // Fixed assignment: the building is gone -> back to automatic; no work there now -> wait by it.
        if let Some(f) = task.forced
            && all_buildings.get(f).is_err()
        {
            log.say(&time, format!("{}'s assignment was cancelled (the building is gone)", sp.name));
            task.forced = None;
        }
        let forced = task.forced;
        if let Some(f) = forced
            && !cands.iter().any(|c| c.1 == f)
        {
            if let Some(old) = own
                && let Some(n) = on_job.get_mut(&old)
            {
                *n = n.saturating_sub(1);
            }
            task.job = None;
            task.kind = "stopped";
            b.state = AiState::Idle;
            if let Ok((fb, ftf)) = all_buildings.get(f) {
                let size = shape_of(&fb.id).size;
                if go(&mut steer, ftf.translation(), walk) < size.x.max(size.z) * 0.5 + 1.2 {
                    *steer = Steer::default();
                }
                b.note = format!("Stopped: assigned to the {}, no work there", fb.name);
            }
            continue;
        }
        cands.retain(|c| c.0.0 > 0 || forced == Some(c.1));
        // Stick with the current job while it lasts, unless a new task appeared (something built, a station
        // got an order, a place freed up): re-choosing every frame would make creatures swap jobs back and
        // forth as the "fewest creatures" counts change under them.
        let open: Vec<Entity> = cands.iter().filter(|c| !matches!(c.2, Job::Fetch(..))).map(|c| c.1).collect();
        let new_task = open.iter().any(|e| !task.known.contains(e));
        task.known = open;
        let keep = own.filter(|_| !new_task && forced.is_none()).and_then(|o| cands.iter().find(|c| c.1 == o)).map(|c| (c.1, c.2));
        let pick = keep.or_else(|| {
            cands
                .iter()
                .filter(|c| forced.is_none_or(|f| c.1 == f))
                .min_by(|x, y| x.0.0.cmp(&y.0.0).then(x.0.1.cmp(&y.0.1)).then(x.0.2.cmp(&y.0.2)).then(x.0.3.total_cmp(&y.0.3)))
                .map(|c| (c.1, c.2))
        });
        if let (Some(old), Some((e, _))) = (own, pick)
            && old != e
        {
            task.switches += 1;
        }
        // Moving to another job frees a place on the old one.
        if let Some((e, job)) = pick
            && own != Some(e)
            && !matches!(job, Job::Fetch(..))
        {
            if let Some(old) = own
                && let Some(n) = on_job.get_mut(&old)
            {
                *n = n.saturating_sub(1);
            }
            *on_job.entry(e).or_default() += 1;
        }
        match pick.map(|p| p.1) {
            Some(Job::Build(e, pos)) => {
                task.kind = "build";
                task.job = Some(e);
                if go(&mut steer, pos, walk) < 2.0 {
                    *contrib.0.entry(e).or_default() += handcraft;
                    b.note = format!("building ({handcraft:.0}/s)");
                    work_cost(&mut roster, a.id, dt, &db);
                } else {
                    b.note = "going to build".into();
                }
            }
            Some(Job::Ranch(e, pos)) => {
                task.kind = "ranch";
                task.job = Some(e);
                let spot = pos + Vec3::new(((a.id % 5) as f32 - 2.0) * 1.2, 0.0, ((a.id / 5 % 5) as f32 - 2.0) * 1.2);
                go(&mut steer, spot, walk * 0.5);
                b.note = "at the ranch".into();
            }
            Some(Job::Facility(i)) => {
                let (e, pos, reach, _, r) = &offers[i];
                let speed = creature_work_speed(&db, &ind.species, &r.job);
                let verb = crate::work::verb(&r.job);
                let name = jobs.get(*e).map_or(String::new(), |j| j.2.name.clone());
                task.kind = verb;
                task.job = Some(*e);
                // Within reach: stand and work (no steering, so the work animation plays).
                if (*pos - m.feet).with_y(0.0).length() < *reach || go(&mut steer, *pos, walk) < *reach {
                    *contrib.0.entry(*e).or_default() += speed;
                    b.note = format!("{verb} at the {name} ({speed:.0}/s){}", if forced.is_some() { " [Assigned]" } else { "" });
                    steer.face = Some(*pos - m.feet);
                    work_cost(&mut roster, a.id, dt, &db);
                } else {
                    b.note = format!("going to the {name} ({verb})");
                }
            }
            Some(Job::Fetch(ie, pos)) => {
                taken_items.push(ie);
                task.kind = "transport";
                task.job = None;
                if go(&mut steer, pos, walk * sp.haul_speed / 100.0) < 1.2
                    && let Ok((_, _, wi)) = items.get(ie)
                {
                    task.carrying = Some((wi.item.clone(), wi.count));
                    commands.entity(ie).despawn();
                }
                b.note = "fetching an item".into();
            }
            None => {
                if let Some(old) = own
                    && let Some(n) = on_job.get_mut(&old)
                {
                    *n = n.saturating_sub(1);
                }
                task.job = None;
                task.kind = "idle";
                b.state = AiState::Idle;
                b.note = "no work".into();
                // Stay near the Camp Core.
                if m.feet.distance(base) > 6.0 {
                    go(&mut steer, base, walk);
                }
                continue;
            }
        }
        b.state = AiState::Work;
    }
}

/// Working costs a little mood and stomach.
fn work_cost(roster: &mut Creatures, id: u64, dt: f32, _db: &Db) {
    if let Some(p) = roster.0.get_mut(id) {
        p.mood = (p.mood - 0.11 * dt).max(0.0);
        p.fullness = (p.fullness - ck_core::consts::STOMACH_PER_S * 0.5 * dt).max(0.0);
    }
}

/// Stomach and mood of camp creatures (rules.ron).
fn needs(time: Res<Time>, db: Res<Db>, clock: Res<WorldClock>, mut roster: ResMut<Creatures>, workers: Query<(&CreatureActor, &WorkerTask)>) {
    let dt = time.delta_secs();
    let in_bed: Vec<u64> = workers.iter().filter(|(_, t)| t.in_bed).map(|(a, _)| a.id).collect();
    let hunger_san = db.rules().mood_loss_hungry;
    let starve_san = db.rules().mood_loss_starving;
    let full_pct = db.rules().full_threshold;
    let recover = db.rules().mood_regen_full;
    let sleep = db.rules().ground_sleep_mood_rate.abs();
    let species: HashMap<String, (f32, f32)> = db.creatures.iter().map(|(k, s)| (k.clone(), (s.stomach.max(1.0), s.stomach_rate.max(0.0)))).collect();
    for p in roster.0.all.values_mut() {
        if !p.owned || p.place != Place::Camp {
            continue;
        }
        let (max, rate) = species.get(&p.species).copied().unwrap_or((100.0, 1.0));
        p.fullness = (p.fullness - ck_core::consts::STOMACH_PER_S * rate * dt).max(0.0);
        let f = p.fullness / max;
        if f <= 0.0 {
            p.mood -= starve_san * dt;
        } else if f < ck_core::consts::EAT_BELOW {
            p.mood -= hunger_san * dt;
        } else if f >= full_pct {
            p.mood += recover * dt;
        }
        if clock.is_night() {
            p.mood += sleep * dt * if in_bed.contains(&p.id) { 2.0 } else { 1.0 };
        }
        p.mood = p.mood.clamp(0.0, 100.0);
    }
}

/// Ranch: each ranching creature at the ranch rolls its species' `ranch` loot table every
/// RANCH_WORK_PER_DROP / (its ranch speed) seconds.
fn ranch(
    mut commands: Commands,
    time: Res<Time>,
    db: Res<Db>,
    drops: Option<Res<DropAssets>>,
    mut rng: ResMut<BaseRng>,
    mut log: ResMut<GameLog>,
    mut ranches: Query<(&GlobalTransform, &mut Ranch), Without<Construction>>,
    workers: Query<(&CreatureActor, &Mover, &WorkerTask)>,
) {
    let Some(da) = drops else { return };
    let dt = time.delta_secs();
    for (rtf, mut r) in &mut ranches {
        for (a, m, t) in &workers {
            if t.kind != "ranch" || m.feet.distance(rtf.translation()) > 6.0 {
                continue;
            }
            let sp = &db.creatures[&a.species];
            let rank = sp.job_rank("ranch").max(1);
            let speed = db.job("ranch").and_then(|j| j.speeds.get(rank as usize)).copied().unwrap_or(40.0);
            let interval = ck_core::consts::RANCH_WORK_PER_DROP / speed.max(1.0);
            let timer = r.timers.entry(a.id).or_insert(interval);
            *timer -= dt;
            if *timer > 0.0 {
                continue;
            }
            *timer = interval;
            // One row of the species' loot table, picked by weight.
            if let Some(rows) = sp.ranch.as_ref().and_then(|l| db.world.loot.get(l))
                && let Some((item, _, min, max)) = pick_weighted(rows, rng.0.f32())
            {
                let n = rng.0.range_i32(*min as i32, *max as i32) as u32;
                spawn_drop(&mut commands, &da, &db, item, n, m.feet + Vec3::Y * 0.6, &mut rng.0);
                log.say(&time, format!("{} produced {} x{n} at the ranch", sp.name, db.item(item).map_or(item.as_str(), |i| i.name.as_str())));
            }
        }
    }
}

/// A loot row picked by weight; `roll` in [0, 1).
pub fn pick_weighted(rows: &[(String, f32, u32, u32)], roll: f32) -> Option<&(String, f32, u32, u32)> {
    let total: f32 = rows.iter().map(|r| r.1.max(0.0)).sum();
    let mut at = roll * total;
    rows.iter().find(|r| {
        at -= r.1.max(0.0);
        at < 0.0
    }).or(rows.last())
}

/// Creatures in the Camp Core's storage heal (STORAGE_HEAL_PER_S) and come back from being knocked out;
/// their mood recovers too.
fn camp_core_recovery(time: Res<Time>, db: Res<Db>, mut roster: ResMut<Creatures>) {
    let dt = time.delta_secs();
    let params = db.rules();
    let mood_regen = db.rules().stored_mood_regen;
    for p in roster.0.all.values_mut() {
        if !p.owned || p.place != Place::Storage {
            continue;
        }
        let max = p.stats(params, &db.creatures[&p.species]).max_hp;
        p.hp = (p.hp + max * ck_core::consts::STORAGE_HEAL_PER_S * dt).min(max);
        p.mood = (p.mood + mood_regen * dt).min(100.0);
        if p.incapacitated && p.hp >= max {
            p.incapacitated = false;
        }
    }
}

/// Everyone regenerates `creature_hp_regen_percent` of max HP per second.
fn regen(time: Res<Time>, db: Res<Db>, mut q: Query<&mut Health>, mut roster: ResMut<Creatures>) {
    let dt = time.delta_secs();
    let rate = db.rules().creature_hp_regen_percent / 100.0;
    for mut h in &mut q {
        if h.hp > 0.0 {
            h.hp = (h.hp + h.max * rate * dt).min(h.max);
        }
    }
    // Party creatures in their orbs too.
    let params = db.rules();
    for p in roster.0.all.values_mut() {
        if p.owned && matches!(p.place, Place::Party(_)) && !p.incapacitated {
            let max = p.stats(params, &db.creatures[&p.species]).max_hp;
            p.hp = (p.hp + max * rate * dt).min(max);
        }
    }
}

/// One camp goal toward the next level: what, have, need.
#[derive(Debug, Clone)]
pub struct BaseMission {
    pub what: String,
    pub have: u32,
    pub need: u32,
}

/// The goals of camp level `level` (empty at the max level), with progress from the worker count and the
/// finished buildings' ids.
pub fn base_missions(db: &Db, level: u32, workers: u32, built: &[String]) -> Vec<BaseMission> {
    let Some(needs) = db.camp_level(level).and_then(|l| l.needs.as_ref()) else { return Vec::new() };
    let mut out = Vec::new();
    if needs.workers > 0 {
        out.push(BaseMission { what: format!("Have {} creatures working at the camp", needs.workers), have: workers, need: needs.workers });
    }
    for (ids, n) in &needs.builds {
        let Some(first) = ids.first() else { continue };
        if *n == 0 {
            continue;
        }
        let have = built.iter().filter(|b| ids.iter().any(|x| x.eq_ignore_ascii_case(b))).count() as u32;
        let names: Vec<String> = ids.iter().map(|i| db.build(i).map_or(i.clone(), |b| b.name.clone())).collect();
        let what = format!("Build {}{}", names.join(" or "), if *n > 1 { format!(" x{n}") } else { String::new() });
        let _ = first;
        out.push(BaseMission { what, have, need: *n });
    }
    out
}

/// Camp level-up when the current level's goals are met.
fn base_level(time: Res<Time>, db: Res<Db>, roster: Res<Creatures>, mut log: ResMut<GameLog>, mut bases: Query<&mut BaseCamp>, built: Query<&Building, Without<Construction>>) {
    let Ok(mut bc) = bases.single_mut() else { return };
    let ids: Vec<String> = built.iter().map(|b| b.id.clone()).collect();
    let missions = base_missions(&db, bc.level, roster.0.in_place(Place::Camp).len() as u32, &ids);
    let ok = !missions.is_empty() && missions.iter().all(|m| m.have >= m.need);
    if ok && db.camp_level(bc.level + 1).is_some() {
        bc.level += 1;
        log.say(&time, format!("camp level {} (workers up to {})", bc.level, max_workers(&db, bc.level)));
    }
}
