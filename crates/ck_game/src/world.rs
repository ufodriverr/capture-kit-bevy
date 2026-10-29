//! The kit island: an original heightfield with a meadow at the spawn, forest, a
//! lake, the sea around it and a plateau with steep cliffs for climbing. Units are metres, Y up.

use avian3d::prelude::*;
use bevy::asset::RenderAssetUsages;
use bevy::light::{CascadeShadowConfigBuilder, GlobalAmbientLight};
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::prelude::*;

pub const WORLD_SIZE: f32 = 480.0;
pub const GRID: usize = 241;
pub const SEA_LEVEL: f32 = 0.0;
pub const SPAWN: Vec2 = Vec2::new(0.0, 0.0);
pub const LAKE: Vec2 = Vec2::new(70.0, -80.0);
pub const PLATEAU: Vec2 = Vec2::new(-95.0, 45.0);

#[derive(PhysicsLayer, Default, Clone, Copy, Debug)]
pub enum Layer {
    #[default]
    Terrain,
    /// Trees, rocks, buildings: solid for movement.
    Solid,
}

/// The terrain's heights, for queries that don't need a physics cast (spawning, water depth).
#[derive(Resource, Debug)]
pub struct Terrain {
    pub heights: Vec<f32>,
}

impl Terrain {
    fn step() -> f32 {
        WORLD_SIZE / (GRID - 1) as f32
    }

    /// Bilinear height at a world position (matches the collider's triangles closely).
    pub fn height(&self, x: f32, z: f32) -> f32 {
        let half = WORLD_SIZE / 2.0;
        let fx = ((x + half) / Self::step()).clamp(0.0, (GRID - 1) as f32 - 0.001);
        let fz = ((z + half) / Self::step()).clamp(0.0, (GRID - 1) as f32 - 0.001);
        let (i, j) = (fx as usize, fz as usize);
        let (tx, tz) = (fx - i as f32, fz - j as f32);
        let h = |a: usize, b: usize| self.heights[a * GRID + b];
        let a = h(i, j) * (1.0 - tx) + h(i + 1, j) * tx;
        let b = h(i, j + 1) * (1.0 - tx) + h(i + 1, j + 1) * tx;
        a * (1.0 - tz) + b * tz
    }

    pub fn normal(&self, x: f32, z: f32) -> Vec3 {
        let e = 0.5;
        Vec3::new(self.height(x - e, z) - self.height(x + e, z), 2.0 * e, self.height(x, z - e) - self.height(x, z + e)).normalize()
    }
}

fn hash(x: i32, z: i32, seed: u32) -> f32 {
    let mut h = (x as u32).wrapping_mul(374_761_393) ^ (z as u32).wrapping_mul(668_265_263) ^ seed.wrapping_mul(2_246_822_519);
    h = (h ^ (h >> 13)).wrapping_mul(1_274_126_177);
    h ^= h >> 16;
    (h & 0xffff) as f32 / 65535.0
}

fn value_noise(x: f32, z: f32, seed: u32) -> f32 {
    let (ix, iz) = (x.floor() as i32, z.floor() as i32);
    let (fx, fz) = (x - x.floor(), z - z.floor());
    let s = |t: f32| t * t * (3.0 - 2.0 * t);
    let (u, v) = (s(fx), s(fz));
    let a = hash(ix, iz, seed) * (1.0 - u) + hash(ix + 1, iz, seed) * u;
    let b = hash(ix, iz + 1, seed) * (1.0 - u) + hash(ix + 1, iz + 1, seed) * u;
    (a * (1.0 - v) + b * v) * 2.0 - 1.0
}

fn fbm(x: f32, z: f32, seed: u32) -> f32 {
    let (mut sum, mut amp, mut f) = (0.0, 1.0, 1.0);
    for o in 0..4 {
        sum += value_noise(x * f, z * f, seed + o) * amp;
        amp *= 0.5;
        f *= 2.0;
    }
    sum
}

