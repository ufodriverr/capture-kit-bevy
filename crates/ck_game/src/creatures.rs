//! Creature actors and the wild AI. Individuals live in `creatures` (ck_core::creature::Roster); an actor points
//! at its individual by id. Wild creatures wander around their herd's home (at least 5 m per move), rest,
//! sleep at night, get curious about the player and fight back when hit. Perception uses the species'
//! `sight`, `sight_angle` and `hearing`. Bodies are built from the species' `model` parts (creatures.ron).

use std::collections::HashMap;

use avian3d::prelude::*;
use bevy::prelude::*;
use ck_core::creature::{CreatureId, Place, Roster};
use ck_core::data::Part;
use ck_core::rng::GameRng;

use crate::combat::{Combatant, Damaged, Faction, Hurtbox};
use crate::data::Db;
use crate::kinematic::{self, Capsule, Mover, turn_toward, world_filter, yaw_of};
use crate::log::{GameLog, Say};
use crate::player::{Health, Locomotion, Player};
use crate::world::{Layer, SEA_LEVEL, Terrain};

#[derive(Resource, Debug, Default)]
pub struct Creatures(pub Roster);

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Role {
    Wild { home: Vec2, spawner: usize },
    Companion,
    Worker,
}

#[derive(Component, Debug, Clone)]
pub struct CreatureActor {
    pub id: CreatureId,
    pub species: String,
    pub role: Role,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum AiState {
    Idle,
    Wander,
    Sleep,
    /// `Special` response: looks at the player for a while (friendly creatures).
    Curious,
    Battle,
    /// Lost its target: walks back home.
    Return,
    Follow,
    Work,
    Eat,
}

impl AiState {
    pub fn name(self) -> &'static str {
        match self {
            AiState::Idle => "Idle",
            AiState::Wander => "Wander",
            AiState::Sleep => "Sleep",
            AiState::Curious => "Curious",
            AiState::Battle => "Battle",
            AiState::Return => "Return",
            AiState::Follow => "Follow",
            AiState::Work => "Work",
            AiState::Eat => "Eat",
        }
    }
}

#[derive(Component, Debug, Clone)]
pub struct Brain {
    pub state: AiState,
    pub timer: f32,
    pub goal: Option<Vec3>,
    pub target: Option<Entity>,
    /// Has noticed the player (no sneak bonus any more).
    pub aware: bool,
    /// Seconds left of going around an obstacle, and which way (+1 left, -1 right).
    pub detour: f32,
    pub detour_sign: f32,
    /// Stuck checks in a row (the side flips only after several).
    pub stuck: u8,
    pub note: String,
}

impl Brain {
    pub fn new(state: AiState) -> Self {
        Self { state, timer: 0.0, goal: None, target: None, aware: false, detour: 0.0, detour_sign: 1.0, stuck: 0, note: String::new() }
    }
}

/// What the AI wants the body to do this frame; `creature_move` applies it.
#[derive(Component, Debug, Clone, Default)]
pub struct Steer {
    pub dir: Vec3,
    pub speed: f32,
    pub face: Option<Vec3>,
    pub jump: bool,
    /// The body is driven by a skill (a roll) this frame.
    pub locked: bool,
    /// Where it is heading (workers): the object there doesn't count as an obstacle.
    pub goal: Option<Vec3>,
}

