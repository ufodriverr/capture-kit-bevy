//! Throwing Capture Orbs (Q: hold to aim with the capture chance shown, release to throw) and the capture
//! judgement (`ck_core::capture`: the chance, then one shake per rules.ron `shake_weights`). An orb is used
//! up whether it hits or not.

use bevy::prelude::*;
use ck_core::capture::{CaptureBreakdown, CaptureInputs, capture_chance, judge_shakes};

use crate::camera::CameraRig;
use crate::combat::{CombatAssets, Hurtbox, overlap_characters};
use crate::data::Db;
use crate::input::PlayerInput;
use crate::inventory::Inventory;
use crate::kinematic::{Mover, world_filter};
use crate::log::{GameLog, Say};
use crate::creatures::{AiState, Brain, CreatureActor, Creatures, Role};
use crate::player::{Health, Locomotion, Player};
use crate::progress::Progress;
use crate::resources::WorldRng;

/// Launch speed and gravity of a thrown orb; time per shake.
pub const THROW_SPEED: f32 = 24.0;
pub const THROW_GRAVITY: f32 = 9.8;
pub const SHAKE_TIME: f32 = 0.9;

#[derive(Component, Debug)]
pub struct OrbBall {
    pub item: String,
    pub vel: Vec3,
    pub life: f32,
}

#[derive(Component, Debug)]
pub struct CaptureAttempt {
    pub creature: Entity,
    pub p: f64,
    pub passed: usize,
    pub shake: usize,
    pub t: f32,
}

/// The creature is inside an orb: hidden, frozen.
#[derive(Component, Debug)]
pub struct InOrb;

/// What the HUD shows while aiming an orb.
#[derive(Resource, Debug, Default)]
pub struct CapturePreview {
    pub aiming: bool,
    pub target: Option<(String, CaptureBreakdown, bool)>,
    /// Where the aimed creature is (for the percent box beside it).
    pub target_at: Option<Vec3>,
}

pub struct CapturePlugin;

impl Plugin for CapturePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<CapturePreview>().add_systems(Update, (aim_and_throw, fly, judge).chain().in_set(crate::Sim::Combat));
    }
}

/// An orb item's capture strength (items.ron `orb_level`).
pub fn orb_level(db: &Db, item: &str) -> f64 {
    f64::from(db.item(item).map_or(1.0, |i| i.orb_level))
}

/// Capture inputs for a target. `unaware`: it hasn't noticed the player.
pub fn inputs(db: &Db, item: &str, ind: &ck_core::creature::Individual, hp: f32, max_hp: f32, unaware: bool, status: Option<&crate::status::Afflicted>) -> CaptureInputs {
    let sp = &db.creatures[&ind.species];
    CaptureInputs {
        orb_level: orb_level(db, item),
        creature_level: ind.level,
        hp: f64::from(hp),
        max_hp: f64::from(max_hp),
        catch_rate: f64::from(sp.catch_rate),
        status_bonus: crate::status::capture_bonus(db, status),
        unaware,
        uncapturable: sp.catch_rate <= 0.0,
    }
}

fn aim_dir(rig: &CameraRig) -> Vec3 {
    Quat::from_euler(EulerRot::YXZ, rig.yaw, rig.pitch, 0.0) * Vec3::NEG_Z
}

