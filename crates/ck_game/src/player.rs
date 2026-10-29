//! The player: locomotion modes with overlays (crouch, sprint, aim), stamina gates and fall damage.
//! Ground and air velocity use acceleration, friction and braking with the speeds in tuning.rs.

use avian3d::prelude::*;
use bevy::prelude::*;

use crate::camera::CameraRig;
use crate::input::PlayerInput;
use crate::kinematic::{self, Capsule, Mover, turn_toward, world_filter, yaw_of};
use crate::log::{GameLog, Say};
use crate::tuning::MoveTuning;
use crate::world::{SEA_LEVEL, Terrain};

#[derive(Component, Debug)]
pub struct Player;

#[derive(Component, Debug, Clone)]
pub struct Health {
    pub hp: f32,
    pub max: f32,
}

#[derive(Component, Debug, Clone)]
pub struct Stamina {
    pub sp: f32,
    pub max: f32,
    /// Seconds until regen resumes.
    pub delay: f32,
    pub exhausted: bool,
}

impl Stamina {
    /// Spends `amount` if not exhausted; returns whether the action may happen.
    pub fn try_spend(&mut self, amount: f32) -> bool {
        if self.exhausted || self.sp <= 0.0 {
            return false;
        }
        self.spend(amount);
        true
    }

    /// Continuous spending (already allowed); empties into exhaustion.
    pub fn spend(&mut self, amount: f32) {
        self.sp = (self.sp - amount).max(0.0);
        self.delay = ck_core::consts::STAMINA_REGEN_DELAY_S;
        if self.sp <= 0.0 {
            self.exhausted = true;
        }
    }

    pub fn usable(&self) -> bool {
        !self.exhausted && self.sp > 0.0
    }
}

#[derive(Component, Debug, Clone)]
pub struct Hunger {
    pub value: f32,
    pub max: f32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Mode {
    Ground,
    Air,
    Roll { t: f32, dir: Vec3 },
    Slide,
    Climb { normal: Vec3 },
    ClimbUp { t: f32, from: Vec3, to: Vec3 },
    Glide,
    Swim,
}

impl Mode {
    pub fn name(&self) -> &'static str {
        match self {
            Mode::Ground => "Ground",
            Mode::Air => "Air",
            Mode::Roll { .. } => "Roll",
            Mode::Slide => "Slide",
            Mode::Climb { .. } => "Climb",
            Mode::ClimbUp { .. } => "ClimbUp",
            Mode::Glide => "Glide",
            Mode::Swim => "Swim",
        }
    }
}

#[derive(Component, Debug, Clone)]
pub struct Locomotion {
    pub mode: Mode,
    pub crouched: bool,
    pub sprinting: bool,
    pub aiming: bool,
    /// Highest point of the current fall (fall damage origin).
    pub fall_origin: f32,
    pub has_glider: bool,
    /// Debug/script override for the glider.
    pub has_glider_debug: bool,
    pub overweight: bool,
    /// Actions that lock movement (attacking, throwing) set this; seconds left.
    pub action_lock: f32,
    pub last_fall_damage: f32,
    /// Seconds until the next roll.
    pub roll_cooldown: f32,
}

pub struct PlayerPlugin;

impl Plugin for PlayerPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, spawn_player.after(crate::world::spawn_world)).add_systems(Update, (locomotion, stamina_regen, hunger_tick).chain().in_set(crate::Sim::Player));
    }
}

pub fn spawn_player(mut commands: Commands, tuning: Res<MoveTuning>, terrain: Res<Terrain>, data: Res<crate::data::Db>) {
    let p = crate::world::SPAWN;
    let feet = Vec3::new(p.x, terrain.height(p.x, p.y) + 0.05, p.y);
    let cap = Capsule { radius: tuning.capsule_radius, half_height: tuning.capsule_half_height };
    let (hp, sp) = (data.rules().player_hp, data.rules().player_stamina);
    commands
        .spawn((
            Name::new("Player"),
            Player,
            Mover::new(feet, cap, tuning.step_height),
            Locomotion { mode: Mode::Ground, crouched: false, sprinting: false, aiming: false, fall_origin: feet.y, has_glider: false, has_glider_debug: false, overweight: false, action_lock: 0.0, last_fall_damage: 0.0, roll_cooldown: 0.0 },
            Health { hp, max: hp },
            Stamina { sp, max: sp, delay: 0.0, exhausted: false },
            Hunger { value: 100.0, max: 100.0 },
            Transform::from_translation(feet),
            Visibility::default(),
        ));
}

