//! Placeholder bodies that show what is happening. The player is a round body with floating orb hands and
//! feet and the held tool in the right hand; procedural poses cover every movement mode and action (walk and
//! sprint cycles, punches and tool swings timed to the hit frame, bow draw and reload, orb wind-up and
//! throw, hand-over-hand climbing, glider, swim strokes, the roll somersault, working at a blueprint). Creatures
//! hop, squash before a skill and lunge on it. Visual only: nothing here feeds the simulation.

use std::f32::consts::{FRAC_PI_2, PI, TAU};

use bevy::prelude::*;
use ck_core::consts as ov;

use crate::capture::{CapturePreview, OrbBall};
use crate::combat::{Reload, Swing};
use crate::data::Db;
use crate::input::PlayerInput;
use crate::inventory::Inventory;
use crate::kinematic::Mover;
use crate::creatures::{AiState, Brain, CreatureActor};
use crate::partner::ShieldGuard;
use crate::player::{Locomotion, Mode, Player};
use crate::skills::{Phase, SkillSlots};
use crate::tuning::MoveTuning;

/// The player's visual root (child of the player entity): lean, bob, squash and the roll spin.
#[derive(Component, Debug, Default)]
pub struct PlayerRig {
    /// Walk / climb / swim cycle (radians).
    phase: f32,
    /// Feet planted so far on the ground walk cycle (sfx.rs plays a step for each new one).
    pub footfalls: u32,
    /// Alternates the punching hand.
    left_punch: bool,
    swinging: bool,
    /// Seconds since the last orb throw / bow shot / grab (large = idle).
    throw_t: f32,
    shot_t: f32,
    grab_t: f32,
    /// 1 when just hurt, decays (fx.rs sets it).
    pub hurt: f32,
    held: String,
    loaded: u32,
}

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
enum Part {
    HandL,
    HandR,
    FootL,
    FootR,
    Head,
}

/// What the right (tools, orb) or left (bow) hand holds.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
enum Held {
    Right,
    Left,
}

#[derive(Component, Debug)]
struct GliderVis;

/// A creature's visual root (child of the actor): hop, squash, lunge, the hit punch.
#[derive(Component, Debug)]
pub struct CreatureRig {
    r: f32,
    phase: f32,
    /// 1 when just hit, decays (fx.rs sets it).
    pub punch: f32,
}

impl CreatureRig {
    pub fn new(r: f32) -> Self {
        Self { r, phase: 0.0, punch: 0.0 }
    }
}

/// The body part that flashes when hit, and its own material.
#[derive(Component, Debug)]
pub struct CreatureSkin(pub Handle<StandardMaterial>);

#[derive(Resource, Debug, Clone)]
pub struct RigAssets {
    orb: Handle<Mesh>,
    cube: Handle<Mesh>,
    skin: Handle<StandardMaterial>,
    shirt: Handle<StandardMaterial>,
    dark: Handle<StandardMaterial>,
    hair: Handle<StandardMaterial>,
    boot: Handle<StandardMaterial>,
    wood: Handle<StandardMaterial>,
    stone: Handle<StandardMaterial>,
    metal: Handle<StandardMaterial>,
    flame: Handle<StandardMaterial>,
    ball: Handle<StandardMaterial>,
    canvas: Handle<StandardMaterial>,
    string: Handle<StandardMaterial>,
    pub flash: Handle<StandardMaterial>,
}

pub struct AnimPlugin;

impl Plugin for AnimPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, build_player_rig.after(crate::player::spawn_player)).add_systems(Update, (held_item, animate_player, creature_tools, animate_creatures).chain().after(crate::Sim::Last));
    }
}

/// Meshes and materials for the rigs and held items.
fn rig_assets(meshes: &mut Assets<Mesh>, mats: &mut Assets<StandardMaterial>) -> RigAssets {
    let m = |mats: &mut Assets<StandardMaterial>, c: Color| mats.add(StandardMaterial { base_color: c, perceptual_roughness: 0.8, ..default() });
    RigAssets {
        orb: meshes.add(Sphere::new(1.0).mesh().ico(3).unwrap()),
        cube: meshes.add(Cuboid::new(1.0, 1.0, 1.0)),
        skin: m(mats, Color::srgb(0.96, 0.78, 0.62)),
        shirt: m(mats, Color::srgb(0.25, 0.45, 0.75)),
        dark: m(mats, Color::srgb(0.08, 0.08, 0.1)),
        hair: m(mats, Color::srgb(0.35, 0.22, 0.12)),
        boot: m(mats, Color::srgb(0.3, 0.2, 0.12)),
        wood: m(mats, Color::srgb(0.55, 0.38, 0.2)),
        stone: m(mats, Color::srgb(0.55, 0.55, 0.58)),
        metal: mats.add(StandardMaterial { base_color: Color::srgb(0.75, 0.77, 0.8), metallic: 0.8, perceptual_roughness: 0.35, ..default() }),
        flame: mats.add(StandardMaterial { base_color: Color::srgb(1.0, 0.6, 0.15), emissive: LinearRgba::rgb(6.0, 2.5, 0.4), ..default() }),
        ball: mats.add(StandardMaterial { base_color: Color::srgb(0.95, 0.95, 1.0), emissive: LinearRgba::rgb(0.2, 0.6, 1.0), ..default() }),
        canvas: mats.add(StandardMaterial { base_color: Color::srgb(0.85, 0.75, 0.5), cull_mode: None, ..default() }),
        string: m(mats, Color::srgb(0.9, 0.9, 0.85)),
        flash: mats.add(StandardMaterial { base_color: Color::WHITE, emissive: LinearRgba::rgb(4.0, 4.0, 4.0), ..default() }),
    }
}