/// Local avoidance: when the way ahead is blocked, the first clear heading in a fan around the wanted one,
/// trying the side picked last time first, so a creature follows an obstacle's edge to get past it instead of
/// swinging left and right in front of it. The object at the goal (the station it walks to) is ignored.
fn avoid(sq: &SpatialQuery, m: &Mover, wish: Vec3, goal: Option<Vec3>, b: &mut Brain, near: &Query<&GlobalTransform, Without<CreatureActor>>) -> Vec3 {
    let r = m.capsule.radius.clamp(0.15, 0.8);
    let from = m.feet + Vec3::Y * (m.step_height + r + 0.05);
    let range = goal.map_or(f32::MAX, |g| (g - m.feet).with_y(0.0).length());
    let len = (1.0 + r * 2.0).min(range);
    if len < 0.3 {
        return wish;
    }
    let probe = Collider::sphere(r * 0.9);
    let filter = SpatialQueryFilter::from_mask([Layer::Solid]);
    let cfg = ShapeCastConfig::from_max_distance(len);
    let blocked = |d: Vec3| -> bool {
        let Ok(dir) = Dir3::new(d) else { return false };
        sq.cast_shape(&probe, from, Quat::IDENTITY, dir, &cfg, &filter).is_some_and(|h| {
            // The goal's own object is where we are going, not in the way.
            !goal.is_some_and(|g| near.get(h.entity).is_ok_and(|t| t.translation().with_y(0.0).distance(g.with_y(0.0)) < 0.6))
        })
    };
    if !blocked(wish) {
        return wish;
    }
    for k in 1..=7 {
        let a = k as f32 * 0.4;
        for s in [b.detour_sign, -b.detour_sign] {
            let d = Quat::from_rotation_y(a * s) * wish;
            if !blocked(d) {
                b.detour_sign = s;
                return d;
            }
        }
    }
    -wish
}

/// Movement and perception numbers from the species row (m/s, metres).
#[derive(Component, Debug, Clone)]
pub struct CreatureBody {
    pub walk: f32,
    pub run: f32,
    pub accel: f32,
    pub braking: f32,
    pub sight: f32,
    pub sight_cos: f32,
    pub hearing: f32,
    pub nocturnal: bool,
}

/// Unit primitives for creature parts (radius / half extent 1) and each species' part materials.
#[derive(Resource, Debug, Clone)]
pub struct CreatureAssets {
    pub ball: Handle<Mesh>,
    pub cube: Handle<Mesh>,
    pub cyl: Handle<Mesh>,
    pub cone: Handle<Mesh>,
    pub owned_mark: Handle<StandardMaterial>,
    /// Species id -> materials of [body, parts...].
    pub mats: HashMap<String, Vec<Handle<StandardMaterial>>>,
}

impl CreatureAssets {
    fn mesh(&self, shape: &str) -> Handle<Mesh> {
        match shape {
            "cube" => self.cube.clone(),
            "cyl" => self.cyl.clone(),
            "cone" => self.cone.clone(),
            _ => self.ball.clone(),
        }
    }
}

/// A part's transform at body radius `r`, mirrored across x = 0 when `flip`.
fn part_transform(p: &Part, r: f32, flip: bool) -> Transform {
    let s = if flip { -1.0 } else { 1.0 };
    let (rx, ry, rz) = p.rot;
    Transform::from_xyz(p.at.0 * r * s, p.at.1 * r, p.at.2 * r)
        .with_rotation(Quat::from_euler(EulerRot::XYZ, rx.to_radians(), ry.to_radians() * s, rz.to_radians() * s))
        .with_scale(Vec3::new(p.size.0, p.size.1, p.size.2) * r)
}

/// A spot that keeps a herd of wild creatures alive (respawns them).
#[derive(Debug, Clone)]
pub struct Spawner {
    pub species: String,
    pub center: Vec2,
    pub radius: f32,
    pub count: usize,
    pub levels: (u32, u32),
    pub alive: usize,
    pub next_at: f32,
}

#[derive(Resource, Debug)]
pub struct Spawners {
    pub list: Vec<Spawner>,
    pub rng: GameRng,
}

/// Game clock: hour of day from rules.ron `start_hour` and `time_scale` (game minutes per real minute).
#[derive(Resource, Debug, Clone)]
pub struct WorldClock {
    pub minutes: f32,
    pub rate: f32,
    pub night_start: f32,
    pub night_end: f32,
}

impl WorldClock {
    pub fn hour(&self) -> f32 {
        (self.minutes / 60.0) % 24.0
    }
    pub fn is_night(&self) -> bool {
        let h = self.hour();
        if self.night_start > self.night_end { h >= self.night_start || h < self.night_end } else { h >= self.night_start && h < self.night_end }
    }
}

pub struct CreaturesPlugin;

