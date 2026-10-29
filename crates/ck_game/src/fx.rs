//! Hit feedback: sparks where a hit lands (wood chips, stone chips, crystal shards, a burst on a knock-out),
//! the struck creature or player recoils (anim.rs), trees sway and rocks shake when hit, a small camera shake,
//! and a hit marker around the crosshair. Visual only.

use bevy::prelude::*;

use crate::anim::{CreatureRig, PlayerRig};
use crate::combat::{Damaged, HitMsg, Hurtbox};
use crate::kinematic::Mover;
use crate::player::Player;
use crate::resources::{NodeKind, ResourceNode};

/// Camera shake strength (0..1), read by `camera::follow`.
#[derive(Resource, Debug, Default)]
pub struct Shake(pub f32);

#[derive(Component, Debug)]
struct Spark {
    vel: Vec3,
    life: f32,
    max: f32,
    size: f32,
    gravity: f32,
}

/// A hit resource node: sways (trees) or shakes (rocks) for a moment.
#[derive(Component, Debug)]
struct Wobble {
    t: f32,
    /// Horizontal direction the hit came from.
    dir: Vec3,
}

/// A node part's rest transform while it wobbles.
#[derive(Component, Debug)]
struct WobbleBase(Transform);

#[derive(Component, Debug)]
struct HitMarker;

#[derive(Resource, Debug)]
struct FxAssets {
    mesh: Handle<Mesh>,
    hit: Handle<StandardMaterial>,
    hurt: Handle<StandardMaterial>,
    puff: Handle<StandardMaterial>,
    wood: Handle<StandardMaterial>,
    leaf: Handle<StandardMaterial>,
    stone: Handle<StandardMaterial>,
    crystal: Handle<StandardMaterial>,
    flame: Handle<StandardMaterial>,
    water: Handle<StandardMaterial>,
    berry: Handle<StandardMaterial>,
}

/// Seconds left on the crosshair hit marker, and whether it was a knock-out.
#[derive(Resource, Debug, Default)]
struct Marker {
    t: f32,
    kill: bool,
}

const WOBBLE_S: f32 = 0.45;

pub struct FxPlugin;

impl Plugin for FxPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Shake>().init_resource::<Marker>().add_systems(Startup, setup).add_systems(Update, ((on_hits, work_particles, sparks, wobble, marker, decay).chain().after(crate::Sim::Combat).before(crate::Sim::Last),));
    }
}

fn setup(mut commands: Commands, mut meshes: ResMut<Assets<Mesh>>, mut mats: ResMut<Assets<StandardMaterial>>) {
    let glow = |mats: &mut Assets<StandardMaterial>, c: Color, e: LinearRgba| mats.add(StandardMaterial { base_color: c, emissive: e, ..default() });
    commands.insert_resource(FxAssets {
        mesh: meshes.add(Sphere::new(1.0).mesh().ico(1).unwrap()),
        hit: glow(&mut mats, Color::srgb(1.0, 0.95, 0.7), LinearRgba::rgb(8.0, 6.0, 2.0)),
        hurt: glow(&mut mats, Color::srgb(1.0, 0.3, 0.2), LinearRgba::rgb(6.0, 0.8, 0.4)),
        puff: mats.add(StandardMaterial { base_color: Color::srgb(0.95, 0.95, 0.95), perceptual_roughness: 1.0, ..default() }),
        wood: mats.add(Color::srgb(0.6, 0.42, 0.22)),
        leaf: mats.add(Color::srgb(0.25, 0.55, 0.2)),
        stone: mats.add(Color::srgb(0.6, 0.6, 0.62)),
        crystal: glow(&mut mats, Color::srgb(0.4, 0.75, 1.0), LinearRgba::rgb(0.5, 1.6, 3.0)),
        flame: glow(&mut mats, Color::srgb(1.0, 0.55, 0.1), LinearRgba::rgb(9.0, 3.0, 0.3)),
        water: glow(&mut mats, Color::srgb(0.35, 0.65, 1.0), LinearRgba::rgb(0.2, 0.6, 1.5)),
        berry: mats.add(Color::srgb(0.85, 0.1, 0.15)),
    });
    // Four dots around the crosshair, shown briefly when a hit lands.
    for (x, y) in [(-1.0, -1.0), (1.0, -1.0), (-1.0, 1.0), (1.0, 1.0)] {
        commands.spawn((
            HitMarker,
            Node { position_type: PositionType::Absolute, left: percent(50.0), top: percent(50.0), width: px(5.0), height: px(5.0), margin: UiRect { left: px(x * 11.0 - 2.5), top: px(y * 11.0 - 2.5), ..default() }, ..default() },
            BackgroundColor(Color::WHITE),
            Visibility::Hidden,
            Pickable::IGNORE,
        ));
    }
}