/// Horizontal velocity: accelerate toward `max_speed`; brake with no input or above it. Velocities in m/s.
pub fn calc_velocity(vel: Vec3, accel: Vec3, max_speed: f32, dt: f32, friction: f32, braking_decel: f32, braking_factor: f32) -> Vec3 {
    let mut v = vel.with_y(0.0);
    let zero_accel = accel.length_squared() < 1e-8;
    let over_max = v.length() > max_speed * 1.01;
    if zero_accel || over_max {
        // Braking, in substeps of at most 1/33 s.
        let f = friction * braking_factor;
        let mut remaining = dt;
        while remaining > 1e-6 && v.length_squared() > 0.0 {
            let step = remaining.min(1.0 / 33.0);
            remaining -= step;
            let old = v;
            let rev = -braking_decel * v.normalize_or_zero();
            v += (-f * v + rev) * step;
            if v.dot(old) <= 0.0 {
                v = Vec3::ZERO;
            }
        }
        if v.length() < 0.1 {
            v = Vec3::ZERO;
        }
    }
    if !zero_accel {
        let dir = accel.normalize();
        if !over_max {
            let speed = v.length();
            v -= (v - dir * speed) * (dt * friction).min(1.0);
        }
        let cap = if over_max { v.length() } else { max_speed };
        v += accel * dt;
        v = v.clamp_length_max(cap);
    }
    v
}