impl Plugin for CreaturesPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Creatures>()
            .add_systems(Startup, setup.after(crate::world::spawn_world))
            .add_systems(Update, (tick_clock, spawn_wild, (wild_ai, crate::companion::companion_ai), crate::skills::run_skills, creature_move, sync_individuals).chain().in_set(crate::Sim::Creatures))
            .add_systems(Update, (react_to_damage, creature_deaths, sync_skill_slots).chain().in_set(crate::Sim::Last));
    }
}

fn setup(mut commands: Commands, db: Res<Db>, mut meshes: ResMut<Assets<Mesh>>, mut mats: ResMut<Assets<StandardMaterial>>) {
    let mut part_mats = HashMap::new();
    for (id, sp) in &db.creatures {
        let list = std::iter::once(&sp.model.body)
            .chain(&sp.model.parts)
            .map(|p| {
                let c = Srgba::hex(&p.color).map_or(Color::WHITE, Color::from);
                mats.add(StandardMaterial { base_color: c, emissive: c.to_linear() * p.glow, perceptual_roughness: 0.85, ..default() })
            })
            .collect();
        part_mats.insert(id.clone(), list);
    }
    commands.insert_resource(CreatureAssets {
        ball: meshes.add(Sphere::new(1.0).mesh().ico(3).unwrap()),
        cube: meshes.add(Cuboid::new(2.0, 2.0, 2.0)),
        cyl: meshes.add(Cylinder::new(1.0, 2.0)),
        cone: meshes.add(Cone { radius: 1.0, height: 2.0 }),
        owned_mark: mats.add(StandardMaterial { base_color: Color::srgb(0.3, 0.7, 1.0), emissive: LinearRgba::rgb(0.2, 0.6, 1.5), ..default() }),
        mats: part_mats,
    });
    // The wild herds of the sample world (world.ron `spawns`).
    let list = db
        .world
        .spawns
        .iter()
        .map(|(species, (x, z), radius, count, levels)| Spawner { species: species.clone(), center: Vec2::new(*x, *z), radius: *radius, count: *count as usize, levels: *levels, alive: 0, next_at: 0.0 })
        .collect();
    commands.insert_resource(Spawners { list, rng: GameRng::new(99) });
    commands.insert_resource(WorldClock {
        minutes: db.rules().start_hour * 60.0,
        rate: db.rules().time_scale,
        night_start: db.rules().night_start,
        night_end: db.rules().night_end,
    });
}

fn tick_clock(time: Res<Time>, mut clock: ResMut<WorldClock>) {
    clock.minutes += time.delta_secs() / 60.0 * clock.rate;
}

/// Body, collision capsule, step height and steepest walkable slope (normal y) for a species.
pub fn creature_body(db: &Db, species: &str) -> (CreatureBody, Capsule, f32, f32) {
    let sp = &db.creatures[species];
    let body = CreatureBody {
        walk: sp.walk_speed,
        run: sp.run_speed,
        accel: 10.0,
        braking: 5.0,
        sight: sp.sight.max(5.0),
        // `sight_angle` is the full cone.
        sight_cos: (sp.sight_angle.max(10.0).to_radians() * 0.5).cos(),
        hearing: sp.hearing,
        nocturnal: sp.nocturnal,
    };
    let cap = Capsule { radius: sp.size * 0.9, half_height: sp.size };
    (body, cap, sp.size.clamp(0.2, 0.45), std::f32::consts::FRAC_1_SQRT_2)
}