fn build_player_rig(mut commands: Commands, mut meshes: ResMut<Assets<Mesh>>, mut mats: ResMut<Assets<StandardMaterial>>, q: Query<Entity, With<Player>>) {
    let a = rig_assets(&mut meshes, &mut mats);
    let Ok(pe) = q.single() else { return };
    let part = |mesh: &Handle<Mesh>, mat: &Handle<StandardMaterial>, at: Vec3, scale: Vec3| (Mesh3d(mesh.clone()), MeshMaterial3d(mat.clone()), Transform::from_translation(at).with_scale(scale));
    let root = commands.spawn((Name::new("PlayerRig"), PlayerRig { throw_t: 9.0, shot_t: 9.0, grab_t: 9.0, ..default() }, Transform::default(), Visibility::default(), ChildOf(pe))).id();
    let s = &a.orb;
    // Body: a round torso, head with eyes and hair.
    commands.spawn((part(s, &a.shirt, Vec3::new(0.0, 0.92, 0.0), Vec3::new(0.3, 0.38, 0.24)), ChildOf(root)));
    commands.spawn((part(s, &a.shirt, Vec3::new(0.0, 0.62, 0.0), Vec3::new(0.24, 0.16, 0.2)), ChildOf(root)));
    let head = commands.spawn((part(s, &a.skin, Vec3::new(0.0, 1.5, 0.0), Vec3::splat(0.24)), Part::Head, ChildOf(root))).id();
    // Children of the unit-scale head orb: local units are head radii.
    for x in [-0.34, 0.34] {
        commands.spawn((part(s, &a.dark, Vec3::new(x, 0.12, -0.88), Vec3::new(0.13, 0.18, 0.1)), ChildOf(head)));
    }
    commands.spawn((part(s, &a.hair, Vec3::new(0.0, 0.32, 0.12), Vec3::new(1.04, 0.8, 1.0)), ChildOf(head)));
    for (p, x) in [(Part::HandL, -0.42), (Part::HandR, 0.42)] {
        let hand = commands.spawn((part(s, &a.skin, Vec3::new(x, 0.85, 0.0), Vec3::splat(0.095)), p, ChildOf(root))).id();
        // Held items hang off an unscaled holder so their meshes keep real sizes.
        commands.spawn((Held::of(p), Transform::from_scale(Vec3::splat(1.0 / 0.095)), Visibility::default(), ChildOf(hand)));
    }
    for (p, x) in [(Part::FootL, -0.15), (Part::FootR, 0.15)] {
        commands.spawn((part(s, &a.boot, Vec3::new(x, 0.07, -0.04), Vec3::new(0.1, 0.07, 0.15)), p, ChildOf(root)));
    }
    // The glider sail above the hands (shown while gliding).
    commands.spawn((part(&a.cube, &a.canvas, Vec3::new(0.0, 2.05, -0.05), Vec3::new(1.9, 0.04, 0.85)), GliderVis, Visibility::Hidden, ChildOf(root)));
    commands.insert_resource(a);
}

impl Held {
    fn of(p: Part) -> Self {
        if p == Part::HandL { Held::Left } else { Held::Right }
    }
}

/// What a weapon id looks like.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Look {
    None,
    Axe,
    Pickaxe,
    Club,
    Spear,
    Knife,
    Torch,
    Bow,
    Orb,
    /// Creature work tools: a mallet (crafting, building), a watering can, a crate carried overhead.
    Hammer,
    Can,
    Crate,
}

/// The tool a creature holds (child of its rig), from its worker task.
#[derive(Component, Debug)]
struct CreatureTool {
    look: Look,
}