fn aim_and_throw(
    mut commands: Commands,
    time: Res<Time>,
    input: Res<PlayerInput>,
    db: Res<Db>,
    rig: Res<CameraRig>,
    roster: Res<Creatures>,
    assets: Option<Res<CombatAssets>>,
    mut preview: ResMut<CapturePreview>,
    mut log: ResMut<GameLog>,
    mut mats: ResMut<Assets<StandardMaterial>>,
    mut player: Query<(Entity, &Mover, &Locomotion, &mut Inventory, Option<&crate::partner::ShieldGuard>), With<Player>>,
    targets: Query<(&CreatureActor, &Mover, &Health, &Brain, Option<&crate::status::Afflicted>), Without<InOrb>>,
    (sq, cam, chars): (avian3d::prelude::SpatialQuery, Query<&GlobalTransform, With<crate::camera::MainCamera>>, Query<(Entity, &Mover, &crate::combat::Hurtbox), Without<Player>>),
) {
    let Ok((pe, pm, loco, mut inv, shield)) = player.single_mut() else { return };
    preview.aiming = input.throw_held;
    preview.target = None;
    preview.target_at = None;
    if input.throw_held || input.throw_released {
        // The creature nearest the aim line within 30 m.
        let dir = aim_dir(&rig);
        let eye = pm.feet + Vec3::Y * 1.5;
        let best = targets
            .iter()
            .filter(|(a, ..)| matches!(a.role, Role::Wild { .. }))
            .filter_map(|(a, m, h, b, st)| {
                let to = m.feet + Vec3::Y * 0.4 - eye;
                let along = to.dot(dir);
                let off = (to - dir * along).length() / along.max(0.1);
                (along > 0.0 && along < 30.0 && off < 0.2).then_some((off, a, h, b, m.feet, st))
            })
            .min_by(|a, b| a.0.total_cmp(&b.0));
        if let Some((_, a, h, b, feet, st)) = best
            && let Some(ind) = roster.0.get(a.id)
        {
            let sneak = !b.aware;
            let bd = capture_chance(db.rules(), &inputs(&db, &inv.orb, ind, h.hp, h.max, sneak, st));
            preview.target = Some((format!("{} Lv{}", db.creatures[&ind.species].name, ind.level), bd, sneak));
            preview.target_at = Some(feet + Vec3::Y * 0.6);
        }
    }
    if !input.throw_released {
        return;
    }
    if shield.is_some() {
        log.say(&time, "throw rejected: bond skill active");
        return;
    }
    if !matches!(loco.mode, crate::player::Mode::Ground | crate::player::Mode::Air | crate::player::Mode::Swim) {
        log.say(&time, "throw rejected: can't throw now");
        return;
    }
    let item = inv.orb.clone();
    if !inv.bag.remove(&item, 1) {
        log.say(&time, format!("throw rejected: no {}", db.item(&item).map_or(item.as_str(), |i| i.name.as_str())));
        return;
    }
    let Some(a) = assets else { return };
    let from = pm.feet + Vec3::Y * 1.5 + pm.forward() * 0.3;
    // The arc lands on the creature the capture box shows, otherwise on the point under the crosshair.
    let target = preview.target_at.or_else(|| cam.single().ok().map(|ctf| crate::combat::crosshair(&sq, ctf, pe, pm.feet + Vec3::Y * 1.2, chars.iter()).0));
    let dir = target.map_or_else(|| (aim_dir(&rig) + Vec3::Y * 0.12).normalize(), |t| crate::combat::launch_dir(from, t, THROW_SPEED, THROW_GRAVITY));
    commands.spawn((
        OrbBall { item, vel: dir * THROW_SPEED, life: 5.0 },
        Mesh3d(a.ball.clone()),
        MeshMaterial3d(mats.add(StandardMaterial { base_color: Color::srgb(0.95, 0.95, 1.0), emissive: LinearRgba::rgb(0.2, 0.6, 1.0), ..default() })),
        Transform::from_translation(from).with_scale(Vec3::splat(0.12)),
    ));
}

fn fly(
    mut commands: Commands,
    time: Res<Time>,
    db: Res<Db>,
    sq: avian3d::prelude::SpatialQuery,
    roster: Res<Creatures>,
    mut log: ResMut<GameLog>,
    mut rng: ResMut<WorldRng>,
    mut balls: Query<(Entity, &mut OrbBall, &mut Transform), Without<CreatureActor>>,
    wild: Query<(Entity, &CreatureActor, &Mover, &Hurtbox, &Health, &Brain, Option<&crate::status::Afflicted>), Without<InOrb>>,
    mut vis: Query<&mut Visibility, With<CreatureActor>>,
) {
    let dt = time.delta_secs();
    for (be, mut b, mut tf) in &mut balls {
        b.life -= dt;
        b.vel.y -= THROW_GRAVITY * dt;
        let from = tf.translation;
        let to = from + b.vel * dt;
        let wild_iter = wild.iter().filter(|(_, a, ..)| matches!(a.role, Role::Wild { .. })).map(|(e, _, m, h, ..)| (e, m, h));
        let hit = (0..=3).find_map(|i| overlap_characters(from.lerp(to, i as f32 / 3.0), 0.15, wild_iter.clone()).first().copied());
        if let Some(pe) = hit
            && let Ok((_, a, m, _, h, brain, st)) = wild.get(pe)
            && let Some(ind) = roster.0.get(a.id)
        {
            let bd = capture_chance(db.rules(), &inputs(&db, &b.item, ind, h.hp, h.max, !brain.aware, st));
            log.say(&time, format!("orb hit {} Lv{}: capture chance {:.1}%", db.creatures[&ind.species].name, ind.level, bd.result * 100.0));
            commands.entity(pe).insert(InOrb);
            if let Ok(mut v) = vis.get_mut(pe) {
                *v = Visibility::Hidden;
            }
            let weights = db.rules().shake_weights.clone();
            let rolls: Vec<f64> = weights.iter().map(|_| rng.0.f64()).collect();
            let passed = judge_shakes(bd.result, &weights, &rolls);
            let need: Vec<String> = weights.iter().map(|w| format!("{:.3}", bd.result.clamp(0.0, 1.0).powf(*w))).collect();
            let got: Vec<String> = rolls.iter().map(|r| format!("{r:.3}")).collect();
            log.say(&time, format!("shake rolls [{}] must be below [{}] -> {passed} pass", got.join(", "), need.join(", ")));
            commands.entity(be).remove::<OrbBall>().insert((CaptureAttempt { creature: pe, p: bd.result, passed, shake: 0, t: 0.0 }, Transform::from_translation(m.feet + Vec3::Y * 0.15).with_scale(Vec3::splat(0.15))));
            continue;
        }
        let step = to - from;
        if let Ok(d) = Dir3::new(step.normalize_or_zero())
            && sq.cast_ray(from, d, step.length(), true, &world_filter(&[])).is_some()
        {
            log.say(&time, "orb missed (used up)");
            commands.entity(be).despawn();
            continue;
        }
        tf.translation = to;
        if b.life <= 0.0 {
            commands.entity(be).despawn();
        }
    }
}