/// Spawns the actor for an individual (wild, companion or worker).
pub fn spawn_creature_actor(commands: &mut Commands, db: &Db, roster: &Roster, a: &CreatureAssets, id: CreatureId, at: Vec3, role: Role, yaw: f32) -> Entity {
    let ind = &roster.all[&id];
    let sp = &db.creatures[&ind.species];
    let (body, cap, step, walkable) = creature_body(db, &ind.species);
    let st = ind.stats(db.rules(), sp);
    let mut m = Mover::new(at, cap, step);
    m.walkable = walkable;
    m.yaw = yaw;
    let wild = matches!(role, Role::Wild { .. });
    let faction = if wild { Faction::Wild } else { Faction::PlayerCreature };
    let r = sp.size;
    let state = match role {
        Role::Wild { .. } => AiState::Idle,
        Role::Companion => AiState::Follow,
        Role::Worker => AiState::Idle,
    };
    let elements = sp.elements.clone();
    commands
        .spawn((
            Name::new(format!("{} #{id}", sp.name)),
            CreatureActor { id, species: ind.species.clone(), role },
            m,
            Brain::new(state),
            Steer::default(),
            body,
            Health { hp: ind.hp.min(st.max_hp), max: st.max_hp },
            Combatant { faction, level: ind.level, melee_attack: st.melee_attack, shot_attack: st.shot_attack, defense: st.defense, elements, receive_rate: 1.0, name: format!("{} Lv{}", ind.name(sp), ind.level) },
            Hurtbox { radius: r * 0.9, height: r * 2.0 },
            crate::skills::SkillSlots::new(&ind.equipped),
            Transform::from_translation(at).with_rotation(Quat::from_rotation_y(yaw)),
            Visibility::default(),
        ))
        .with_children(|c| {
            c.spawn((crate::anim::CreatureRig::new(r), Transform::default(), Visibility::default())).with_children(|c| {
                let mats = &a.mats[&ind.species];
                let body = &sp.model.body;
                c.spawn((Mesh3d(a.mesh(&body.shape)), MeshMaterial3d(mats[0].clone()), part_transform(body, r, false), crate::anim::CreatureSkin(mats[0].clone())));
                for (p, mat) in sp.model.parts.iter().zip(&mats[1..]) {
                    for flip in if p.mirror { &[false, true][..] } else { &[false][..] } {
                        c.spawn((Mesh3d(a.mesh(&p.shape)), MeshMaterial3d(mat.clone()), part_transform(p, r, *flip)));
                    }
                }
                if !wild {
                    c.spawn((Mesh3d(a.ball.clone()), MeshMaterial3d(a.owned_mark.clone()), Transform::from_xyz(0.0, r * 2.25, 0.0).with_scale(Vec3::splat(0.07))));
                }
            });
        })
        .id()
}

fn spawn_wild(mut commands: Commands, time: Res<Time>, db: Res<Db>, terrain: Res<Terrain>, assets: Option<Res<CreatureAssets>>, mut roster: ResMut<Creatures>, mut sp: ResMut<Spawners>, q: Query<&CreatureActor>) {
    let Some(assets) = assets else { return };
    let now = time.elapsed_secs();
    let mut alive = vec![0usize; sp.list.len()];
    for a in &q {
        if let Role::Wild { spawner, .. } = a.role
            && spawner < alive.len()
        {
            alive[spawner] += 1;
        }
    }
    let Spawners { list, rng } = &mut *sp;
    for (i, s) in list.iter_mut().enumerate() {
        s.alive = alive[i];
        if s.alive >= s.count || now < s.next_at {
            continue;
        }
        // Fill instantly at start; afterwards one at a time, every 45 s.
        s.next_at = if now < 1.0 { 0.0 } else { now + 45.0 };
        let ang = rng.range_f32(0.0, std::f32::consts::TAU);
        let r = s.radius * rng.f32().sqrt();
        let (x, z) = (s.center.x + ang.cos() * r, s.center.y + ang.sin() * r);
        let y = terrain.height(x, z);
        if y < SEA_LEVEL + 0.5 {
            continue;
        }
        let level = rng.range_i32(s.levels.0 as i32, s.levels.1 as i32) as u32;
        let id = roster.0.create(&db, &s.species, level, rng);
        spawn_creature_actor(&mut commands, &db, &roster.0, &assets, id, Vec3::new(x, y + 0.1, z), Role::Wild { home: s.center, spawner: i }, rng.range_f32(0.0, std::f32::consts::TAU));
        s.alive += 1;
    }
}

/// Line of sight from eye to eye over the static world.
pub fn line_of_sight(sq: &SpatialQuery, from: Vec3, to: Vec3) -> bool {
    let d = to - from;
    let len = d.length();
    let Ok(dir) = Dir3::new(d / len.max(1e-4)) else { return true };
    sq.cast_ray(from, dir, len, true, &SpatialQueryFilter::from_mask([Layer::Terrain, Layer::Solid])).is_none()
}