pub fn locomotion(
    time: Res<Time>,
    input: Res<PlayerInput>,
    rig: Res<CameraRig>,
    t: Res<MoveTuning>,
    sq: SpatialQuery,
    terrain: Res<Terrain>,
    mut log: ResMut<GameLog>,
    focus: Res<crate::input::UiFocus>,
    mut q: Query<(Entity, &mut Mover, &mut Locomotion, &mut Stamina, &mut Health), With<Player>>,
    mut tf: Query<&mut Transform, With<Player>>,
) {
    let dt = time.delta_secs().min(1.0 / 20.0);
    if dt <= 0.0 {
        return;
    }
    let Ok((e, mut m, mut loco, mut sp, mut hp)) = q.single_mut() else { return };
    let filter = world_filter(&[e]);
    let cam_fwd = kinematic::yaw_forward(rig.yaw);
    let cam_right = Vec3::new(-cam_fwd.z, 0.0, cam_fwd.x);
    let mut wish = cam_right * input.move_axis.x + cam_fwd * input.move_axis.y;
    if wish.length() > 1.0 {
        wish = wish.normalize();
    }
    let locked = loco.action_lock > 0.0;
    loco.action_lock = (loco.action_lock - dt).max(0.0);
    loco.roll_cooldown = (loco.roll_cooldown - dt).max(0.0);
    if locked {
        wish *= 0.0;
    }
    loco.aiming = input.aim;
    if !matches!(loco.mode, Mode::Ground | Mode::Air) {
        loco.sprinting = false;
    }
    let depth = SEA_LEVEL - m.feet.y;
    let prev_mode = loco.mode;

    // Water entry wins over air/ground (not over climbing).
    if depth > t.capsule_half_height * 1.6 && !matches!(loco.mode, Mode::Swim | Mode::Climb { .. } | Mode::ClimbUp { .. }) {
        loco.mode = Mode::Swim;
        loco.crouched = false;
    }

    match loco.mode {
        Mode::Ground | Mode::Air | Mode::Slide => {
            let grounded = m.grounded;
            if loco.mode == Mode::Ground && !grounded {
                loco.mode = Mode::Air;
                loco.fall_origin = m.feet.y;
            }
            if loco.mode == Mode::Air && grounded && m.vel.y <= 0.0 {
                land(&mut loco, &m, &mut hp, &t, &time, &mut log);
                loco.mode = Mode::Ground;
            }
            if loco.mode == Mode::Air {
                loco.fall_origin = loco.fall_origin.max(m.feet.y);
            }
            // Edge-triggered actions.
            if loco.mode == Mode::Ground && !locked {
                if input.crouch_pressed {
                    let speed = m.vel.with_y(0.0).length();
                    if loco.sprinting && speed >= t.slide_start_speed * 0.95 {
                        loco.mode = Mode::Slide;
                        let d = m.vel.with_y(0.0).normalize_or(m.forward());
                        m.vel = d * speed.max(t.slide_start_speed);
                        log.say(&time, "slide started");
                    } else {
                        loco.crouched = !loco.crouched;
                    }
                } else if input.roll_pressed && loco.roll_cooldown > 0.0 {
                    log.say(&time, "roll rejected: cooldown");
                } else if input.roll_pressed {
                    if sp.try_spend(t.roll_sp) {
                        loco.roll_cooldown = t.roll_cooldown + t.roll_time;
                        let dir = if wish.length_squared() > 0.01 { wish.normalize() } else { m.forward() };
                        loco.mode = Mode::Roll { t: 0.0, dir };
                        loco.crouched = false;
                        m.yaw = yaw_of(dir);
                    } else {
                        log.say(&time, "roll rejected: no stamina");
                    }
                } else if input.jump_pressed {
                    if loco.overweight {
                        log.say(&time, "jump rejected: over the weight limit");
                    } else if loco.crouched {
                        loco.crouched = false;
                    } else if sp.try_spend(t.jump_sp) {
                        m.vel.y = t.jump_speed;
                        m.grounded = false;
                        loco.mode = Mode::Air;
                        loco.fall_origin = m.feet.y;
                    } else {
                        log.say(&time, "jump rejected: no stamina");
                    }
                }
            } else if loco.mode == Mode::Slide && (input.jump_pressed || input.crouch_pressed) {
                loco.mode = Mode::Ground;
                if input.jump_pressed && sp.try_spend(t.jump_sp) {
                    m.vel.y = t.jump_speed;
                    m.grounded = false;
                    loco.mode = Mode::Air;
                    loco.fall_origin = m.feet.y;
                }
            } else if loco.mode == Mode::Air && input.jump_pressed && m.vel.y < 1.0 {
                if !loco.has_glider {
                    log.say(&time, "glide rejected: no glider equipped");
                } else if sp.usable() {
                    loco.mode = Mode::Glide;
                    loco.fall_origin = m.feet.y;
                } else {
                    log.say(&time, "glide rejected: no stamina");
                }
            }
        }
        _ => {}
    }

    match loco.mode {
        Mode::Ground | Mode::Air => {
            let air = loco.mode == Mode::Air;
            let moving = wish.length_squared() > 0.01;
            if input.sprint && moving && loco.crouched && !air {
                loco.crouched = false;
            }
            loco.sprinting = input.sprint && moving && !loco.aiming && !loco.crouched && !air && sp.usable() && !locked && !loco.overweight;
            if !air && loco.sprinting {
                sp.spend(t.sprint_sp * dt);
            }
            let max_speed = if loco.overweight {
                t.walk_speed * 0.5
            } else if loco.crouched {
                t.crouch_speed
            } else if loco.sprinting {
                t.sprint_speed
            } else if loco.aiming {
                t.walk_speed * t.aim_walk_multiplier
            } else {
                t.walk_speed
            };
            let (friction, braking, accel) = if air { (0.0, 0.0, wish * t.max_accel * t.air_control) } else { (t.ground_friction, t.braking_walking, wish * t.max_accel) };
            let h = calc_velocity(m.vel, accel, max_speed, dt, friction, braking, t.braking_friction_factor);
            m.vel = Vec3::new(h.x, m.vel.y - t.gravity * dt, h.z);
            if !air && m.vel.y < 0.0 {
                m.vel.y = -0.5;
            }
            face(&mut m, &loco, wish, rig.yaw, t.rotation_rate * dt);
            let snap = if air { 0.0 } else { t.step_height };
            kinematic::move_character(&sq, &terrain, &mut m, dt, snap, &filter);
            // Climb: falling (or jumping) into a steep wall while pressing toward it.
            if air && moving && sp.usable() && let Some(n) = climb_wall(&sq, &m, &t, &filter, wish) {
                loco.mode = Mode::Climb { normal: n };
                loco.sprinting = false;
                m.vel = Vec3::ZERO;
            }
        }
        Mode::Roll { t: rt, dir } => {
            let nt = rt + dt;
            let speed = t.roll_speed * roll_profile(nt / t.roll_time);
            m.vel = Vec3::new(dir.x * speed, m.vel.y - t.gravity * dt, dir.z * speed);
            if m.grounded && m.vel.y < 0.0 {
                m.vel.y = -0.5;
            }
            kinematic::move_character(&sq, &terrain, &mut m, dt, t.step_height, &filter);
            loco.mode = if nt >= t.roll_time { if m.grounded { Mode::Ground } else { Mode::Air } } else { Mode::Roll { t: nt, dir } };
            if loco.mode == Mode::Air {
                loco.fall_origin = m.feet.y;
            }
        }
        Mode::Slide => {
            let n = m.ground_normal;
            let g = Vec3::NEG_Y * t.gravity;
            let along = g - n * g.dot(n);
            let mut v = m.vel.with_y(0.0);
            v += along.with_y(0.0) * t.slide_add_rate * dt;
            let speed = v.length();
            if along.length() < 0.5 {
                v = v.normalize_or_zero() * (speed - t.slide_flat_decel * dt).max(0.0);
            }
            // A little steering.
            if wish.length_squared() > 0.01 {
                let s = v.length();
                v = (v.normalize_or_zero() + wish * 1.5 * dt).normalize_or_zero() * s;
            }
            v = v.clamp_length_max(t.slide_max_speed);
            m.vel = Vec3::new(v.x, -0.5, v.z);
            if v.length_squared() > 0.01 {
                m.yaw = yaw_of(v);
            }
            kinematic::move_character(&sq, &terrain, &mut m, dt, t.step_height, &filter);
            if v.length() < 2.0 {
                loco.mode = Mode::Ground;
                loco.crouched = true;
            } else if !m.grounded {
                loco.mode = Mode::Air;
                loco.fall_origin = m.feet.y;
            }
        }
        Mode::Climb { normal } => {
            let up = (Vec3::Y - normal * normal.y).normalize_or(Vec3::Y);
            let right = up.cross(normal).normalize_or_zero();
            let ax = if locked { Vec2::ZERO } else { input.move_axis };
            let v = (up * ax.y + right * -ax.x) * t.climb_speed;
            let shape = m.capsule.collider();
            if v.length_squared() > 0.0 {
                sp.spend(t.climb_sp * dt);
                let (c, _) = kinematic::slide(&sq, &shape, m.center(), v * dt, &filter);
                m.feet = c - Vec3::Y * m.capsule.half_height;
            }
            m.vel = v;
            m.yaw = yaw_of(-normal.with_y(0.0));
            loco.fall_origin = m.feet.y;
            let reach = m.capsule.radius + t.climb_probe_forward;
            let wall = kinematic::cast_dir(&sq, &shape, m.center(), -normal, reach, &filter);
            if std::env::var("PW_DEBUG_CLIMB").is_ok() {
                info!("climb feet={:.2?} n={:.2?} v={:.2?} wall={wall:?} sp={:.1}", m.feet, normal, v, sp.sp);
            }
            if input.jump_pressed {
                m.vel = normal * 4.0 + Vec3::Y * 3.0;
                loco.mode = Mode::Air;
            } else if input.crouch_pressed || !sp.usable() {
                m.vel = normal * 1.0;
                loco.mode = Mode::Air;
                if !sp.usable() {
                    log.say(&time, "climb ended: out of stamina");
                }
            } else {
                loco.mode = match wall {
                    Some((d, n)) if n.y < m.walkable => {
                        // Hold on: close the gap straight along the normal, never along the slope, so letting
                        // go of the keys stops dead on the wall.
                        let gap = d - kinematic::SKIN * 2.0;
                        if gap.abs() > 0.005 {
                            m.feet -= normal * gap;
                        }
                        let n = normal.lerp(n, (dt * 10.0).min(1.0)).normalize_or(n);
                        // Head clear of the wall while climbing up: pull up over the edge.
                        let head = m.center() + up * t.climb_probe_upper;
                        let clear = Dir3::new(-n).is_ok_and(|d| sq.cast_ray(head, d, reach + 0.3, true, &filter).is_none_or(|h| h.normal.y >= m.walkable));
                        match if ax.y > 0.1 && clear { top_spot(&sq, &terrain, &m, &t, &filter, -n) } else { None } {
                            Some(to) => Mode::ClimbUp { t: 0.0, from: m.feet, to },
                            None => Mode::Climb { normal: n },
                        }
                    }
                    // The surface in front is standable now: step up onto it.
                    Some(_) => match top_spot(&sq, &terrain, &m, &t, &filter, -normal) {
                        Some(to) => Mode::ClimbUp { t: 0.0, from: m.feet, to },
                        None => {
                            m.vel = -normal * 1.5 + Vec3::Y * 1.5;
                            Mode::Air
                        }
                    },
                    // The wall fell away under the hands (a rounded lip): the top is just ahead, or nothing.
                    None => match top_spot(&sq, &terrain, &m, &t, &filter, -normal) {
                        Some(to) => Mode::ClimbUp { t: 0.0, from: m.feet, to },
                        None => Mode::Air,
                    },
                };
                if matches!(loco.mode, Mode::Climb { .. }) && m.vel.y < 0.0 && ground_below(&sq, &m, &filter) {
                    loco.mode = Mode::Ground;
                }
            }
        }
        Mode::ClimbUp { t: ct, from, to } => {
            let nt = ct + dt / 0.6;
            let k = nt.min(1.0);
            // Up first, then forward.
            let up = from.lerp(Vec3::new(from.x, to.y, from.z), (k * 1.6).min(1.0));
            m.feet = up.lerp(to.with_y(up.y), ((k - 0.5) * 2.0).clamp(0.0, 1.0));
            m.vel = Vec3::ZERO;
            loco.fall_origin = m.feet.y;
            if nt >= 1.0 {
                // Settle on whatever is below.
                kinematic::move_character(&sq, &terrain, &mut m, dt, 1.0, &filter);
                loco.mode = if m.grounded { Mode::Ground } else { Mode::Air };
            } else {
                loco.mode = Mode::ClimbUp { t: nt, from, to };
            }
        }
        Mode::Glide => {
            sp.spend(t.glider_sp * dt);
            let target = wish * t.glider_max_speed;
            let mut h = m.vel.with_y(0.0);
            let fwd = if wish.length_squared() > 0.01 { target } else { m.forward() * t.glider_max_speed * 0.6 };
            h = h.lerp(fwd, (dt * 1.5).min(1.0));
            let vy = m.vel.y + (-t.glider_fall_speed - m.vel.y) * (dt * 4.0).min(1.0);
            m.vel = Vec3::new(h.x, vy, h.z);
            if h.length_squared() > 0.1 {
                m.yaw = turn_toward(m.yaw, yaw_of(h), t.rotation_rate * 0.5 * dt);
            }
            kinematic::move_character(&sq, &terrain, &mut m, dt, 0.0, &filter);
            loco.fall_origin = m.feet.y;
            if m.grounded {
                loco.mode = Mode::Ground;
            } else if !input.jump_held || !sp.usable() {
                loco.mode = Mode::Air;
            }
        }
        Mode::Swim => {
            let dash = input.sprint && sp.usable();
            let moving = wish.length_squared() > 0.01;
            let cost = if dash && moving {
                t.swim_dash_sp
            } else if moving {
                t.swim_sp
            } else {
                t.swim_idle_sp
            };
            sp.spend(cost * dt);
            if sp.sp <= 0.0 {
                // Drowning: 5% of max HP per second while stamina is empty.
                hp.hp = (hp.hp - hp.max * 0.05 * dt).max(0.0);
            }
            let speed = if dash { t.swim_dash_speed } else { t.swim_speed };
            let h = calc_velocity(m.vel, wish * t.max_accel * 0.5, speed, dt, 4.0, 4.0, 1.0);
            // Float with the chest at the surface.
            let target_feet = SEA_LEVEL - t.capsule_half_height * 1.35;
            let vy = ((target_feet - m.feet.y) * 3.0).clamp(-2.0, 2.0);
            m.vel = Vec3::new(h.x, vy, h.z);
            face(&mut m, &loco, wish, rig.yaw, t.rotation_rate * dt);
            kinematic::move_character(&sq, &terrain, &mut m, dt, 0.0, &filter);
            loco.fall_origin = m.feet.y;
            if SEA_LEVEL - m.feet.y < t.capsule_half_height * 1.45 && ground_below_by(&sq, &m, &filter, 0.6) {
                loco.mode = Mode::Ground;
            } else if moving && let Some(n) = climb_wall(&sq, &m, &t, &filter, wish) {
                loco.mode = Mode::Climb { normal: n };
            }
        }
    }
    if prev_mode.name() != loco.mode.name() && matches!(loco.mode, Mode::Climb { .. } | Mode::Glide | Mode::Swim | Mode::ClimbUp { .. }) {
        log.say(&time, format!("movement: {} -> {}", prev_mode.name(), loco.mode.name()));
    }

    // Attacking, aiming or throwing turns the player to the camera's view at once (the target is where the
    // crosshair is, not where the character last walked).
    if !focus.0 && (input.attack_pressed || input.throw_held || input.throw_released) && matches!(loco.mode, Mode::Ground | Mode::Air | Mode::Swim) {
        m.yaw = rig.yaw;
    }
    if let Ok(mut tr) = tf.single_mut() {
        tr.translation = m.feet;
        tr.rotation = Quat::from_rotation_y(m.yaw);
    }
}

