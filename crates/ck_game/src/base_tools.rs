//! Camp objects that do something besides crafting: beds (the player sleeps through the night, camp creatures
//! sleep in creature beds), the Repair Bench, the Job Board (Job Priorities and the Assignment Board:
//! per-creature priority numbers and fixed assignments), and lifting camp creatures to carry them to a
//! facility for an assignment (F puts them down at the facility in focus).

use std::collections::HashMap;

use bevy::prelude::*;
use ck_core::consts as ov;

use crate::building::{Building, Construction};
use crate::data::Db;
use crate::input::PlayerInput;
use crate::interact::{Focus, Interact, Interactable};
use crate::inventory::Inventory;
use crate::kinematic::Mover;
use crate::log::{GameLog, Say};
use crate::creatures::{CreatureActor, Creatures, Role, Steer, WorldClock};
use crate::player::{Health, Player};

/// A bed. The player's (`creature: false`) skips the night; creature beds are where camp creatures sleep.
#[derive(Component, Debug, Clone, Copy)]
pub struct Bed {
    pub creature: bool,
}

#[derive(Component, Debug, Clone, Copy)]
pub struct RepairBench;

/// The Job Board: opens Job Priorities and the Assignment Board.
#[derive(Component, Debug, Clone, Copy)]
pub struct PriorityBoard;

/// Next value when a priority cell is clicked. Everything starts at the least important (5), so one click
/// makes a job first: 5 -> 1 -> 2 -> 3 -> 4 -> off (0) -> 5.
pub fn next_prio(p: u8) -> u8 {
    let last = ck_core::creature::DEFAULT_WORK_PRIO;
    match p {
        0 => last,
        p if p >= last => 1,
        p if p == last - 1 => 0,
        p => p + 1,
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn priority_cycle() {
        let mut seq = vec![ck_core::creature::DEFAULT_WORK_PRIO];
        for _ in 0..6 {
            seq.push(super::next_prio(*seq.last().unwrap()));
        }
        assert_eq!(seq, [5, 1, 2, 3, 4, 0, 5]);
    }
}

/// A job's display name (jobs.ron).
pub fn suit_name<'a>(db: &'a Db, id: &'a str) -> &'a str {
    db.job(id).map_or(id, |j| j.name.as_str())
}

/// Whether a camp creature can be assigned to a building; the error says why not. `fixed_there` = creatures
/// already assigned to it.
pub fn check_fixed(db: &Db, species: &str, b: &Building, blueprint: bool, ranch: bool, fixed_there: usize) -> Result<(), String> {
    let sp = db.creatures.get(species).ok_or("unknown creature")?;
    let fail = |why: String| Err(format!("Can't assign it to the {}: {why}", b.name));
    let full = || Err(format!("The {} has no free place; the creature goes back to choosing its own work.", b.name));
    if blueprint {
        return if crate::crafting::creature_work_speed(db, species, "craft") > 0.0 && sp.hands { Ok(()) } else { fail(format!("building needs the {} job and hands.", suit_name(db, "craft"))) };
    }
    if ranch {
        let max = db.build("ranch").and_then(|r| r.jobs.first()).map_or(4, |j| j.workers);
        return if sp.job_rank("ranch") == 0 {
            fail(format!("it can't do the {} job.", suit_name(db, "ranch")))
        } else if max > 0 && fixed_there >= max {
            full()
        } else {
            Ok(())
        };
    }
    let rows = crate::work::assign_rows(db, &b.id);
    let Some(first) = rows.first() else { return fail("nothing to do there.".into()) };
    match rows.iter().find(|r| sp.job_rank(&r.job) >= r.rank) {
        Some(r) if r.workers > 0 && fixed_there >= r.workers => full(),
        Some(_) => Ok(()),
        None if rows.iter().any(|r| sp.job_rank(&r.job) > 0) => fail(format!("its {} talent is too low.", suit_name(db, &first.job))),
        None => fail(format!("it can't do the {} job.", suit_name(db, &first.job))),
    }
}

/// The camp creature being carried, and a short pause after putting one down (the F release that follows must
/// not open the station's menu).
#[derive(Resource, Debug, Default)]
pub struct Carry {
    pub creature: Option<Entity>,
    pub cooldown: f32,
}

pub struct BaseToolsPlugin;

impl Plugin for BaseToolsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Carry>().add_systems(Update, (sleep, carry).in_set(crate::Sim::Last).before(crate::menus::open_close)).add_systems(Update, carry_follow.after(crate::creatures::creature_move).before(crate::camera::follow));
    }
}

