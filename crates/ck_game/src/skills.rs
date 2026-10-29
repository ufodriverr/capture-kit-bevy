//! Creature active skills: selection, range and line-of-sight checks, cooldowns, and execution.
//!
//! Selection: the target within max range with a clear line (a separate ray check) and beyond min range;
//! approach/side-step when nothing fits. Of the valid skills the highest-power one wins. The cooldown
//! restarts when the action ends.
//! Roll skills (skills.ron `Roll`: time, speed, homing, radius) curl up, tumble into the target and end
//! dizzy; shot skills (`Shot`: speed, radius) take Ready 1.0 s / Process 1.0 s / After 0.8 s.

use std::collections::HashMap;

use avian3d::prelude::*;
use bevy::prelude::*;
use ck_core::data::SkillAction;
use ck_core::stats::Element;

use crate::combat::{Combatant, CombatAssets, HitKind, HitMsg, Hurtbox, Projectile};
use crate::data::Db;
use crate::kinematic::{self, Mover, yaw_of};
use crate::creatures::{Brain, CreatureActor, CreatureBody, Steer, line_of_sight};
use crate::resources::Tool;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Phase {
    Ready,
    Process,
    After,
}

#[derive(Debug, Clone)]
pub struct Cast {
    pub skill: String,
    pub phase: Phase,
    pub t: f32,
    pub target: Entity,
    pub dir: Vec3,
    pub hit_done: bool,
}

#[derive(Component, Debug, Clone, Default)]
pub struct SkillSlots {
    pub equipped: Vec<String>,
    pub cooldowns: HashMap<String, f32>,
    pub casting: Option<Cast>,
    /// Set by the AI: start this skill on the brain's target.
    pub request: Option<(String, Entity)>,
}

impl SkillSlots {
    pub fn new(equipped: &[String]) -> Self {
        Self { equipped: equipped.to_vec(), ..default() }
    }
    pub fn ready(&self, w: &str) -> bool {
        self.cooldowns.get(w).is_none_or(|c| *c <= 0.0)
    }
}

/// The numbers a skill needs at run time (metres, seconds).
#[derive(Debug, Clone)]
pub struct SkillInfo {
    pub power: f32,
    pub min_range: f32,
    pub max_range: f32,
    pub max_height: f32,
    pub cool: f32,
    pub element: Element,
    pub ignore_ray: bool,
}

pub fn info(db: &Db, w: &str) -> Option<SkillInfo> {
    let z = db.skills.get(w)?;
    Some(SkillInfo {
        power: z.power,
        min_range: z.min_range,
        max_range: z.max_range,
        max_height: z.max_height,
        cool: z.cooldown,
        element: z.element,
        ignore_ray: false,
    })
}

/// Battle movement and skill choice toward a target position. Sets `slots.request` (via the brain's
/// target) when a skill fits; otherwise approaches or side-steps.
pub fn battle_steer(sq: &SpatialQuery, m: &Mover, steer: &mut Steer, body: &CreatureBody, slots: &SkillSlots, target: Vec3, b: &mut Brain, _dt: f32) {
    let to = target - m.feet;
    steer.face = Some(to);
    b.note = String::new();
    if slots.request.is_some() {
        return;
    }
    // Ranges are evaluated by run_skills (it has the data); here only move to keep a sensible distance.
    let dist = to.with_y(0.0).length();
    if dist > 6.0 {
        steer.dir = to.with_y(0.0).normalize_or_zero();
        steer.speed = body.run;
        b.note = "approach".into();
    } else if !line_of_sight(sq, m.feet + Vec3::Y * 0.5, target + Vec3::Y * 0.8) {
        steer.dir = Quat::from_rotation_y(1.2) * to.with_y(0.0).normalize_or_zero();
        steer.speed = body.run;
        b.note = "no line of sight".into();
    } else {
        // SideMove: circle the target while skills cool down.
        steer.dir = Quat::from_rotation_y(std::f32::consts::FRAC_PI_2 * b.detour_sign) * to.with_y(0.0).normalize_or_zero();
        steer.speed = body.walk.max(1.0);
        b.note = "side step".into();
    }
}