/// Speed multiplier over the roll (0..1 of `roll_time`): 3 at the start, 1 at half, 0 at the end, linear in
/// between, normalised so the mean is 1 and the roll covers `roll_speed` x `roll_time`.
fn roll_profile(x: f32) -> f32 {
    let x = x.clamp(0.0, 1.0);
    let v = if x < 0.5 { 3.0 - 4.0 * x } else { 2.0 - 2.0 * x };
    // Mean of that shape over [0,1] = (0.5 * (3 + 1) / 2) + (0.5 * (1 + 0) / 2) = 1.25.
    v / 1.25
}

fn face(m: &mut Mover, loco: &Locomotion, wish: Vec3, cam_yaw: f32, step: f32) {
    if loco.aiming {
        m.yaw = turn_toward(m.yaw, cam_yaw, step * 2.0);
    } else if wish.length_squared() > 0.01 {
        m.yaw = turn_toward(m.yaw, yaw_of(wish), step);
    }
}

fn land(loco: &mut Locomotion, m: &Mover, hp: &mut Health, t: &MoveTuning, time: &Time, log: &mut GameLog) {
    let drop = loco.fall_origin - m.feet.y;
    loco.last_fall_damage = 0.0;
    if drop > t.fall_damage_start {
        // Fall damage: max HP x (drop - start) / (death - start).
        let dmg = (hp.max * (drop - t.fall_damage_start) / (t.fall_damage_death - t.fall_damage_start)).floor();
        hp.hp = (hp.hp - dmg).max(0.0);
        loco.last_fall_damage = dmg;
        log.say(time, format!("fall damage {dmg:.0} ({drop:.1} m drop)"));
    }
}