/// F at the player's bed after dusk: sleep until morning (SLEEP_FROM_HOUR 18:30 to WAKE_HOUR 06:00, HP full).
fn sleep(time: Res<Time>, mut ev: MessageReader<Interact>, mut clock: ResMut<WorldClock>, mut log: ResMut<GameLog>, beds: Query<(&Bed, &Building), Without<Construction>>, mut player: Query<&mut Health, With<Player>>) {
    for Interact(e) in ev.read() {
        let Ok((bed, b)) = beds.get(*e) else { continue };
        if bed.creature {
            log.say(&time, format!("{}: camp creatures sleep here at night", b.name));
            continue;
        }
        let h = clock.hour();
        if !(h >= ov::SLEEP_FROM_HOUR || h < ov::WAKE_HOUR) {
            log.say(&time, format!("sleep rejected: you can only sleep at night ({:02.0}:30-{:02.0}:00)", ov::SLEEP_FROM_HOUR.floor(), ov::WAKE_HOUR));
            continue;
        }
        let wait = (ov::WAKE_HOUR - h).rem_euclid(24.0);
        clock.minutes += wait * 60.0;
        if let Ok(mut hp) = player.single_mut() {
            hp.hp = hp.max;
        }
        log.say(&time, format!("slept in the {} until {:02.0}:00 (HP restored)", b.name, ov::WAKE_HOUR));
    }
}

/// F on a camp creature lifts it; F again puts it down: at the facility in focus (assignment there, if it
/// can work it), or in front of the player (Automatic Work Mode).
#[allow(clippy::too_many_arguments)]
fn carry(
    mut commands: Commands,
    time: Res<Time>,
    input: Res<PlayerInput>,
    db: Res<Db>,
    roster: Res<Creatures>,
    focus: Res<Focus>,
    mut carry: ResMut<Carry>,
    mut ev: MessageReader<Interact>,
    mut log: ResMut<GameLog>,
    player: Query<&Mover, With<Player>>,
    mut actors: Query<(&CreatureActor, &mut Mover, &mut crate::base::WorkerTask), Without<Player>>,
    jobs: Query<(&Building, &GlobalTransform, Has<Construction>, Has<crate::base::Ranch>), Or<(With<crate::crafting::Station>, With<crate::work::Producer>, With<crate::work::Farm>, With<crate::base::Ranch>, With<Construction>)>>,
) {
    carry.cooldown = (carry.cooldown - time.delta_secs()).max(0.0);
    let name = |a: &CreatureActor| roster.0.get(a.id).map_or("Creature".to_owned(), |p| db.creatures.get(&p.species).map_or(p.species.clone(), |s| s.name.clone()));
    let Ok(pm) = player.single() else { return };
    if let Some(pe) = carry.creature {
        let fixed: Vec<crate::base::WorkerTask> = actors.iter().filter(|(x, ..)| x.id != 0).map(|(.., t)| t.clone()).collect();
        let Ok((a, mut m, mut task)) = actors.get_mut(pe) else {
            carry.creature = None;
            return;
        };
        if !input.interact_pressed {
            return;
        }
        let target = focus.0.as_ref().map(|(e, _)| *e).filter(|e| *e != pe).and_then(|e| jobs.get(e).ok().map(|j| (e, j)));
        let species = roster.0.get(a.id).map(|p| p.species.clone()).unwrap_or_default();
        let spot = match target {
            Some((je, (b, jtf, blueprint, ranch))) => {
                let fixed_there = fixed.iter().filter(|t| t.forced == Some(je)).count();
                match check_fixed(&db, &species, b, blueprint, ranch, fixed_there) {
                    Ok(()) => {
                        task.forced = Some(je);
                        task.job = None;
                        log.say(&time, format!("put {}: assigned to the {}", name(a), b.name));
                    }
                    Err(why) => {
                        task.forced = None;
                        log.say(&time, format!("put {}: {why}", name(a)));
                    }
                }
                let size = crate::building::shape_of(&b.id).size;
                jtf.translation() + (pm.feet - jtf.translation()).with_y(0.0).normalize_or(Vec3::X) * (size.x.max(size.z) * 0.5 + 0.6)
            }
            None => {
                log.say(&time, format!("put {} down", name(a)));
                pm.feet + pm.forward() * 1.2
            }
        };
        m.feet = spot + Vec3::Y * 0.3;
        m.vel = Vec3::ZERO;
        commands.entity(pe).insert(Interactable { prompt: format!("{}: lift (F)", name(a)), radius: 1.6 });
        carry.creature = None;
        carry.cooldown = 0.6;
        return;
    }
    for Interact(e) in ev.read() {
        if carry.cooldown > 0.0 {
            continue;
        }
        if let Ok((a, _, mut task)) = actors.get_mut(*e)
            && a.role == Role::Worker
        {
            task.forced = None;
            task.job = None;
            task.carrying = None;
            carry.creature = Some(*e);
            commands.entity(*e).remove::<Interactable>();
            log.say(&time, format!("lifted {} (F at a work place puts it to work there, F elsewhere puts it down)", name(a)));
            break;
        }
    }
}