fn judge(
    mut commands: Commands,
    time: Res<Time>,
    db: Res<Db>,
    mut roster: ResMut<Creatures>,
    mut progress: ResMut<Progress>,
    mut log: ResMut<GameLog>,
    mut q: Query<(Entity, &mut CaptureAttempt, &mut Transform)>,
    mut actors: Query<(&CreatureActor, &mut Brain, &mut Visibility)>,
    player: Query<Entity, With<Player>>,
) {
    let dt = time.delta_secs();
    for (e, mut c, mut tf) in &mut q {
        c.t += dt;
        // Wobble.
        tf.rotation = Quat::from_rotation_z((c.t * 18.0).sin() * 0.3 * (1.0 - (c.t % SHAKE_TIME) / SHAKE_TIME));
        if c.t < SHAKE_TIME {
            continue;
        }
        c.t = 0.0;
        c.shake += 1;
        let Ok((a, mut brain, mut vis)) = actors.get_mut(c.creature) else {
            commands.entity(e).despawn();
            continue;
        };
        let id = a.id;
        if c.shake <= c.passed && c.shake < db.rules().shake_weights.len() {
            log.say(&time, format!("shake {}...", c.shake));
            continue;
        }
        commands.entity(e).despawn();
        let species = roster.0.get(id).map(|p| p.species.clone()).unwrap_or_default();
        let name = db.creatures.get(&species).map_or(species.clone(), |s| s.name.clone());
        if c.passed >= db.rules().shake_weights.len() {
            match roster.0.take_ownership(id) {
                Ok(place) => {
                    let level = roster.0.get(id).map_or(1, |p| p.level);
                    let count = progress.captures.entry(species.clone()).or_insert(0);
                    *count += 1;
                    let n = *count;
                    let base = ck_core::creature::defeat_exp(db.rules(), &db.creatures[&species], level);
                    // The first `capture_bonus_count` captures of a species give double experience.
                    let bonus = if n <= db.rules().capture_bonus_count { base } else { 0 };
                    progress.pending_exp.push((base + bonus, format!("captured {name}")));
                    log.say(&time, format!("captured {name} Lv{level} (p={:.1}%) -> {}", c.p * 100.0, match place {
                        ck_core::creature::Place::Party(i) => format!("party slot {}", i + 1),
                        _ => "Camp Core storage".into(),
                    }));
                    commands.entity(c.creature).despawn();
                }
                Err(why) => {
                    log.say(&time, format!("capture failed: {why}"));
                    commands.entity(c.creature).remove::<InOrb>();
                    *vis = Visibility::Inherited;
                }
            }
        } else {
            log.say(&time, format!("{name} broke free after {} shake(s) (p={:.1}%)", c.passed, c.p * 100.0));
            commands.entity(c.creature).remove::<InOrb>();
            *vis = Visibility::Inherited;
            // Broke free: it turns on the thrower.
            brain.aware = true;
            brain.state = AiState::Battle;
            brain.target = player.single().ok();
        }
    }
}