fn tool_for(brain: &Brain, task: Option<&crate::base::WorkerTask>) -> Look {
    let Some(t) = task else { return Look::None };
    if t.carrying.is_some() {
        return Look::Crate;
    }
    if brain.state != AiState::Work {
        return Look::None;
    }
    match t.kind {
        "crafting" | "build" => Look::Hammer,
        "chopping" => Look::Axe,
        "mining" => Look::Pickaxe,
        "watering" => Look::Can,
        _ => Look::None,
    }
}

/// Gives each creature rig the tool its task needs (none when idle).
fn creature_tools(mut commands: Commands, a: Option<Res<RigAssets>>, actors: Query<(&Brain, Option<&crate::base::WorkerTask>)>, rigs: Query<(Entity, &ChildOf, Option<&Children>), With<CreatureRig>>, mut tools: Query<&mut CreatureTool>) {
    let Some(a) = a else { return };
    for (re, parent, kids) in &rigs {
        let want = actors.get(parent.parent()).map_or(Look::None, |(b, t)| tool_for(b, t));
        match kids.into_iter().flatten().find(|k| tools.contains(**k)) {
            Some(te) => {
                let Ok(mut pt) = tools.get_mut(*te) else { continue };
                if pt.look != want {
                    commands.entity(*te).despawn_related::<Children>();
                    spawn_look(&mut commands, &a, *te, want);
                    pt.look = want;
                }
            }
            None if want != Look::None => {
                let te = commands.spawn((CreatureTool { look: want }, Transform::from_scale(Vec3::splat(0.55)), Visibility::default(), ChildOf(re))).id();
                spawn_look(&mut commands, &a, te, want);
            }
            None => {}
        }
    }
}

fn look_of(db: &Db, item: &str) -> Look {
    if item.is_empty() {
        return Look::None;
    }
    let Some(i) = db.item(item) else { return Look::Club };
    match (i.kind.as_str(), i.sub.as_str()) {
        ("Orb", _) => Look::Orb,
        (_, "Bow") => Look::Bow,
        (_, "Pickaxe") => Look::Pickaxe,
        (_, "Axe") => Look::Axe,
        (_, "Spear") => Look::Spear,
        (_, "Torch") => Look::Torch,
        _ if i.id.contains("knife") => Look::Knife,
        _ => Look::Club,
    }
}

/// Spawns the held-item meshes for `look` under `holder` (grip at the origin, the item pointing +Y).
fn spawn_look(commands: &mut Commands, a: &RigAssets, holder: Entity, look: Look) {
    let b = |at: Vec3, size: Vec3, mat: &Handle<StandardMaterial>| (Mesh3d(a.cube.clone()), MeshMaterial3d(mat.clone()), Transform::from_translation(at).with_scale(size), ChildOf(holder));
    let ball = |at: Vec3, r: f32, mat: &Handle<StandardMaterial>| (Mesh3d(a.orb.clone()), MeshMaterial3d(mat.clone()), Transform::from_translation(at).with_scale(Vec3::splat(r)), ChildOf(holder));
    let handle = |len: f32| b(Vec3::new(0.0, len * 0.5 - 0.08, 0.0), Vec3::new(0.045, len, 0.045), &a.wood);
    match look {
        Look::None => {}
        Look::Axe => {
            commands.spawn(handle(0.62));
            commands.spawn(b(Vec3::new(0.0, 0.46, -0.08), Vec3::new(0.05, 0.16, 0.2), &a.stone));
        }
        Look::Pickaxe => {
            commands.spawn(handle(0.62));
            commands.spawn(b(Vec3::new(0.0, 0.5, 0.0), Vec3::new(0.05, 0.07, 0.5), &a.stone));
        }
        Look::Club => {
            commands.spawn(handle(0.4));
            commands.spawn(b(Vec3::new(0.0, 0.45, 0.0), Vec3::new(0.1, 0.32, 0.1), &a.wood));
        }
        Look::Spear => {
            commands.spawn(b(Vec3::new(0.0, 0.45, 0.0), Vec3::new(0.035, 1.5, 0.035), &a.wood));
            commands.spawn((Mesh3d(a.cube.clone()), MeshMaterial3d(a.stone.clone()), Transform::from_xyz(0.0, 1.27, 0.0).with_rotation(Quat::from_rotation_y(PI / 4.0)).with_scale(Vec3::new(0.07, 0.2, 0.07)), ChildOf(holder)));
        }
        Look::Knife => {
            commands.spawn(handle(0.14));
            commands.spawn(b(Vec3::new(0.0, 0.2, 0.0), Vec3::new(0.015, 0.22, 0.06), &a.metal));
        }
        Look::Torch => {
            commands.spawn(handle(0.5));
            commands.spawn(ball(Vec3::new(0.0, 0.46, 0.0), 0.08, &a.flame));
            commands.spawn((PointLight { color: Color::srgb(1.0, 0.65, 0.3), intensity: 60_000.0, range: 12.0, ..default() }, Transform::from_xyz(0.0, 0.5, 0.0), ChildOf(holder)));
        }
        Look::Bow => {
            // Vertical bow, bulging forward (-Z): grip, two angled limbs, the string behind.
            commands.spawn(b(Vec3::ZERO, Vec3::new(0.04, 0.22, 0.04), &a.wood));
            for s in [-1.0f32, 1.0] {
                commands.spawn((Mesh3d(a.cube.clone()), MeshMaterial3d(a.wood.clone()), Transform::from_xyz(0.0, s * 0.3, 0.06).with_rotation(Quat::from_rotation_x(s * 0.45)).with_scale(Vec3::new(0.03, 0.42, 0.03)), ChildOf(holder)));
            }
            commands.spawn(b(Vec3::new(0.0, 0.0, 0.16), Vec3::new(0.008, 0.98, 0.008), &a.string));
        }
        Look::Orb => {
            commands.spawn(ball(Vec3::new(0.0, 0.05, -0.04), 0.11, &a.ball));
        }
        Look::Hammer => {
            commands.spawn(handle(0.42));
            commands.spawn(b(Vec3::new(0.0, 0.36, 0.0), Vec3::new(0.12, 0.12, 0.22), &a.wood));
        }
        Look::Can => {
            commands.spawn(b(Vec3::new(0.0, 0.1, 0.0), Vec3::new(0.18, 0.2, 0.22), &a.metal));
            commands.spawn((Mesh3d(a.cube.clone()), MeshMaterial3d(a.metal.clone()), Transform::from_xyz(0.0, 0.16, -0.17).with_rotation(Quat::from_rotation_x(-0.8)).with_scale(Vec3::new(0.04, 0.04, 0.2)), ChildOf(holder)));
        }
        Look::Crate => {
            commands.spawn(b(Vec3::ZERO, Vec3::new(0.45, 0.35, 0.45), &a.wood));
        }
    }
}