/// Small deterministic jitter (no RNG resource needed for visuals).
fn jitter(seed: u32) -> Vec3 {
    let f = |k: u32| {
        let x = (seed.wrapping_mul(747_796_405).wrapping_add(k.wrapping_mul(2_891_336_453))) ^ (seed >> 7);
        let x = x.wrapping_mul(277_803_737);
        ((x >> 9) as f32 / (1u32 << 23) as f32) * 2.0 - 1.0
    };
    Vec3::new(f(1), f(2), f(3))
}

#[allow(clippy::too_many_arguments)]
fn burst(commands: &mut Commands, a: &FxAssets, mat: &Handle<StandardMaterial>, at: Vec3, toward: Vec3, n: u32, speed: f32, size: f32, seed: &mut u32) {
    burst_g(commands, a, mat, at, toward, n, speed, size, 9.0, seed);
}

#[allow(clippy::too_many_arguments)]
fn burst_g(commands: &mut Commands, a: &FxAssets, mat: &Handle<StandardMaterial>, at: Vec3, toward: Vec3, n: u32, speed: f32, size: f32, gravity: f32, seed: &mut u32) {
    for _ in 0..n {
        *seed = seed.wrapping_add(1);
        let j = jitter(*seed);
        let vel = (toward * 0.8 + j + Vec3::Y * 0.6).normalize_or_zero() * speed * (0.6 + 0.4 * j.x.abs());
        let life = 0.3 + 0.25 * j.y.abs();
        commands.spawn((Spark { vel, life, max: life, size: size * (0.7 + 0.5 * j.z.abs()), gravity }, Mesh3d(a.mesh.clone()), MeshMaterial3d(mat.clone()), Transform::from_translation(at).with_scale(Vec3::splat(size))));
    }
}