/// Climbable = too steep to stand on (below `walkable`, the same threshold as walking). Starting needs the
/// chest ray toward `wish` to hit such a surface with more of it above (not a knee-high ledge).
fn climb_wall(sq: &SpatialQuery, m: &Mover, t: &MoveTuning, filter: &SpatialQueryFilter, wish: Vec3) -> Option<Vec3> {
    let touching = m.contacts.iter().any(|n| n.y < m.walkable && n.y > -0.2 && n.dot(wish) < -0.3);
    if !touching {
        return None;
    }
    let d = Dir3::new(wish.with_y(0.0).normalize_or_zero()).ok()?;
    let reach = m.capsule.radius + t.climb_probe_forward;
    let chest = sq.cast_ray(m.center(), d, reach, true, filter)?;
    let n = chest.normal;
    if n.y >= m.walkable {
        return None;
    }
    let up = (Vec3::Y - n * n.y).normalize_or(Vec3::Y);
    let upper = sq.cast_ray(m.center() + up * t.climb_probe_upper, Dir3::new(-n).ok()?, reach + 0.3, true, filter)?;
    (upper.normal.y < m.walkable).then_some(n)
}

/// Where the feet land when pulling up over the lip toward `into` (the wall direction): a walkable spot no
/// lower than the feet and within an arm's reach above the head, found by dropping the capsule from above.
fn top_spot(sq: &SpatialQuery, terrain: &Terrain, m: &Mover, t: &MoveTuning, filter: &SpatialQueryFilter, into: Vec3) -> Option<Vec3> {
    let fwd = into.with_y(0.0).normalize_or_zero();
    if fwd == Vec3::ZERO {
        return None;
    }
    let shape = m.capsule.collider();
    let hh = m.capsule.half_height;
    let rise = hh * 2.0 + t.climb_probe_upper;
    for ahead in [m.capsule.radius * 2.0 + 0.2, m.capsule.radius * 3.5 + 0.4] {
        let start = m.feet + fwd * ahead + Vec3::Y * (rise + hh + 0.2);
        if terrain.height(start.x, start.z) > start.y - hh {
            continue; // still inside the cliff up there
        }
        let Some((d, n)) = kinematic::cast_dir(sq, &shape, start, Vec3::NEG_Y, rise + 0.6, filter) else { continue };
        if n.y < m.walkable {
            continue;
        }
        let feet = start - Vec3::Y * ((d - kinematic::SKIN).max(0.0) + hh);
        if feet.y > m.feet.y - 0.3 {
            return Some(feet);
        }
    }
    None
}