/// Keeps the hands' held items in sync with the equipped weapon (the orb while aiming a throw).
fn held_item(mut commands: Commands, db: Res<Db>, a: Option<Res<RigAssets>>, preview: Res<CapturePreview>, player: Query<&Inventory, With<Player>>, mut rig: Query<&mut PlayerRig>, holders: Query<(Entity, &Held)>) {
    let (Some(a), Ok(inv), Ok(mut rig)) = (a, player.single(), rig.single_mut()) else { return };
    let weapon = inv.weapon().map_or(String::new(), |w| w.item.clone());
    let want = if preview.aiming { inv.orb.clone() } else { weapon };
    if rig.held == want && !holders.is_empty() {
        return;
    }
    rig.held = want.clone();
    let look = look_of(&db, &want);
    for (e, h) in &holders {
        commands.entity(e).despawn_related::<Children>();
        let side = if look == Look::Bow { Held::Left } else { Held::Right };
        if *h == side {
            spawn_look(&mut commands, &a, e, look);
        }
    }
}

fn ease(x: f32) -> f32 {
    let x = x.clamp(0.0, 1.0);
    x * x * (3.0 - 2.0 * x)
}

/// 0..1 progress of `t` through `[a, b]`.
fn span(t: f32, a: f32, b: f32) -> f32 {
    ((t - a) / (b - a)).clamp(0.0, 1.0)
}

#[derive(Debug, Clone, Copy)]
struct Pose {
    hand_l: Vec3,
    hand_r: Vec3,
    /// Hand rotation about X (tilts a held item forward when negative).
    tilt_l: f32,
    tilt_r: f32,
    foot_l: Vec3,
    foot_r: Vec3,
    /// Body pitch (negative leans forward), twist (yaw) and lift.
    lean: f32,
    twist: f32,
    lift: f32,
    squash: f32,
    /// Somersault angle about the body centre.
    spin: f32,
}

impl Default for Pose {
    fn default() -> Self {
        Self { hand_l: Vec3::new(-0.42, 0.85, 0.0), hand_r: Vec3::new(0.42, 0.85, 0.0), tilt_l: 0.0, tilt_r: -0.5, foot_l: Vec3::new(-0.15, 0.07, -0.04), foot_r: Vec3::new(0.15, 0.07, -0.04), lean: 0.0, twist: 0.0, lift: 0.0, squash: 1.0, spin: 0.0 }
    }
}