#[allow(clippy::too_many_arguments)]
fn on_hits(
    mut commands: Commands,
    a: Option<Res<FxAssets>>,
    mut hits: MessageReader<HitMsg>,
    mut damaged: MessageReader<Damaged>,
    mut shake: ResMut<Shake>,
    mut mark: ResMut<Marker>,
    movers: Query<(&Mover, Option<&Hurtbox>)>,
    players: Query<(), With<Player>>,
    nodes: Query<(&GlobalTransform, &ResourceNode)>,
    mut creature_rigs: Query<(&mut CreatureRig, &ChildOf)>,
    mut player_rig: Query<&mut PlayerRig>,
    mut seed: Local<u32>,
) {
    let Some(a) = a else { return };
    let pos = |e: Entity| movers.get(e).ok().map(|(m, h)| m.feet + Vec3::Y * h.map_or(0.8, |h| h.height * 0.55));
    for h in hits.read() {
        let Ok((tf, n)) = nodes.get(h.target) else { continue };
        let from = pos(h.attacker).unwrap_or(tf.translation());
        let base = tf.translation();
        let (mat, alt, lift, r) = match n.kind {
            NodeKind::Tree => (&a.wood, Some(&a.leaf), 1.1, 0.45),
            NodeKind::Crystal => (&a.crystal, Some(&a.stone), 0.6, 1.0),
            NodeKind::Ore => (&a.wood, Some(&a.stone), 0.6, 1.0),
            NodeKind::BerryBush => (&a.leaf, None, 0.5, 0.9),
            _ => (&a.stone, None, 0.4, 0.8),
        };
        let toward = (from - base).with_y(0.0).normalize_or(Vec3::Z);
        let at = base + Vec3::Y * lift + toward * r;
        burst(&mut commands, &a, mat, at, toward, 7, 4.0, 0.06, &mut seed);
        if let Some(alt) = alt {
            burst(&mut commands, &a, alt, at + Vec3::Y * 0.3, toward, 3, 2.5, 0.05, &mut seed);
        }
        burst(&mut commands, &a, &a.hit, at, toward, 3, 5.0, 0.035, &mut seed);
        commands.entity(h.target).insert(Wobble { t: 0.0, dir: -toward });
        if players.contains(h.attacker) {
            shake.0 = shake.0.max(0.12);
            mark.t = 0.12;
            mark.kill = false;
        }
    }
    for d in damaged.read() {
        let Some(at) = pos(d.target) else { continue };
        let from = pos(d.attacker).unwrap_or(at);
        let toward = (from - at).with_y(0.0).normalize_or(Vec3::Z);
        let hurt_player = players.contains(d.target);
        let at = at + toward * 0.35;
        let mat = if hurt_player { &a.hurt } else { &a.hit };
        burst(&mut commands, &a, mat, at, toward, if d.killed { 16 } else { 8 }, if d.killed { 6.0 } else { 4.5 }, 0.045, &mut seed);
        if d.killed {
            burst(&mut commands, &a, &a.puff, at, Vec3::ZERO, 8, 1.6, 0.16, &mut seed);
        }
        for (mut rig, parent) in &mut creature_rigs {
            if parent.parent() == d.target {
                rig.punch = 1.0;
            }
        }
        if hurt_player {
            if let Ok(mut r) = player_rig.single_mut() {
                r.hurt = 1.0;
            }
            shake.0 = shake.0.max(0.45);
        } else if players.contains(d.attacker) {
            shake.0 = shake.0.max(if d.killed { 0.35 } else { 0.18 });
            mark.t = if d.killed { 0.3 } else { 0.15 };
            mark.kill = d.killed;
        }
    }
}

/// What working looks like, per job: fire breath (kindling), water drops, wood/stone chips at the target,
/// sparkles at a bench or blueprint, seeds, picked berries.
#[allow(clippy::type_complexity)]
fn work_particles(mut commands: Commands, time: Res<Time>, a: Option<Res<FxAssets>>, workers: Query<(Entity, &crate::base::WorkerTask, &crate::creatures::Brain, &Mover)>, targets: Query<&GlobalTransform>, mut timers: Local<std::collections::HashMap<Entity, f32>>, mut seed: Local<u32>) {
    let Some(a) = a else { return };
    let dt = time.delta_secs();
    for (e, task, brain, m) in &workers {
        if brain.state != crate::creatures::AiState::Work || m.vel.with_y(0.0).length() > 0.2 {
            continue;
        }
        let Some(target) = task.job.and_then(|j| targets.get(j).ok()).map(|t| t.translation()) else { continue };
        let t = timers.entry(e).or_insert(0.0);
        *t -= dt;
        if *t > 0.0 {
            continue;
        }
        *t = 0.3;
        let r = m.capsule.radius;
        let mouth = m.feet + Vec3::Y * r * 1.1 + m.forward() * r;
        let toward = (target - m.feet).with_y(0.0).normalize_or(m.forward());
        match task.kind {
            "kindling" => burst_g(&mut commands, &a, &a.flame, mouth, toward * 2.0, 4, 2.2, 0.06, -2.0, &mut seed),
            "watering" => burst_g(&mut commands, &a, &a.water, mouth + Vec3::Y * 0.2, toward * 1.5, 4, 3.0, 0.04, 9.0, &mut seed),
            "mining" => burst(&mut commands, &a, &a.stone, target.lerp(m.feet, 0.5) + Vec3::Y * 0.5, -toward, 3, 3.0, 0.05, &mut seed),
            "chopping" => burst(&mut commands, &a, &a.wood, target.lerp(m.feet, 0.5) + Vec3::Y * 0.6, -toward, 3, 3.0, 0.05, &mut seed),
            "crafting" | "build" => burst(&mut commands, &a, &a.hit, target + Vec3::Y * 0.9, -toward, 2, 2.5, 0.03, &mut seed),
            "planting" => burst(&mut commands, &a, &a.wood, mouth, toward, 2, 1.5, 0.03, &mut seed),
            "gathering" => burst(&mut commands, &a, &a.berry, target + Vec3::Y * 0.5, -toward, 2, 2.0, 0.05, &mut seed),
            _ => {}
        }
    }
    timers.retain(|e, _| workers.contains(*e));
}