fn smoothstep(e0: f32, e1: f32, x: f32) -> f32 {
    let t = ((x - e0) / (e1 - e0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// The island's height function.
pub fn island_height(x: f32, z: f32) -> f32 {
    let p = Vec2::new(x, z);
    let edge = p.length() / (WORLD_SIZE * 0.5);
    let mut h = 9.0 + fbm(x / 110.0, z / 110.0, 1) * 9.0 + fbm(x / 40.0, z / 40.0, 3).abs() * 7.0 + fbm(x / 14.0, z / 14.0, 7) * 0.8;
    // Gentle meadow around the spawn.
    let meadow = smoothstep(55.0, 20.0, p.distance(SPAWN));
    h = h * (1.0 - meadow) + 4.0 * meadow;
    // Lake bowl.
    let lake = smoothstep(45.0, 10.0, p.distance(LAKE));
    h = h * (1.0 - lake) + -3.5 * lake;
    // Plateau with steep cliffs (about 70 degrees over 7 m).
    let plat = smoothstep(42.0, 35.0, p.distance(PLATEAU));
    h += plat * 16.0;
    // Sea all around.
    h = h * (1.0 - smoothstep(0.72, 0.95, edge)) + -8.0 * smoothstep(0.72, 0.95, edge);
    h
}

pub struct WorldPlugin;

impl Plugin for WorldPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(ClearColor(Color::srgb(0.53, 0.74, 0.93)))
            .insert_resource(GlobalAmbientLight { color: Color::srgb(0.8, 0.85, 1.0), brightness: 400.0, ..default() })
            .add_systems(Startup, spawn_world)
            .add_systems(Update, day_night);
    }
}

#[derive(Component, Debug)]
pub struct Sun;

/// Visual day/night from the world clock (sun up 05:00-19:00, moonlight at night; the gameplay night is
/// rules.ron `night_start`/`night_end`).
fn day_night(clock: Option<Res<crate::creatures::WorldClock>>, mut clear: ResMut<ClearColor>, mut ambient: ResMut<GlobalAmbientLight>, mut sun: Query<(&mut DirectionalLight, &mut Transform), With<Sun>>, mut fog: Query<&mut DistanceFog>) {
    let Some(clock) = clock else { return };
    let h = clock.hour();
    // 0 at sunrise/sunset, 1 at noon, negative at night.
    let day = (((h - SUNRISE) / (SUNSET - SUNRISE)) * std::f32::consts::PI).sin();
    let light = day.clamp(0.0, 1.0);
    let dusk = (1.0 - (day.abs() * 3.0).min(1.0)).max(0.0);
    let night = Color::srgb(0.03, 0.05, 0.12);
    let noon = Color::srgb(0.53, 0.74, 0.93);
    let orange = Color::srgb(0.95, 0.55, 0.35);
    let sky = night.mix(&noon, light.powf(0.5)).mix(&orange, dusk * 0.5);
    clear.0 = sky;
    ambient.brightness = 60.0 + 340.0 * light.powf(0.5);
    ambient.color = Color::srgb(0.55, 0.6, 0.9).mix(&Color::srgb(0.8, 0.85, 1.0), light);
    for mut f in &mut fog {
        f.color = sky.mix(&Color::srgb(0.62, 0.78, 0.92), light * 0.6);
    }
    for (mut l, mut t) in &mut sun {
        if day > 0.0 {
            // The sun crosses from east to west; lowest at sunrise/sunset.
            let along = (h - SUNRISE) / (SUNSET - SUNRISE);
            l.illuminance = 400.0 + 9000.0 * light;
            l.color = orange.mix(&Color::WHITE, (light * 2.5).min(1.0));
            *t = Transform::from_rotation(Quat::from_euler(EulerRot::YXZ, 1.6 - along * 3.2 + 0.3, -(0.12 + 1.1 * light), 0.0));
        } else {
            // Moonlight.
            l.illuminance = 350.0;
            l.color = Color::srgb(0.6, 0.7, 1.0);
            *t = Transform::from_rotation(Quat::from_euler(EulerRot::YXZ, 0.4, -0.9, 0.0));
        }
    }
}

const SUNRISE: f32 = 5.0;
const SUNSET: f32 = 19.0;

pub fn spawn_world(mut commands: Commands, mut meshes: ResMut<Assets<Mesh>>, mut materials: ResMut<Assets<StandardMaterial>>) {
    let step = Terrain::step();
    let half = WORLD_SIZE / 2.0;
    let mut heights = vec![0.0; GRID * GRID];
    for i in 0..GRID {
        for j in 0..GRID {
            heights[i * GRID + j] = island_height(-half + i as f32 * step, -half + j as f32 * step);
        }
    }
    let terrain = Terrain { heights };

    // Render mesh with vertex colours by height and slope.
    let mut pos = Vec::with_capacity(GRID * GRID);
    let mut nrm = Vec::with_capacity(GRID * GRID);
    let mut col = Vec::with_capacity(GRID * GRID);
    for i in 0..GRID {
        for j in 0..GRID {
            let (x, z) = (-half + i as f32 * step, -half + j as f32 * step);
            let y = terrain.heights[i * GRID + j];
            let n = terrain.normal(x, z);
            pos.push([x, y, z]);
            nrm.push(n.to_array());
            let jitter = hash(i as i32, j as i32, 99) * 0.06;
            let c = if y < SEA_LEVEL + 0.8 {
                [0.82 + jitter, 0.76 + jitter, 0.55, 1.0]
            } else if n.y < 0.72 {
                [0.45 + jitter, 0.42 + jitter, 0.38 + jitter, 1.0]
            } else {
                let g = smoothstep(0.72, 0.95, n.y);
                [0.28 + jitter - 0.1 * g, 0.5 + jitter + 0.1 * g, 0.22, 1.0]
            };
            col.push(Color::srgb(c[0], c[1], c[2]).to_linear().to_f32_array());
        }
    }
    let mut idx = Vec::with_capacity((GRID - 1) * (GRID - 1) * 6);
    for i in 0..GRID - 1 {
        for j in 0..GRID - 1 {
            let a = (i * GRID + j) as u32;
            let b = ((i + 1) * GRID + j) as u32;
            let c = (i * GRID + j + 1) as u32;
            let d = ((i + 1) * GRID + j + 1) as u32;
            idx.extend_from_slice(&[a, c, b, b, c, d]);
        }
    }
    let mut mesh = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default());
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, pos);
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, nrm);
    mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, col);
    mesh.insert_indices(Indices::U32(idx));

    let rows: Vec<Vec<f32>> = (0..GRID).map(|i| terrain.heights[i * GRID..(i + 1) * GRID].to_vec()).collect();
    commands.spawn((
        Name::new("Terrain"),
        Mesh3d(meshes.add(mesh)),
        MeshMaterial3d(materials.add(StandardMaterial { perceptual_roughness: 0.95, ..default() })),
        Transform::default(),
        RigidBody::Static,
        Collider::heightfield(rows, Vec3::new(WORLD_SIZE, 1.0, WORLD_SIZE)),
        CollisionLayers::new(Layer::Terrain, LayerMask::ALL),
    ));
    commands.insert_resource(terrain);

    // Sea.
    commands.spawn((
        Name::new("Water"),
        Mesh3d(meshes.add(Plane3d::default().mesh().size(WORLD_SIZE * 3.0, WORLD_SIZE * 3.0))),
        MeshMaterial3d(materials.add(StandardMaterial { base_color: Color::srgba(0.15, 0.45, 0.7, 0.72), alpha_mode: AlphaMode::Blend, perceptual_roughness: 0.15, ..default() })),
        Transform::from_xyz(0.0, SEA_LEVEL, 0.0),
    ));

    commands.spawn((
        Sun,
        DirectionalLight { illuminance: 9000.0, shadow_maps_enabled: true, ..default() },
        Transform::from_rotation(Quat::from_euler(EulerRot::YXZ, 0.7, -0.9, 0.0)),
        CascadeShadowConfigBuilder { maximum_distance: 120.0, first_cascade_far_bound: 12.0, ..default() }.build(),
    ));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plateau_profile_has_a_cliff() {
        let dir = (SPAWN - PLATEAU).normalize();
        let hs: Vec<(f32, f32)> = (25..=50).map(|r| (r as f32, island_height(PLATEAU.x + dir.x * r as f32, PLATEAU.y + dir.y * r as f32))).collect();
        for (r, h) in &hs {
            println!("r={r} h={h:.2}");
        }
        assert!(hs.first().unwrap().1 - hs.last().unwrap().1 > 10.0);
    }
}