#[allow(clippy::too_many_arguments)]
fn animate_player(
    time: Res<Time>,
    db: Res<Db>,
    input: Res<PlayerInput>,
    tuning: Res<MoveTuning>,
    preview: Res<CapturePreview>,
    player: Query<(&Mover, &Locomotion, &Inventory, Option<&Swing>, Option<&Reload>, Has<ShieldGuard>), With<Player>>,
    thrown: Query<(), Added<OrbBall>>,
    arrows: Query<(), Added<crate::combat::Projectile>>,
    mut rigs: Query<(&mut PlayerRig, &mut Transform, &Children)>,
    mut parts: Query<(&Part, &mut Transform), Without<PlayerRig>>,
    mut glider: Query<&mut Visibility, With<GliderVis>>,
) {
    let (Ok((m, loco, inv, swing, reload, shield)), Ok((mut rig, mut root, _))) = (player.single(), rigs.single_mut()) else { return };
    let dt = time.delta_secs();
    let now = time.elapsed_secs();
    let speed = m.vel.with_y(0.0).length();
    rig.throw_t += dt;
    rig.shot_t += dt;
    rig.grab_t += dt;
    rig.hurt = (rig.hurt - dt * 3.0).max(0.0);
    if !thrown.is_empty() {
        rig.throw_t = 0.0;
    }
    let look = look_of(&db, &inv.weapon().map_or(String::new(), |w| w.item.clone()));
    if look == Look::Bow && !arrows.is_empty() && rig.loaded > 0 && inv.loaded == 0 {
        rig.shot_t = 0.0;
    }
    rig.loaded = inv.loaded;
    if input.interact_pressed {
        rig.grab_t = 0.0;
    }
    // A new swing flips the punching hand.
    let swinging = swing.is_some();
    if swinging && !rig.swinging {
        rig.left_punch = !rig.left_punch;
    }
    rig.swinging = swinging;

    let mut p = Pose::default();
    let breathe = (now * 2.2).sin();
    p.hand_l.y += breathe * 0.015;
    p.hand_r.y += breathe * 0.015;
    p.lift = breathe * 0.008;
    match loco.mode {
        Mode::Ground | Mode::Air => {
            let air = loco.mode == Mode::Air;
            let amp = (speed / tuning.walk_speed.max(0.1)).clamp(0.0, 1.0);
            let sprint = ((speed - tuning.walk_speed) / (tuning.sprint_speed - tuning.walk_speed).max(0.1)).clamp(0.0, 1.0);
            if !air {
                let d = speed * dt * 2.4;
                // A foot touches down each time the phase passes pi/2 + k*pi (where its lift ends).
                let k = |x: f32| ((x - FRAC_PI_2) / PI).floor();
                if amp > 0.15 {
                    rig.footfalls += (k(rig.phase + d) - k(rig.phase)).max(0.0) as u32;
                }
                rig.phase = (rig.phase + d) % TAU;
            }
            let s = rig.phase.sin();
            let swing_amp = 0.22 + sprint * 0.2;
            p.hand_l.z += s * swing_amp * amp;
            p.hand_r.z -= s * swing_amp * amp;
            p.hand_l.y += sprint * 0.18 + (s * amp * 0.05).abs();
            p.hand_r.y += sprint * 0.18 + (s * amp * 0.05).abs();
            p.foot_l.z -= s * 0.26 * amp;
            p.foot_r.z += s * 0.26 * amp;
            p.foot_l.y += (rig.phase.cos()).max(0.0) * 0.14 * amp;
            p.foot_r.y += (-rig.phase.cos()).max(0.0) * 0.14 * amp;
            p.lift += (s.abs() * 0.05) * amp;
            p.lean = -0.08 * amp - 0.22 * sprint;
            if air {
                let falling = (-m.vel.y / 6.0).clamp(0.0, 1.0);
                p.hand_l = Vec3::new(-0.5, 1.15 + falling * 0.35, 0.05);
                p.hand_r = Vec3::new(0.5, 1.15 + falling * 0.35, 0.05);
                p.foot_l = Vec3::new(-0.16, 0.22, -0.18);
                p.foot_r = Vec3::new(0.16, 0.05, 0.12);
            }
            if loco.crouched {
                p.squash = 0.72;
                p.lean = -0.25;
            }
        }
        Mode::Roll { t, .. } => {
            let k = (t / tuning.roll_time.max(0.01)).clamp(0.0, 1.0);
            p.spin = -TAU * ease(k);
            p.squash = 0.8;
            p.hand_l = Vec3::new(-0.25, 0.95, -0.25);
            p.hand_r = Vec3::new(0.25, 0.95, -0.25);
            p.foot_l = Vec3::new(-0.15, 0.4, -0.2);
            p.foot_r = Vec3::new(0.15, 0.4, -0.2);
        }
        Mode::Slide => {
            p.lean = 0.45;
            p.hand_l = Vec3::new(-0.5, 0.9, 0.35);
            p.hand_r = Vec3::new(0.5, 1.2, 0.1);
            p.foot_l = Vec3::new(-0.15, 0.15, -0.45);
            p.foot_r = Vec3::new(0.15, 0.08, -0.25);
        }
        Mode::Climb { normal } => {
            rig.phase = (rig.phase + m.vel.length() * dt * 4.0) % TAU;
            let s = rig.phase.sin();
            p.lean = -normal.y.clamp(-0.9, 0.9).asin();
            p.hand_l = Vec3::new(-0.3, 1.55 + s * 0.2, -0.36);
            p.hand_r = Vec3::new(0.3, 1.55 - s * 0.2, -0.36);
            p.tilt_r = -1.2;
            p.foot_l = Vec3::new(-0.17, 0.3 - s * 0.15, -0.3);
            p.foot_r = Vec3::new(0.17, 0.3 + s * 0.15, -0.3);
        }
        Mode::ClimbUp { t, .. } => {
            let k = ease(t);
            p.hand_l = Vec3::new(-0.3, 1.9 - k * 0.9, -0.45 + k * 0.3);
            p.hand_r = Vec3::new(0.3, 1.9 - k * 0.9, -0.45 + k * 0.3);
            p.foot_l = Vec3::new(-0.15, 0.45 * (1.0 - k), -0.2);
            p.foot_r = Vec3::new(0.15, 0.2 * (1.0 - k), -0.1);
            p.lean = -0.5 * (1.0 - k);
        }
        Mode::Glide => {
            p.hand_l = Vec3::new(-0.24, 1.95, -0.05);
            p.hand_r = Vec3::new(0.24, 1.95, -0.05);
            p.lean = -0.3;
            p.foot_l = Vec3::new(-0.14, 0.05, 0.25);
            p.foot_r = Vec3::new(0.14, 0.12, 0.3);
            let sway = (now * 1.5).sin() * 0.04;
            p.twist = sway;
        }
        Mode::Swim => {
            rig.phase = (rig.phase + (0.8 + speed) * dt * 2.5) % TAU;
            let (s, c) = rig.phase.sin_cos();
            p.lean = -1.1;
            let reach = 0.28 + 0.25 * (1.0 - c);
            p.hand_l = Vec3::new(-reach, 1.15 + s * 0.08, -0.45 - 0.25 * c);
            p.hand_r = Vec3::new(reach, 1.15 + s * 0.08, -0.45 - 0.25 * c);
            p.foot_l = Vec3::new(-0.16, 0.1 + s * 0.12, 0.1);
            p.foot_r = Vec3::new(0.16, 0.1 - s * 0.12, 0.1);
        }
    }

    // Actions over the movement pose.
    let upright = matches!(loco.mode, Mode::Ground | Mode::Air);
    if let Some(sw) = swing.filter(|_| upright) {
        let t = sw.t;
        let (wind, hit, end) = (ov::SWING_HIT_AT_S * 0.55, ov::SWING_HIT_AT_S, ov::SWING_TIME_S);
        let up = ease(span(t, 0.0, wind));
        let strike = span(t, wind, hit).powi(2);
        let back = ease(span(t, hit + 0.06, end));
        if look == Look::None {
            // Jab with alternating hands, the other one guarding.
            let side = if rig.left_punch { -1.0 } else { 1.0 };
            let chamber = Vec3::new(side * 0.32, 1.12, 0.12);
            let out = Vec3::new(side * 0.1, 1.2, -0.78);
            let rest = if side < 0.0 { p.hand_l } else { p.hand_r };
            let pos = rest.lerp(chamber, up).lerp(out, strike).lerp(rest, back);
            let guard = Vec3::new(-side * 0.22, 1.25, -0.3);
            if side < 0.0 {
                p.hand_l = pos;
                p.hand_r = p.hand_r.lerp(guard, 1.0 - back);
            } else {
                p.hand_r = pos;
                p.hand_l = p.hand_l.lerp(guard, 1.0 - back);
            }
            p.twist += side * (0.25 * up - 0.55 * strike) * (1.0 - back);
            p.lean -= 0.12 * strike * (1.0 - back);
        } else if look == Look::Spear || look == Look::Knife {
            // Thrust.
            let chamber = Vec3::new(0.3, 1.05, 0.25);
            let out = Vec3::new(0.18, 1.15, -0.85);
            p.hand_r = p.hand_r.lerp(chamber, up).lerp(out, strike).lerp(p.hand_r, back);
            p.tilt_r = -0.5 + (-1.07) * (up.max(strike)) * (1.0 - back);
            p.hand_l = p.hand_l.lerp(Vec3::new(-0.1, 1.05, -0.2), 1.0 - back);
            p.twist -= 0.35 * strike * (1.0 - back);
        } else if look != Look::Bow {
            // Overhead chop: the tool goes back over the shoulder, then down through the target.
            let raised = Vec3::new(0.32, 1.6, 0.12);
            let down = Vec3::new(0.12, 0.95, -0.62);
            p.hand_r = p.hand_r.lerp(raised, up).lerp(down, strike).lerp(p.hand_r, back);
            p.hand_l = p.hand_l.lerp(p.hand_r + Vec3::new(-0.16, -0.08, 0.04), (up + strike).min(1.0) * (1.0 - back));
            p.tilt_r = -0.5 + 1.6 * up * (1.0 - strike) - 1.5 * strike * (1.0 - back);
            p.tilt_l = p.tilt_r;
            p.twist += (0.3 * up - 0.45 * strike) * (1.0 - back);
            p.lean += (0.08 * up - 0.28 * strike) * (1.0 - back);
        }
    }
    if look == Look::Bow && upright {
        if loco.aiming {
            let drawn = inv.loaded > 0 && reload.is_none();
            p.hand_l = Vec3::new(-0.08, 1.42, -0.62);
            p.tilt_l = 0.0;
            let snap = 1.0 - ease(span(rig.shot_t, 0.0, 0.18));
            p.hand_r = if drawn { Vec3::new(0.02, 1.44, -0.12) } else { Vec3::new(0.05, 1.42, -0.5).lerp(Vec3::new(0.3, 1.5, 0.22), 1.0 - snap) };
            if reload.is_some() {
                p.hand_r = Vec3::new(0.28, 1.55, 0.2);
            }
        } else {
            p.hand_l.y -= 0.05;
            p.tilt_l = -0.3;
        }
    }
    if preview.aiming && upright {
        // Wind-up: orb back over the shoulder, the other hand pointing at the target.
        p.hand_r = Vec3::new(0.38, 1.7, 0.28);
        p.hand_l = Vec3::new(-0.25, 1.35, -0.55);
        p.twist += 0.3;
    } else if rig.throw_t < 0.35 {
        let k = ease(rig.throw_t / 0.35);
        let out = Vec3::new(0.15, 1.3, -0.72);
        p.hand_r = out.lerp(p.hand_r, k);
        p.twist += -0.4 * (1.0 - k);
        p.lean -= 0.2 * (1.0 - k);
    }
    if rig.grab_t < 0.3 && upright && swing.is_none() {
        let k = (rig.grab_t / 0.3 * PI).sin();
        p.hand_r = p.hand_r.lerp(Vec3::new(0.18, 0.55, -0.55), k);
        p.lean -= 0.25 * k;
    } else if input.interact_held && upright && speed < 0.3 && swing.is_none() {
        // Working (building, gathering by hand): quick hammer strokes.
        let k = (now * 9.0).sin().abs();
        p.hand_r = Vec3::new(0.2, 1.0 + k * 0.25, -0.5);
        p.hand_l = Vec3::new(-0.2, 0.95, -0.45);
        p.tilt_r = -0.6 - k;
        p.lean -= 0.1;
    }
    if shield {
        p.hand_l = Vec3::new(-0.3, 1.2, -0.45);
    }
    if rig.hurt > 0.0 {
        p.lean += 0.3 * rig.hurt;
        p.squash *= 1.0 - 0.12 * rig.hurt;
    }

    // Apply: the spin turns about the body centre, everything else about the feet.
    let centre = Vec3::Y * 0.9;
    let rot = Quat::from_rotation_y(p.twist) * Quat::from_rotation_x(p.lean);
    let spin = Quat::from_rotation_x(p.spin);
    root.rotation = rot * spin;
    root.translation = Vec3::Y * p.lift + (centre - spin * centre);
    root.scale = Vec3::new(1.0 + (1.0 - p.squash) * 0.4, p.squash, 1.0 + (1.0 - p.squash) * 0.4);
    let k = 1.0 - (-dt * 22.0).exp();
    for (part, mut tf) in &mut parts {
        let (at, tilt) = match part {
            Part::HandL => (p.hand_l, Some(p.tilt_l)),
            Part::HandR => (p.hand_r, Some(p.tilt_r)),
            Part::FootL => (p.foot_l, None),
            Part::FootR => (p.foot_r, None),
            Part::Head => (Vec3::new(0.0, 1.5 + breathe * 0.006, 0.0), None),
        };
        tf.translation = tf.translation.lerp(at, k);
        if let Some(t) = tilt {
            tf.rotation = tf.rotation.slerp(Quat::from_rotation_x(t), k);
        }
    }
    for mut v in &mut glider {
        *v = if loco.mode == Mode::Glide { Visibility::Inherited } else { Visibility::Hidden };
    }
}

