//! Third-person spring-arm camera with a profile per state (walking, aiming, airborne, crouched
//! aim, sliding), blended over `interp_time`, pulled in by collision, and a wider FOV when sprinting.

use avian3d::prelude::*;
use bevy::prelude::*;

use crate::input::PlayerInput;
use crate::kinematic::{Mover, world_filter};
use crate::player::{Locomotion, Mode, Player};
use crate::tuning::{CameraProfile, MoveTuning};

#[derive(Resource, Debug, Clone)]
pub struct CameraRig {
    pub yaw: f32,
    pub pitch: f32,
    pub arm: f32,
    pub offset: Vec3,
    pub fov_deg: f32,
    pub profile: &'static str,
}

impl Default for CameraRig {
    fn default() -> Self {
        Self { yaw: 0.0, pitch: -0.25, arm: 3.3, offset: Vec3::new(0.8, 0.6, 0.6), fov_deg: 70.0, profile: "Walking" }
    }
}

#[derive(Component, Debug)]
pub struct MainCamera;

pub struct CameraPlugin;

impl Plugin for CameraPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<CameraRig>().add_systems(Startup, spawn_camera).add_systems(Update, look.before(crate::Sim::Player)).add_systems(Update, follow.after(crate::Sim::Last));
    }
}

fn spawn_camera(mut commands: Commands) {
    commands.spawn((
        Name::new("Camera"),
        MainCamera,
        Camera3d::default(),
        Projection::Perspective(PerspectiveProjection { fov: 70f32.to_radians(), near: 0.05, far: 2000.0, ..default() }),
        DistanceFog { color: Color::srgb(0.62, 0.78, 0.92), falloff: FogFalloff::Linear { start: 150.0, end: 420.0 }, ..default() },
        Transform::from_xyz(0.0, 10.0, 10.0),
    ));
}

fn look(input: Res<PlayerInput>, mut rig: ResMut<CameraRig>) {
    rig.yaw -= input.look.x;
    rig.pitch = (rig.pitch - input.look.y).clamp(-1.35, 1.2);
}

pub fn follow(time: Res<Time>, t: Res<MoveTuning>, shake: Option<Res<crate::fx::Shake>>, mut rig: ResMut<CameraRig>, sq: SpatialQuery, player: Query<(Entity, &Mover, &Locomotion), With<Player>>, mut cam: Query<(&mut Transform, &mut Projection), With<MainCamera>>) {
    let Ok((e, m, loco)) = player.single() else { return };
    let Ok((mut tr, mut proj)) = cam.single_mut() else { return };
    let c = &t.camera;
    let air = matches!(loco.mode, Mode::Air | Mode::Glide | Mode::Climb { .. } | Mode::ClimbUp { .. });
    let (p, name): (&CameraProfile, &'static str) = match (loco.aiming, air, loco.crouched, loco.mode) {
        (true, true, _, _) => (&c.airborne_aiming, "AirborneAiming"),
        (true, false, true, _) => (&c.crouch_aiming, "CrouchAiming"),
        (true, false, false, _) => (&c.aiming, "Aiming"),
        (false, _, _, Mode::Slide) => (&c.sliding, "Sliding"),
        (false, true, _, _) => (&c.airborne, "Airborne"),
        _ => (&c.walking, "Walking"),
    };
    rig.profile = name;
    // Exponential blend that settles in about `interp_time`.
    let k = 1.0 - (-time.delta_secs() * 6.0 / c.interp_time.max(0.01)).exp();
    rig.arm += (p.arm - rig.arm) * k;
    rig.offset = rig.offset.lerp(p.offset, k);
    let fov = if loco.sprinting { c.fov_sprint_deg } else { c.fov_walk_deg };
    rig.fov_deg += (fov - rig.fov_deg) * k;

    let rot = Quat::from_euler(EulerRot::YXZ, rig.yaw, rig.pitch, 0.0);
    let fwd = rot * Vec3::NEG_Z;
    let right = rot * Vec3::X;
    let up = rot * Vec3::Y;
    let origin = m.feet + Vec3::Y * c.pivot_height;
    // Spring arm: back along the view by the arm length, then the socket offset in view space.
    let desired = origin - fwd * rig.arm + fwd * rig.offset.x + right * rig.offset.y + up * rig.offset.z;
    let to = desired - origin;
    let len = to.length();
    let filter = world_filter(&[e]);
    let mut eye = desired;
    if let Ok(dir) = Dir3::new(to / len.max(1e-4)) {
        let probe = Collider::sphere(0.12);
        let cfg = ShapeCastConfig::from_max_distance(len);
        if let Some(hit) = sq.cast_shape(&probe, origin, Quat::IDENTITY, dir, &cfg, &filter) {
            eye = origin + dir * (hit.distance - 0.05).max(0.2);
        }
    }
    eye.y = eye.y.max(crate::world::SEA_LEVEL - 0.6).max(eye.y);
    // Hit shake (fx.rs): a quick jitter across the view.
    let s = shake.map_or(0.0, |s| s.0 * s.0) * 0.12;
    if s > 0.0 {
        let now = time.elapsed_secs();
        eye += right * (now * 53.0).sin() * s + up * (now * 61.0).cos() * s;
    }
    tr.translation = eye;
    tr.rotation = rot;
    if let Projection::Perspective(pp) = &mut *proj {
        pp.fov = rig.fov_deg.to_radians();
    }
}
