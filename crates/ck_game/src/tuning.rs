//! Player movement and camera numbers in metres and seconds. Stamina costs come from rules.ron
//! (`Db::move_tuning`); the rest are engine feel, tuned here.

use bevy::prelude::*;

#[derive(Resource, Debug, Clone)]
pub struct MoveTuning {
    pub walk_speed: f32,
    pub sprint_speed: f32,
    pub crouch_speed: f32,
    pub aim_walk_multiplier: f32,
    pub max_accel: f32,
    pub braking_walking: f32,
    pub ground_friction: f32,
    pub braking_friction_factor: f32,
    pub air_control: f32,
    pub gravity: f32,
    pub jump_speed: f32,
    pub step_height: f32,
    /// Yaw turn rate when orienting to movement (rad/s).
    pub rotation_rate: f32,
    pub capsule_radius: f32,
    pub capsule_half_height: f32,
    pub sprint_sp: f32,
    pub jump_sp: f32,
    pub glider_sp: f32,
    pub swim_idle_sp: f32,
    pub swim_sp: f32,
    pub swim_dash_sp: f32,
    pub roll_sp: f32,
    pub climb_sp: f32,
    pub slide_start_speed: f32,
    pub slide_max_speed: f32,
    pub slide_add_rate: f32,
    pub slide_flat_decel: f32,
    pub climb_speed: f32,
    pub climb_probe_forward: f32,
    pub climb_probe_upper: f32,
    pub glider_max_speed: f32,
    pub glider_fall_speed: f32,
    pub swim_speed: f32,
    pub swim_dash_speed: f32,
    pub roll_time: f32,
    pub roll_speed: f32,
    pub roll_cooldown: f32,
    pub fall_damage_start: f32,
    pub fall_damage_death: f32,
    pub camera: CameraTuning,
}

#[derive(Debug, Clone)]
pub struct CameraProfile {
    pub arm: f32,
    /// Camera offset from the arm's end (x back along the arm, y right, z up), metres.
    pub offset: Vec3,
}

#[derive(Debug, Clone)]
pub struct CameraTuning {
    pub walking: CameraProfile,
    pub aiming: CameraProfile,
    pub airborne: CameraProfile,
    pub airborne_aiming: CameraProfile,
    pub crouch_aiming: CameraProfile,
    pub sliding: CameraProfile,
    pub interp_time: f32,
    pub fov_walk_deg: f32,
    pub fov_sprint_deg: f32,
    /// Spring arm origin above the feet.
    pub pivot_height: f32,
}

impl Default for MoveTuning {
    fn default() -> Self {
        let p = |arm: f32, x: f32, y: f32, z: f32| CameraProfile { arm, offset: Vec3::new(x, y, z) };
        Self {
            walk_speed: 4.2,
            sprint_speed: 6.0,
            crouch_speed: 2.8,
            aim_walk_multiplier: 0.5,
            max_accel: 22.0,
            braking_walking: 22.0,
            ground_friction: 8.0,
            braking_friction_factor: 2.0,
            air_control: 0.06,
            gravity: 15.0,
            jump_speed: 7.2,
            step_height: 0.45,
            rotation_rate: 12.0,
            capsule_radius: 0.35,
            capsule_half_height: 0.9,
            sprint_sp: 8.0,
            jump_sp: 8.0,
            glider_sp: 9.0,
            swim_idle_sp: 0.0,
            swim_sp: 4.0,
            swim_dash_sp: 10.0,
            roll_sp: 15.0,
            climb_sp: 6.0,
            slide_start_speed: 5.0,
            slide_max_speed: 12.0,
            slide_add_rate: 1.8,
            slide_flat_decel: 4.0,
            climb_speed: 2.2,
            climb_probe_forward: 0.8,
            climb_probe_upper: 0.7,
            glider_max_speed: 7.5,
            glider_fall_speed: 2.2,
            swim_speed: 3.2,
            swim_dash_speed: 4.8,
            roll_time: 0.55,
            roll_speed: 7.5,
            roll_cooldown: 0.7,
            fall_damage_start: 10.0,
            fall_damage_death: 50.0,
            camera: CameraTuning {
                walking: p(3.4, 0.8, 0.6, 0.6),
                aiming: p(1.3, 0.0, 0.42, 0.45),
                airborne: p(3.1, 0.8, 0.6, 0.55),
                airborne_aiming: p(2.0, 0.5, 0.5, 0.6),
                crouch_aiming: p(1.3, 0.0, 0.3, 0.7),
                sliding: p(3.1, 0.8, 0.5, 0.3),
                interp_time: 0.45,
                fov_walk_deg: 70.0,
                fov_sprint_deg: 80.0,
                pivot_height: 0.9,
            },
        }
    }
}