fn animate_creatures(
    time: Res<Time>,
    a: Option<Res<RigAssets>>,
    actors: Query<(&Mover, &Brain, Option<&SkillSlots>, &CreatureActor)>,
    mut rigs: Query<(&mut CreatureRig, &mut Transform, &ChildOf, &Children)>,
    mut skin: Query<(&mut MeshMaterial3d<StandardMaterial>, &CreatureSkin)>,
    mut tools: Query<(&CreatureTool, &mut Transform), Without<CreatureRig>>,
) {
    let dt = time.delta_secs();
    let now = time.elapsed_secs();
    for (mut rig, mut tf, parent, children) in &mut rigs {
        let Ok((m, brain, slots, actor)) = actors.get(parent.parent()) else { continue };
        let r = rig.r;
        let speed = m.vel.with_y(0.0).length();
        rig.phase = (rig.phase + speed / r.max(0.1) * dt * 1.1) % TAU;
        rig.punch = (rig.punch - dt * 4.0).max(0.0);
        let amp = (speed / 1.5).clamp(0.0, 1.0);
        let hop = rig.phase.sin().abs();
        let mut lift = hop * r * 0.35 * amp;
        let mut roll = rig.phase.sin() * 0.14 * amp;
        let mut pitch = -0.1 * amp;
        let mut fwd = 0.0;
        // Landing squash between hops, breathing at rest.
        let mut sy = 1.0 - (1.0 - hop) * 0.12 * amp + (now * 2.0 + actor.id as f32).sin() * 0.025 * (1.0 - amp);
        let mut sxz = 1.0 / sy.sqrt();
        if brain.state == AiState::Sleep {
            sy = 0.82 + (now * 0.9).sin() * 0.03;
            sxz = 1.1;
            roll = 0.0;
            lift = 0.0;
        }
        if brain.state == AiState::Work && speed < 0.2 {
            // At a work spot: quick nods and little hops (busy).
            let k = (now * 9.0 + actor.id as f32).sin().abs();
            pitch = -0.35 * k;
            lift = r * 0.12 * k;
            sy = 1.0 - 0.08 * (1.0 - k);
            sxz = 1.0 / sy.sqrt();
        }
        if let Some(c) = slots.and_then(|s| s.casting.as_ref()) {
            match c.phase {
                // Wind-up: crouch and tremble (the tell to dodge).
                Phase::Ready => {
                    let k = (c.t * 2.5).min(1.0);
                    sy = 1.0 - 0.25 * k;
                    sxz = 1.0 + 0.15 * k;
                    roll = (now * 40.0).sin() * 0.05 * k;
                    pitch = 0.15 * k;
                }
                // Release: stretch and lunge.
                Phase::Process => {
                    sy = 1.15;
                    sxz = 0.9;
                    fwd = -r * 0.5;
                    pitch = -0.35;
                }
                Phase::After => {
                    pitch = -0.1;
                }
            }
        }
        // Hit punch: squashed flat, knocked back.
        let p = rig.punch;
        sy *= 1.0 - 0.3 * p;
        sxz *= 1.0 + 0.25 * p;
        fwd += r * 0.25 * p;
        pitch += 0.4 * p;
        tf.translation = Vec3::new(0.0, lift, fwd);
        tf.rotation = Quat::from_rotation_x(pitch) * Quat::from_rotation_z(roll);
        tf.scale = Vec3::new(sxz, sy, sxz);
        // The work tool swings with the nods; the crate rides on top.
        let beat = (now * 9.0 + actor.id as f32).sin().abs();
        for ch in children {
            if let Ok((t, mut ttf)) = tools.get_mut(*ch) {
                match t.look {
                    Look::Crate => {
                        ttf.translation = Vec3::new(0.0, r * 2.05, 0.0);
                        ttf.rotation = Quat::IDENTITY;
                    }
                    Look::Can => {
                        ttf.translation = Vec3::new(r * 0.85, r * 0.9, -r * 0.55);
                        ttf.rotation = Quat::from_rotation_x(-0.5 * beat);
                    }
                    _ => {
                        ttf.translation = Vec3::new(r * 0.9, r * 0.85, -r * 0.35);
                        ttf.rotation = Quat::from_rotation_x(0.7 - 2.0 * beat);
                    }
                }
            }
        }
        if let Some(a) = &a {
            for ch in children {
                if let Ok((mut mat, own)) = skin.get_mut(*ch) {
                    let want = if p > 0.7 { &a.flash } else { &own.0 };
                    if mat.0 != *want {
                        mat.0 = want.clone();
                    }
                }
            }
        }
    }
}