/// Picks the skill to use now: valid range, height and line of sight, off cooldown, highest power.
pub fn choose_skill(db: &Db, sq: &SpatialQuery, slots: &SkillSlots, from: Vec3, target: Vec3) -> Result<String, String> {
    let d = (target - from).with_y(0.0).length();
    let dh = (target.y - from.y).abs();
    let mut best: Option<(f32, &String)> = None;
    let mut reason = "no skills equipped".to_owned();
    for w in &slots.equipped {
        let Some(i) = info(db, w) else { continue };
        if !slots.ready(w) {
            reason = "all skills cooling down".into();
            continue;
        }
        if d > i.max_range {
            reason = format!("target out of range ({d:.1} m > {:.1} m)", i.max_range);
            continue;
        }
        if d < i.min_range {
            reason = format!("target too close ({d:.1} m < {:.1} m)", i.min_range);
            continue;
        }
        if dh > i.max_height {
            reason = "height difference too large".into();
            continue;
        }
        if !i.ignore_ray && !line_of_sight(sq, from + Vec3::Y * 0.5, target + Vec3::Y * 0.8) {
            reason = "blocked line of sight".into();
            continue;
        }
        if best.is_none_or(|b| i.power > b.0) {
            best = Some((i.power, w));
        }
    }
    best.map(|b| b.1.clone()).ok_or(reason)
}

fn element_color(e: Element) -> Color {
    match e {
        Element::Fire => Color::srgb(1.0, 0.4, 0.1),
        Element::Water => Color::srgb(0.2, 0.5, 1.0),
        Element::Spark => Color::srgb(1.0, 0.95, 0.3),
        Element::Leaf => Color::srgb(0.3, 0.9, 0.3),
        Element::Frost => Color::srgb(0.65, 0.9, 1.0),
        Element::Earth => Color::srgb(0.7, 0.55, 0.35),
        _ => Color::srgb(0.9, 0.95, 1.0),
    }
}