/// Does this creature notice the player now? Sight cone + line of sight, or hearing.
fn notices(sq: &SpatialQuery, m: &Mover, b: &CreatureBody, pm: &Mover, ploco: &Locomotion) -> bool {
    let to = pm.feet - m.feet;
    let dist = to.length();
    let crouch = if ploco.crouched { 0.5 } else { 1.0 };
    let fwd = m.forward();
    let in_cone = to.with_y(0.0).normalize_or_zero().dot(fwd) >= b.sight_cos;
    let seen = dist < b.sight * crouch && in_cone && line_of_sight(sq, m.feet + Vec3::Y * 0.5, pm.feet + Vec3::Y * 1.5);
    let moving = pm.vel.with_y(0.0).length() > 0.5;
    let heard = moving && dist < 8.0 * b.hearing * crouch * if ploco.sprinting { 1.6 } else { 1.0 };
    seen || heard
}

fn wild_ai(
    time: Res<Time>,
    clock: Res<WorldClock>,
    sq: SpatialQuery,
    mut spawners: ResMut<Spawners>,
    player: Query<(Entity, &Mover, &Locomotion), With<Player>>,
    mut q: Query<(Entity, &CreatureActor, &Mover, &mut Brain, &mut Steer, &CreatureBody, &crate::skills::SkillSlots, &Health), Without<crate::capture::InOrb>>,
    others: Query<(&Mover, &Health), Without<CreatureActor>>,
    creature_pos: Query<(&Mover, &Health), With<CreatureActor>>,
) {
    let dt = time.delta_secs();
    let Ok((pe, pm, ploco)) = player.single() else { return };
    let rng = &mut spawners.rng;
    for (_e, actor, m, mut b, mut steer, body, slots, hp) in &mut q {
        let Role::Wild { home, .. } = actor.role else { continue };
        if hp.hp <= 0.0 {
            continue;
        }
        b.timer -= dt;
        *steer = Steer { face: None, ..default() };
        if slots.casting.is_some() {
            continue; // the skill drives the body
        }
        let home3 = Vec3::new(home.x, m.feet.y, home.y);
        // Perception.
        if !b.aware && !matches!(b.state, AiState::Sleep) && notices(&sq, m, body, pm, ploco) {
            b.aware = true;
            if matches!(b.state, AiState::Idle | AiState::Wander) {
                // Friendly preset: player discovery -> Special (curious look).
                b.state = AiState::Curious;
                b.timer = rng.range_f32(2.5, 4.5);
            }
        }
        if b.aware && pm.feet.distance(m.feet) > body.sight * 1.6 && b.state != AiState::Battle {
            b.aware = false;
        }
        match b.state {
            AiState::Idle => {
                if !body.nocturnal && clock.is_night() {
                    b.state = AiState::Sleep;
                } else if b.timer <= 0.0 {
                    // Pick a wander goal at least 5 m away, inside the home area.
                    let ang = rng.range_f32(0.0, std::f32::consts::TAU);
                    let r = rng.range_f32(5.0, 12.0);
                    let mut goal = m.feet + Vec3::new(ang.cos() * r, 0.0, ang.sin() * r);
                    if goal.with_y(0.0).distance(home3.with_y(0.0)) > 18.0 {
                        goal = home3 + (goal - home3).with_y(0.0).normalize_or_zero() * 8.0;
                    }
                    b.goal = Some(goal);
                    b.state = AiState::Wander;
                    b.timer = 15.0;
                }
            }
            AiState::Wander => {
                let goal = b.goal.unwrap_or(m.feet);
                let to = (goal - m.feet).with_y(0.0);
                // Arrived within 1 m.
                if to.length() < 1.0 || b.timer <= 0.0 {
                    b.state = AiState::Idle;
                    b.timer = rng.range_f32(3.0, 8.0);
                } else {
                    steer.dir = to.normalize();
                    steer.speed = body.walk;
                }
            }
            AiState::Sleep => {
                if body.nocturnal || !clock.is_night() {
                    b.state = AiState::Idle;
                    b.timer = 2.0;
                }
            }
            AiState::Curious => {
                steer.face = Some(pm.feet - m.feet);
                if b.timer <= 0.0 {
                    b.state = AiState::Idle;
                    b.timer = rng.range_f32(1.0, 3.0);
                }
            }
            AiState::Battle => {
                let target = b.target.filter(|t| *t == pe || creature_pos.get(*t).is_ok());
                let tpos = target.and_then(|t| if t == pe { Some((pm.feet, 1.0)) } else { creature_pos.get(t).ok().map(|(mm, h)| (mm.feet, h.hp)) }).or_else(|| target.and_then(|t| others.get(t).ok().map(|(mm, h)| (mm.feet, h.hp))));
                match tpos {
                    // Give up beyond 40 m from home or when the target is gone.
                    Some((tp, thp)) if thp > 0.0 && tp.with_y(0.0).distance(home3.with_y(0.0)) < 40.0 => {
                        crate::skills::battle_steer(&sq, m, &mut steer, body, slots, tp, &mut b, dt);
                    }
                    _ => {
                        b.state = AiState::Return;
                        b.target = None;
                    }
                }
            }
            AiState::Return => {
                let to = (home3 - m.feet).with_y(0.0);
                if to.length() < 3.0 {
                    b.state = AiState::Idle;
                    b.timer = 2.0;
                } else {
                    steer.dir = to.normalize();
                    steer.speed = body.run * 0.6;
                }
            }
            _ => b.state = AiState::Idle,
        }
    }
}