/// The carried creature rides above the player's head.
fn carry_follow(carry: Res<Carry>, player: Query<&Mover, With<Player>>, mut q: Query<(&mut Mover, &mut Transform, &mut Steer), (With<CreatureActor>, Without<Player>)>) {
    let (Some(pe), Ok(pm)) = (carry.creature, player.single()) else { return };
    if let Ok((mut m, mut tf, mut steer)) = q.get_mut(pe) {
        m.feet = pm.feet + Vec3::Y * 1.85 + pm.forward() * 0.1;
        m.vel = Vec3::ZERO;
        m.yaw = pm.yaw;
        *steer = Steer { locked: true, ..default() };
        tf.translation = m.feet;
        tf.rotation = Quat::from_rotation_y(m.yaw);
    }
}

/// Repair cost for an item stack (half its recipe's materials, scaled by the missing durability,
/// rounded up; nothing when it has no recipe). None when it doesn't need repair.
pub fn repair_cost(db: &Db, item: &str, durability: Option<f32>) -> Option<Vec<(String, u32)>> {
    let max = db.item(item)?.durability?;
    let missing = 1.0 - durability? / max.max(1.0);
    if missing <= 0.001 {
        return None;
    }
    let recipe = db.recipes.iter().find(|r| r.product.eq_ignore_ascii_case(item));
    Some(recipe.map_or_else(Vec::new, |r| r.materials.iter().map(|(m, n)| (m.clone(), ((*n as f32) * 0.5 * missing).ceil().max(1.0) as u32)).collect()))
}

/// Every damaged stack the player has: (slot label, item, durability, cost).
pub fn damaged(db: &Db, inv: &Inventory) -> Vec<(String, String, f32, Vec<(String, u32)>)> {
    let mut out = Vec::new();
    let mut push = |label: String, s: &ck_core::inventory::Stack| {
        if let Some(cost) = repair_cost(db, &s.item, s.durability) {
            out.push((label, s.item.clone(), s.durability.unwrap_or(0.0), cost));
        }
    };
    for (i, w) in inv.weapons.iter().enumerate() {
        if let Some(s) = w {
            push(format!("weapon {}", i + 1), s);
        }
    }
    if let Some(s) = &inv.armor {
        push("armor".into(), s);
    }
    if let Some(s) = &inv.head {
        push("head".into(), s);
    }
    if let Some(s) = &inv.glider {
        push("glider".into(), s);
    }
    for (i, s) in inv.bag.slots.iter().enumerate() {
        if let Some(s) = s {
            push(format!("bag {}", i + 1), s);
        }
    }
    out
}

/// Repairs one damaged stack (by `damaged` label) or all of them, paying the materials (free crafting
/// ignores what is missing). Returns the log line.
pub fn repair(db: &Db, inv: &mut Inventory, which: Option<&str>, free: bool) -> String {
    let list: Vec<_> = damaged(db, inv).into_iter().filter(|(l, ..)| which.is_none_or(|w| w == l)).collect();
    if list.is_empty() {
        return "repair: nothing needs repairing".into();
    }
    let mut total: HashMap<String, u32> = HashMap::new();
    for (.., cost) in &list {
        for (m, n) in cost {
            *total.entry(m.clone()).or_default() += n;
        }
    }
    if !free && let Some((m, n)) = total.iter().find(|(m, n)| inv.count(m) < **n) {
        return format!("repair rejected: not enough {} ({}/{n}; F2 = free crafting)", db.item(m).map_or(m.as_str(), |i| i.name.as_str()), inv.count(m));
    }
    for (m, n) in &total {
        let take = (*n).min(inv.count(m));
        inv.bag.remove(m, take);
    }
    let mut names = Vec::new();
    for (label, item, ..) in &list {
        let max = db.item(item).and_then(|i| i.durability).unwrap_or(100.0);
        let slot = if let Some(i) = label.strip_prefix("weapon ").and_then(|n| n.parse::<usize>().ok()) {
            inv.weapons.get_mut(i - 1).and_then(|w| w.as_mut())
        } else if label == "armor" {
            inv.armor.as_mut()
        } else if label == "head" {
            inv.head.as_mut()
        } else if label == "glider" {
            inv.glider.as_mut()
        } else {
            label.strip_prefix("bag ").and_then(|n| n.parse::<usize>().ok()).and_then(|i| inv.bag.slots.get_mut(i - 1)).and_then(|s| s.as_mut())
        };
        if let Some(s) = slot {
            s.durability = Some(max);
            names.push(db.item(item).map_or(item.clone(), |i| i.name.clone()));
        }
    }
    format!("repaired {}", names.join(", "))
}