fn sparks(mut commands: Commands, time: Res<Time>, mut q: Query<(Entity, &mut Spark, &mut Transform)>) {
    let dt = time.delta_secs();
    for (e, mut s, mut tf) in &mut q {
        s.life -= dt;
        if s.life <= 0.0 {
            commands.entity(e).despawn();
            continue;
        }
        let g = s.gravity;
        s.vel.y -= g * dt;
        s.vel *= 1.0 - (dt * 2.5).min(0.5);
        tf.translation += s.vel * dt;
        tf.scale = Vec3::splat(s.size * (s.life / s.max).sqrt());
    }
}

fn wobble(mut commands: Commands, time: Res<Time>, mut nodes: Query<(Entity, &mut Wobble, &ResourceNode, &Children)>, mut parts: Query<(&mut Transform, Option<&WobbleBase>)>) {
    let dt = time.delta_secs();
    for (e, mut w, n, children) in &mut nodes {
        w.t += dt;
        let k = (1.0 - w.t / WOBBLE_S).max(0.0);
        let osc = (w.t * 38.0).sin() * k * k;
        for ch in children {
            let Ok((mut tf, base)) = parts.get_mut(*ch) else { continue };
            let Some(base) = base.map(|b| b.0) else {
                commands.entity(*ch).insert(WobbleBase(*tf));
                continue;
            };
            if k <= 0.0 {
                *tf = base;
                commands.entity(*ch).remove::<WobbleBase>();
                continue;
            }
            match n.kind {
                // Trees lean away from the blow about their base.
                NodeKind::Tree => {
                    let axis = Vec3::Y.cross(w.dir).normalize_or(Vec3::X);
                    let rot = Quat::from_axis_angle(axis, osc * 0.06);
                    tf.translation = rot * base.translation;
                    tf.rotation = rot * base.rotation;
                }
                _ => {
                    tf.translation = base.translation + w.dir * osc * 0.06;
                    tf.scale = base.scale * (1.0 + 0.06 * k * k);
                }
            }
        }
        if k <= 0.0 {
            commands.entity(e).remove::<Wobble>();
        }
    }
}

fn marker(time: Res<Time>, mut m: ResMut<Marker>, mut q: Query<(&mut Visibility, &mut BackgroundColor), With<HitMarker>>) {
    m.t = (m.t - time.delta_secs()).max(0.0);
    for (mut v, mut bg) in &mut q {
        *v = if m.t > 0.0 { Visibility::Inherited } else { Visibility::Hidden };
        bg.0 = if m.kill { Color::srgb(1.0, 0.3, 0.25) } else { Color::WHITE };
    }
}

fn decay(time: Res<Time>, mut shake: ResMut<Shake>) {
    shake.0 = (shake.0 - time.delta_secs() * 2.5).max(0.0);
}