/// Moves every creature body from its `Steer`: acceleration/braking, gravity, collide-and-slide,
/// simple detours around obstacles, and turning to face.
#[allow(clippy::type_complexity)]
pub fn creature_move(time: Res<Time>, sq: SpatialQuery, terrain: Res<Terrain>, near: Query<&GlobalTransform, Without<CreatureActor>>, mut q: Query<(Entity, &mut Mover, &mut Steer, &CreatureBody, &mut Brain, &mut Transform), (With<CreatureActor>, Without<crate::capture::InOrb>, Without<crate::partner::Shielding>)>) {
    let dt = time.delta_secs().min(0.05);
    let filter = world_filter(&[]);
    for (_e, mut m, mut steer, body, mut b, mut tf) in &mut q {
        if !steer.locked {
            let mut dir = steer.dir;
            if steer.speed > 0.2 && dir.length_squared() > 0.01 {
                dir = avoid(&sq, &m, dir, steer.goal, &mut b, &near);
            }
            if b.detour > 0.0 {
                b.detour -= dt;
                dir = Quat::from_rotation_y(1.1 * b.detour_sign) * dir;
            }
            let accel = dir * body.accel.max(1.0) * if steer.speed > 0.0 { 1.0 } else { 0.0 };
            let h = crate::player::calc_velocity(m.vel, accel, steer.speed.max(0.01), dt, 8.0, body.braking, 2.0);
            m.vel = Vec3::new(h.x, m.vel.y - 9.8 * dt, h.z);
            if steer.jump && m.grounded {
                m.vel.y = 5.0;
            }
            if m.grounded && m.vel.y < 0.0 {
                m.vel.y = -0.5;
            }
        } else {
            m.vel.y -= 9.8 * dt;
            if m.grounded && m.vel.y < 0.0 {
                m.vel.y = -0.5;
            }
        }
        let before = m.feet;
        let snap = if m.grounded && m.vel.y <= 0.0 { m.step_height } else { 0.0 };
        kinematic::move_character(&sq, &terrain, &mut m, dt, snap, &filter);
        // Stuck against something while trying to move: detour sideways for a moment.
        let wanted = steer.dir.length_squared() > 0.01 && steer.speed > 0.2 && !steer.locked;
        // Still stuck (something the probes missed): sidestep the same way as before; switch sides only
        // after several tries, so it doesn't rock back and forth.
        if wanted && b.detour <= 0.0 && (m.feet - before).with_y(0.0).length() < steer.speed * dt * 0.25 {
            b.stuck = b.stuck.saturating_add(1);
            if b.stuck >= 3 {
                b.stuck = 0;
                b.detour_sign = -b.detour_sign;
            }
            b.detour = 0.8;
            steer.jump = true;
        } else if wanted && b.detour <= 0.0 {
            b.stuck = 0;
        }
        // Water: creatures float at the surface and swim slowly.
        if m.feet.y < SEA_LEVEL - 0.4 {
            m.feet.y = SEA_LEVEL - 0.4;
            m.vel.y = 0.0;
        }
        let face = steer.face.or_else(|| (m.vel.with_y(0.0).length() > 0.15).then(|| m.vel.with_y(0.0)));
        if let Some(f) = face.filter(|f| f.length_squared() > 1e-4) {
            m.yaw = turn_toward(m.yaw, yaw_of(f), 6.0 * dt);
        }
        tf.translation = m.feet;
        tf.rotation = Quat::from_rotation_y(m.yaw);
    }
}