/// Starts requested skills, ticks cooldowns and runs the active cast of every creature.
pub fn run_skills(
    mut commands: Commands,
    time: Res<Time>,
    db: Res<Db>,
    sq: SpatialQuery,
    assets: Option<Res<CombatAssets>>,
    mut mats: ResMut<Assets<StandardMaterial>>,
    mut hits: MessageWriter<HitMsg>,
    mut log: ResMut<crate::log::GameLog>,
    shielding: Query<(), With<crate::partner::Shielding>>,
    mut set: ParamSet<(Query<(Entity, &Mover, &Hurtbox, &crate::player::Health)>, Query<(Entity, &mut Mover, &mut SkillSlots, &mut Steer, &mut Brain, &Combatant), With<CreatureActor>>)>,
) {
    let dt = time.delta_secs();
    let targets: HashMap<Entity, (Vec3, f32, f32)> = set.p0().iter().map(|(e, m, h, hp)| (e, (m.feet, h.radius, hp.hp))).collect();
    for (e, mut m, mut slots, mut steer, mut b, comb) in &mut set.p1() {
        for c in slots.cooldowns.values_mut() {
            *c -= dt * db.rules().companion_cooldown_rate;
        }
        if shielding.contains(e) {
            slots.casting = None;
            continue;
        }
        // Start a requested skill.
        if slots.casting.is_none()
            && let Some(target) = b.target
            && let Some(&(tp, _, thp)) = targets.get(&target)
            && thp > 0.0
        {
            match choose_skill(&db, &sq, &slots, m.feet, tp) {
                Ok(w) => {
                    let dir = (tp - m.feet).with_y(0.0).normalize_or(m.forward());
                    slots.casting = Some(Cast { skill: w.clone(), phase: Phase::Ready, t: 0.0, target, dir, hit_done: false });
                    let name = db.skills.get(&w).map_or(w.as_str(), |z| z.name.as_str());
                    log.push(time.elapsed_secs(), format!("{} uses {name}", comb.name));
                    b.note = format!("casting {name}");
                }
                Err(why) => {
                    if b.note.is_empty() || b.note == "side step" {
                        b.note = why;
                    }
                }
            }
        }
        slots.request = None;
        let Some(mut cast) = slots.casting.clone() else { continue };
        let Some(i) = info(&db, &cast.skill) else {
            slots.casting = None;
            continue;
        };
        cast.t += dt;
        let tgt = targets.get(&cast.target).copied();
        let action = db.skills.get(&cast.skill).map(|s| s.action.clone());
        let roly = matches!(action, Some(SkillAction::Roll { .. }));
        let mut done = false;
        steer.speed = 0.0;
        steer.dir = Vec3::ZERO;
        if roly {
            let Some(SkillAction::Roll { time: attack_time, speed, homing, radius }) = action else { unreachable!() };
            match cast.phase {
                Phase::Ready => {
                    if let Some((tp, _, _)) = tgt {
                        cast.dir = (tp - m.feet).with_y(0.0).normalize_or(cast.dir);
                    }
                    steer.face = Some(cast.dir);
                    // Curl up for 0.35 s before rolling.
                    if cast.t > 0.35 {
                        cast.phase = Phase::Process;
                        cast.t = 0.0;
                    }
                }
                Phase::Process => {
                    // Homing: turn the roll toward the target at `homing` rad/s.
                    if let Some((tp, tr, _)) = tgt {
                        let want = (tp - m.feet).with_y(0.0).normalize_or(cast.dir);
                        let a0 = yaw_of(cast.dir);
                        let a1 = kinematic::turn_toward(a0, yaw_of(want), homing * dt);
                        cast.dir = kinematic::yaw_forward(a1);
                        let center = m.feet + Vec3::Y * 0.5;
                        let tc = Vec3::new(tp.x, center.y.clamp(tp.y, tp.y + 1.5), tp.z);
                        if !cast.hit_done && center.distance(tc) < radius + tr {
                            cast.hit_done = true;
                            hits.write(HitMsg { target: cast.target, attacker: e, power: i.power, kind: HitKind::Skill, element: i.element, tool: Tool::None, push: cast.dir });
                        }
                    }
                    // creature_move moves the body with this velocity (steer.locked).
                    steer.locked = true;
                    m.vel = Vec3::new(cast.dir.x * speed, m.vel.y, cast.dir.z * speed);
                    let blocked = cast.t > 0.2 && m.contacts.iter().any(|n| n.y < m.walkable && n.dot(cast.dir) < -0.5);
                    m.yaw = yaw_of(cast.dir);
                    if cast.hit_done || cast.t >= attack_time || blocked {
                        cast.phase = Phase::After;
                        cast.t = 0.0;
                        m.vel = Vec3::new(0.0, m.vel.y, 0.0);
                    }
                }
                Phase::After => {
                    // Dizzy for 1.2 s after the roll.
                    b.note = "dizzy".into();
                    if cast.t > 1.2 {
                        done = true;
                    }
                }
            }
        } else {
            let (ready, process, after) = (1.0, 1.0, 0.8);
            match cast.phase {
                Phase::Ready => {
                    if let Some((tp, _, _)) = tgt {
                        cast.dir = (tp - m.feet).normalize_or(cast.dir);
                    }
                    steer.face = Some(cast.dir.with_y(0.0));
                    if cast.t > ready {
                        cast.phase = Phase::Process;
                        cast.t = 0.0;
                        if let Some(a) = &assets {
                            let (speed, r) = match action {
                                Some(SkillAction::Shot { speed, radius }) => (speed, radius),
                                _ => (30.0, 0.3),
                            };
                            let aim = tgt.map_or(cast.dir, |(tp, _, _)| (tp + Vec3::Y * 0.6 - (m.feet + Vec3::Y * 0.6)).normalize_or(cast.dir));
                            let mat = mats.add(StandardMaterial { base_color: element_color(i.element), emissive: element_color(i.element).to_linear() * 2.0, ..default() });
                            commands.spawn((
                                Projectile { vel: aim * speed, attacker: e, power: i.power, kind: HitKind::Skill, element: i.element, life: (i.max_range / speed).max(0.5) + 0.3, gravity: 0.0, radius: r, faction: comb.faction, status: None },
                                Mesh3d(a.ball.clone()),
                                MeshMaterial3d(mat),
                                Transform::from_translation(m.feet + Vec3::Y * 0.6 + aim * 0.6).with_scale(Vec3::splat(r)),
                            ));
                        }
                    }
                }
                Phase::Process => {
                    steer.face = Some(cast.dir.with_y(0.0));
                    if cast.t > process {
                        cast.phase = Phase::After;
                        cast.t = 0.0;
                    }
                }
                Phase::After => {
                    if cast.t > after {
                        done = true;
                    }
                }
            }
        }
        if done {
            // The cooldown starts when the action ends.
            slots.cooldowns.insert(cast.skill.clone(), i.cool);
            slots.casting = None;
        } else {
            slots.casting = Some(cast);
        }
    }
}
