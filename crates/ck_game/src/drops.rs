//! Items dropped in the world (defeated creatures, dismantled leftovers): a small box that flies to the player
//! and is picked up when they come near; F on it still works.

use bevy::prelude::*;
use ck_core::rng::GameRng;

use crate::data::Db;
use crate::interact::{Interact, Interactable};
use crate::inventory::Inventory;
use crate::log::{GameLog, Say};
use crate::player::Player;
use crate::world::Terrain;

#[derive(Component, Debug, Clone)]
pub struct WorldItem {
    pub item: String,
    pub count: u32,
    pub vel: Vec3,
    pub age: f32,
}

#[derive(Resource, Debug, Clone)]
pub struct DropAssets {
    pub mesh: Handle<Mesh>,
    pub mat: Handle<StandardMaterial>,
}

pub struct DropsPlugin;

impl Plugin for DropsPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, setup).add_systems(Update, (settle, auto_pickup, pick).chain().in_set(crate::Sim::Last));
    }
}

fn setup(mut commands: Commands, mut meshes: ResMut<Assets<Mesh>>, mut mats: ResMut<Assets<StandardMaterial>>) {
    commands.insert_resource(DropAssets { mesh: meshes.add(Cuboid::new(0.25, 0.25, 0.25)), mat: mats.add(StandardMaterial { base_color: Color::srgb(1.0, 0.85, 0.3), emissive: LinearRgba::rgb(0.6, 0.45, 0.1), ..default() }) });
}

pub fn spawn_drop(commands: &mut Commands, a: &DropAssets, db: &Db, item: &str, count: u32, at: Vec3, rng: &mut GameRng) {
    let name = db.item(item).map_or(item.to_owned(), |i| i.name.clone());
    let ang = rng.range_f32(0.0, std::f32::consts::TAU);
    commands.spawn((
        Name::new(format!("Drop {name}")),
        WorldItem { item: item.to_owned(), count, vel: Vec3::new(ang.cos() * 1.5, 3.0, ang.sin() * 1.5), age: 0.0 },
        Interactable { prompt: format!("Pick up {name} x{count}"), radius: 1.6 },
        Mesh3d(a.mesh.clone()),
        MeshMaterial3d(a.mat.clone()),
        Transform::from_translation(at),
    ));
}

fn settle(time: Res<Time>, terrain: Res<Terrain>, mut q: Query<(&mut WorldItem, &mut Transform)>) {
    let dt = time.delta_secs();
    for (mut w, mut tf) in &mut q {
        w.age += dt;
        let floor = terrain.height(tf.translation.x, tf.translation.z) + 0.13;
        if tf.translation.y > floor || w.vel.y > 0.0 {
            w.vel.y -= 9.8 * dt;
            tf.translation += w.vel * dt;
            if tf.translation.y < floor {
                tf.translation.y = floor;
                w.vel = Vec3::ZERO;
            }
        }
        tf.rotate_y(dt * 1.5);
    }
}

/// Near the player: the item flies in and goes into the bag (left on the ground when the bag is full).
fn auto_pickup(
    mut commands: Commands,
    time: Res<Time>,
    db: Res<Db>,
    mut log: ResMut<GameLog>,
    mut q: Query<(Entity, &mut WorldItem, &mut Transform)>,
    mut player: Query<(&mut Inventory, &crate::kinematic::Mover), With<Player>>,
    mut full_said: Local<f32>,
) {
    let Ok((mut inv, m)) = player.single_mut() else { return };
    let now = time.elapsed_secs();
    let chest = m.feet + Vec3::Y * 0.9;
    for (e, mut w, mut tf) in &mut q {
        // Let it pop out first (and not snap back into the bag of whoever just dropped it).
        if w.age < 0.5 {
            continue;
        }
        let d = chest - tf.translation;
        if d.length() > ck_core::consts::AUTO_PICKUP_M {
            continue;
        }
        let name = db.item(&w.item).map_or(w.item.clone(), |i| i.name.clone());
        if d.length() > 0.6 {
            // Fly in (only when it will fit, else it would hover at the player's feet).
            if inv.bag.clone().add(&w.item, 1, &*db.data) == 0 {
                tf.translation += d.normalize() * (ck_core::consts::PICKUP_PULL_SPEED * time.delta_secs()).min(d.length());
                w.vel = Vec3::ZERO;
            } else if now - *full_said > 5.0 {
                *full_said = now;
                log.say(&time, format!("pick up rejected: bag full ({name})"));
            }
            continue;
        }
        let left = inv.give(&db, &w.item, w.count);
        if left < w.count {
            log.say(&time, format!("+{} {name}", w.count - left));
        }
        if left == 0 {
            commands.entity(e).despawn();
        } else {
            w.count = left;
        }
    }
}

fn pick(mut commands: Commands, time: Res<Time>, db: Res<Db>, mut log: ResMut<GameLog>, mut ev: MessageReader<Interact>, q: Query<&WorldItem>, mut player: Query<&mut Inventory, With<Player>>) {
    let Ok(mut inv) = player.single_mut() else { return };
    for Interact(e) in ev.read() {
        let Ok(w) = q.get(*e) else { continue };
        let left = inv.give(&db, &w.item, w.count);
        let name = db.item(&w.item).map_or(w.item.as_str(), |i| i.name.as_str());
        if left == w.count {
            log.say(&time, format!("pick up rejected: bag full ({name})"));
            continue;
        }
        log.say(&time, format!("+{} {name}", w.count - left));
        if left == 0 {
            commands.entity(*e).despawn();
        } else {
            commands.entity(*e).insert(WorldItem { count: left, ..w.clone() });
        }
    }
}