fn ground_below(sq: &SpatialQuery, m: &Mover, filter: &SpatialQueryFilter) -> bool {
    ground_below_by(sq, m, filter, 0.15)
}

fn ground_below_by(sq: &SpatialQuery, m: &Mover, filter: &SpatialQueryFilter, dist: f32) -> bool {
    sq.cast_ray(m.feet + Vec3::Y * 0.1, Dir3::NEG_Y, dist + 0.1, true, filter).is_some_and(|h| h.normal.y >= m.walkable)
}

fn stamina_regen(time: Res<Time>, mut q: Query<(&mut Stamina, &Locomotion), With<Player>>) {
    let dt = time.delta_secs();
    for (mut s, loco) in &mut q {
        let spending = loco.sprinting || matches!(loco.mode, Mode::Glide | Mode::Swim | Mode::Climb { .. });
        if spending {
            continue;
        }
        if s.delay > 0.0 {
            s.delay -= dt;
            continue;
        }
        s.sp = (s.sp + ck_core::consts::STAMINA_REGEN_PER_S * dt).min(s.max);
        if s.exhausted && s.sp >= s.max * ck_core::consts::STAMINA_EXHAUST_UNLOCK_FRACTION {
            s.exhausted = false;
        }
    }
}

fn hunger_tick(time: Res<Time>, data: Res<crate::data::Db>, mut q: Query<(&mut Hunger, &mut Health), With<Player>>) {
    let dt = time.delta_secs();
    for (mut h, mut hp) in &mut q {
        h.value = (h.value - data.rules().hunger_rate * dt).max(0.0);
        if h.value <= 0.0 {
            // Starving: rules.ron `starving_hp_percent` of max HP per second.
            hp.hp = (hp.hp - hp.max * data.rules().starving_hp_percent / 100.0 * dt).max(1.0);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::calc_velocity;
    use bevy::prelude::*;

    #[test]
    fn accelerates_to_walk_speed_and_stops() {
        let mut v = Vec3::ZERO;
        for _ in 0..120 {
            v = calc_velocity(v, Vec3::X * 20.48, 4.0, 1.0 / 60.0, 8.0, 20.48, 2.0);
        }
        assert!((v.length() - 4.0).abs() < 0.01, "{v}");
        let mut n = 0;
        while v.length() > 0.0 && n < 600 {
            v = calc_velocity(v, Vec3::ZERO, 4.0, 1.0 / 60.0, 8.0, 20.48, 2.0);
            n += 1;
        }
        assert!(n < 30, "stops within half a second ({n} frames)");
    }
}