/// The actor's HP is authoritative while it exists; the individual mirrors it.
fn sync_individuals(mut roster: ResMut<Creatures>, q: Query<(&CreatureActor, &Health)>) {
    for (a, h) in &q {
        if let Some(p) = roster.0.get_mut(a.id) {
            p.hp = h.hp;
        }
    }
}

/// Wild creatures that get hurt fight back (friendly/warlike presets: damage by the player -> Battle).
fn react_to_damage(mut ev: MessageReader<Damaged>, mut q: Query<(&CreatureActor, &mut Brain)>) {
    for d in ev.read() {
        if let Ok((a, mut b)) = q.get_mut(d.target)
            && matches!(a.role, Role::Wild { .. })
            && !d.killed
        {
            b.aware = true;
            b.state = AiState::Battle;
            b.target = Some(d.attacker);
        }
    }
}

/// HP 0: wild creatures drop their items, give XP and vanish; owned creatures are incapacitated and recalled.
pub fn creature_deaths(
    mut commands: Commands,
    time: Res<Time>,
    db: Res<Db>,
    mut roster: ResMut<Creatures>,
    mut log: ResMut<GameLog>,
    mut rng: ResMut<crate::resources::WorldRng>,
    mut ev: MessageReader<Damaged>,
    q: Query<(&CreatureActor, &Mover)>,
    mut progress: ResMut<crate::progress::Progress>,
    drops: Option<Res<crate::drops::DropAssets>>,
) {
    for d in ev.read() {
        if !d.killed {
            continue;
        }
        let Ok((a, m)) = q.get(d.target) else { continue };
        let Some(ind) = roster.0.get(a.id).cloned() else { continue };
        let sp = &db.creatures[&ind.species];
        match a.role {
            Role::Wild { .. } => {
                if let Some(da) = &drops {
                    for (item, rate, lo, hi) in &sp.drops {
                        if rng.0.chance(f64::from(*rate) / 100.0) {
                            let n = rng.0.range_i32(*lo as i32, *hi as i32) as u32;
                            crate::drops::spawn_drop(&mut commands, da, &db, item, n, m.feet + Vec3::Y * 0.5, &mut rng.0);
                        }
                    }
                }
                let xp = ck_core::creature::defeat_exp(db.rules(), sp, ind.level);
                progress.pending_exp.push((xp, format!("defeated {}", ind.name(sp))));
                log.say(&time, format!("{} Lv{} defeated", ind.name(sp), ind.level));
                roster.0.remove_wild(a.id);
                commands.entity(d.target).despawn();
            }
            Role::Companion | Role::Worker => {
                if let Some(p) = roster.0.get_mut(a.id) {
                    p.incapacitated = true;
                    p.hp = 0.0;
                    if a.role == Role::Worker {
                        p.place = Place::Storage;
                    }
                }
                log.say(&time, format!("{} is incapacitated", ind.name(sp)));
                commands.entity(d.target).despawn();
            }
        }
    }
}

/// A summoned creature uses the skills equipped on its individual (changed in the details screen, or learned).
fn sync_skill_slots(roster: Res<Creatures>, mut q: Query<(&CreatureActor, &mut crate::skills::SkillSlots)>) {
    if !roster.is_changed() {
        return;
    }
    for (a, mut slots) in &mut q {
        if let Some(p) = roster.0.get(a.id)
            && slots.equipped != p.equipped
        {
            slots.equipped = p.equipped.clone();
        }
    }
}
