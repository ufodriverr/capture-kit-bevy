//! Gatherable world objects: trees (Wood, Fiber), rocks (Stone), spark crystals, copper ore, berry bushes and
//! pebbles. HP, defense and drops come from world.ron `nodes`. Hitting removes HP and puts the drops
//! straight into the bag.

use avian3d::prelude::*;
use bevy::prelude::*;
use ck_core::consts as ov;
use ck_core::rng::GameRng;

use crate::data::Db;
use crate::interact::{Interact, Interactable};
use crate::inventory::Inventory;
use crate::log::{GameLog, Say};
use crate::player::Player;
use crate::world::{LAKE, Layer, PLATEAU, SEA_LEVEL, Terrain};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NodeKind {
    Tree,
    Rock,
    Crystal,
    /// Copper ore rock.
    Ore,
    BerryBush,
    Pebble,
}

impl NodeKind {
    pub fn name(self) -> &'static str {
        match self {
            NodeKind::Tree => "Tree",
            NodeKind::Rock => "Rock",
            NodeKind::Crystal => "Spark Crystal",
            NodeKind::Ore => "Copper Ore",
            NodeKind::BerryBush => "Berry Bush",
            NodeKind::Pebble => "Pebble",
        }
    }
    fn data(self) -> ck_core::data::NodeKind {
        use ck_core::data::NodeKind as K;
        match self {
            NodeKind::Tree => K::Tree,
            NodeKind::Rock => K::Rock,
            NodeKind::Crystal => K::Crystal,
            NodeKind::Ore => K::Ore,
            NodeKind::BerryBush => K::BerryBush,
            NodeKind::Pebble => K::Pebble,
        }
    }
    /// Which tool mines it at rules.ron `node_tool_rate`.
    pub fn tool(self) -> Tool {
        match self {
            NodeKind::Tree => Tool::Axe,
            NodeKind::Rock | NodeKind::Crystal | NodeKind::Ore => Tool::Pickaxe,
            _ => Tool::None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tool {
    None,
    Axe,
    Pickaxe,
}

impl Tool {
    pub fn of(item: &str) -> Tool {
        let l = item.to_ascii_lowercase();
        if l.ends_with("pickaxe") {
            Tool::Pickaxe
        } else if l.ends_with("axe") {
            Tool::Axe
        } else {
            Tool::None
        }
    }
}

#[derive(Component, Debug, Clone)]
pub struct ResourceNode {
    pub kind: NodeKind,
    pub hp: f32,
    pub max_hp: f32,
    pub defense: f32,
    /// Drop batches already given.
    pub steps: u32,
    /// Items per batch: (item, min, max, chance).
    pub batch: Vec<(String, u32, u32, f32)>,
    pub last_batch: Vec<(String, u32, u32, f32)>,
    /// Game time when it comes back (destroyed or picked).
    pub respawn_at: Option<f32>,
}

impl ResourceNode {
    pub fn alive(&self) -> bool {
        self.respawn_at.is_none()
    }
}

#[derive(Resource, Debug)]
pub struct WorldRng(pub GameRng);

pub struct ResourcesPlugin;

impl Plugin for ResourcesPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, spawn_nodes.after(crate::world::spawn_world)).add_systems(Update, (pick_up, respawn).in_set(crate::Sim::Last));
    }
}

fn node(db: &Db, kind: NodeKind) -> ResourceNode {
    let def = db.node(kind.data());
    let hp = def.map_or(500.0, |d| d.hp);
    let defense = def.map_or(2.0, |d| d.defense);
    let rows = |v: Option<&Vec<(String, u32, u32, f32)>>| v.cloned().unwrap_or_default();
    let (batch, last) = (rows(def.map(|d| &d.batch)), rows(def.map(|d| &d.last)));
    ResourceNode { kind, hp, max_hp: hp, defense, steps: 0, batch, last_batch: last, respawn_at: None }
}

#[derive(Resource, Debug, Clone)]
pub struct NodeAssets {
    pub trunk: Handle<Mesh>,
    pub canopy: Handle<Mesh>,
    pub rock: Handle<Mesh>,
    pub crystal: Handle<Mesh>,
    pub bush: Handle<Mesh>,
    pub berry: Handle<Mesh>,
    pub bark: Handle<StandardMaterial>,
    pub leaves: Handle<StandardMaterial>,
    pub stone: Handle<StandardMaterial>,
    pub crystal_mat: Handle<StandardMaterial>,
    pub copper: Handle<StandardMaterial>,
    pub bush_mat: Handle<StandardMaterial>,
    pub red: Handle<StandardMaterial>,
}

fn spawn_nodes(mut commands: Commands, db: Res<Db>, terrain: Res<Terrain>, mut meshes: ResMut<Assets<Mesh>>, mut mats: ResMut<Assets<StandardMaterial>>) {
    let a = NodeAssets {
        trunk: meshes.add(Cylinder::new(0.28, 4.0)),
        canopy: meshes.add(Sphere::new(1.8).mesh().ico(2).unwrap()),
        rock: meshes.add(Sphere::new(1.0).mesh().ico(1).unwrap()),
        crystal: meshes.add(Cuboid::new(0.25, 0.9, 0.25)),
        bush: meshes.add(Sphere::new(0.7).mesh().ico(2).unwrap()),
        berry: meshes.add(Sphere::new(0.09).mesh().ico(1).unwrap()),
        bark: mats.add(Color::srgb(0.42, 0.28, 0.16)),
        leaves: mats.add(StandardMaterial { base_color: Color::srgb(0.2, 0.48, 0.18), perceptual_roughness: 0.9, ..default() }),
        stone: mats.add(StandardMaterial { base_color: Color::srgb(0.55, 0.55, 0.53), perceptual_roughness: 0.95, ..default() }),
        crystal_mat: mats.add(StandardMaterial { base_color: Color::srgb(0.25, 0.65, 1.0), emissive: LinearRgba::rgb(0.1, 0.4, 1.2), ..default() }),
        copper: mats.add(StandardMaterial { base_color: Color::srgb(0.78, 0.45, 0.22), metallic: 0.6, perceptual_roughness: 0.45, ..default() }),
        bush_mat: mats.add(Color::srgb(0.18, 0.42, 0.2)),
        red: mats.add(StandardMaterial { base_color: Color::srgb(0.85, 0.08, 0.12), emissive: LinearRgba::rgb(0.3, 0.0, 0.0), ..default() }),
    };
    let mut rng = GameRng::new(4242);
    let ok = |x: f32, z: f32, min_slope: f32| {
        let h = terrain.height(x, z);
        let p = Vec2::new(x, z);
        h > SEA_LEVEL + 1.2 && terrain.normal(x, z).y > min_slope && p.length() > 10.0 && p.distance(LAKE) > 38.0 && (p.distance(PLATEAU) < 33.0 || p.distance(PLATEAU) > 44.0) && p.length() < 200.0
    };
    let mut place = |kind: NodeKind, count: usize, center: Vec2, spread: f32, rng: &mut GameRng| {
        let mut placed = 0;
        let mut tries = 0;
        while placed < count && tries < count * 40 {
            tries += 1;
            let ang = rng.range_f32(0.0, std::f32::consts::TAU);
            let r = spread * rng.f32().sqrt();
            let (x, z) = (center.x + ang.cos() * r, center.y + ang.sin() * r);
            if !ok(x, z, 0.85) {
                continue;
            }
            let y = terrain.height(x, z);
            spawn_node(&mut commands, &a, &db, kind, Vec3::new(x, y, z), rng);
            placed += 1;
        }
    };
    // Forests, rocky ground near the plateau, Spark Crystal by the cliffs and the lake, bushes in the meadows.
    place(NodeKind::Tree, 90, Vec2::new(60.0, 40.0), 50.0, &mut rng);
    place(NodeKind::Tree, 70, Vec2::new(-40.0, -60.0), 45.0, &mut rng);
    place(NodeKind::Tree, 50, Vec2::new(-110.0, 50.0), 30.0, &mut rng);
    place(NodeKind::Tree, 40, Vec2::ZERO, 140.0, &mut rng);
    place(NodeKind::Rock, 25, Vec2::new(-60.0, 20.0), 30.0, &mut rng);
    place(NodeKind::Rock, 25, Vec2::ZERO, 120.0, &mut rng);
    place(NodeKind::Crystal, 14, Vec2::new(-58.0, 35.0), 18.0, &mut rng);
    place(NodeKind::Crystal, 10, LAKE + Vec2::new(-20.0, 45.0), 18.0, &mut rng);
    place(NodeKind::Ore, 12, Vec2::new(-45.0, 5.0), 16.0, &mut rng);
    place(NodeKind::Ore, 6, Vec2::new(25.0, -5.0), 12.0, &mut rng);
    place(NodeKind::BerryBush, 30, Vec2::new(15.0, 25.0), 45.0, &mut rng);
    place(NodeKind::Pebble, 50, Vec2::ZERO, 60.0, &mut rng);
    commands.insert_resource(a);
    commands.insert_resource(WorldRng(GameRng::new(777)));
}

pub fn spawn_node(commands: &mut Commands, a: &NodeAssets, db: &Db, kind: NodeKind, at: Vec3, rng: &mut GameRng) {
    let n = node(db, kind);
    let yaw = rng.range_f32(0.0, std::f32::consts::TAU);
    let s = rng.range_f32(0.85, 1.2);
    let tf = Transform::from_translation(at).with_rotation(Quat::from_rotation_y(yaw));
    let name = Name::new(kind.name());
    match kind {
        NodeKind::Tree => {
            commands
                .spawn((name, n, tf, Visibility::default(), RigidBody::Static, Collider::cylinder(0.3, 8.0), CollisionLayers::new(Layer::Solid, LayerMask::ALL)))
                .with_children(|c| {
                    c.spawn((Mesh3d(a.trunk.clone()), MeshMaterial3d(a.bark.clone()), Transform::from_xyz(0.0, 2.0 * s, 0.0).with_scale(Vec3::splat(s))));
                    c.spawn((Mesh3d(a.canopy.clone()), MeshMaterial3d(a.leaves.clone()), Transform::from_xyz(0.0, 4.6 * s, 0.0).with_scale(Vec3::new(1.0, 1.25, 1.0) * s)));
                });
        }
        NodeKind::Rock | NodeKind::Crystal | NodeKind::Ore => {
            let scale = if kind == NodeKind::Rock { Vec3::new(1.5, 1.0, 1.3) * s } else { Vec3::new(1.1, 0.8, 1.0) * s };
            commands
                .spawn((name, n, tf, Visibility::default(), RigidBody::Static, Collider::sphere(1.0), CollisionLayers::new(Layer::Solid, LayerMask::ALL)))
                .insert(Transform { scale, ..tf.with_translation(at + Vec3::Y * 0.25 * scale.y) })
                .with_children(|c| {
                    c.spawn((Mesh3d(a.rock.clone()), MeshMaterial3d(a.stone.clone()), Transform::default()));
                    if kind != NodeKind::Rock {
                        let mat = if kind == NodeKind::Ore { &a.copper } else { &a.crystal_mat };
                        for i in 0..5 {
                            let ang = i as f32 * 1.3;
                            c.spawn((Mesh3d(a.crystal.clone()), MeshMaterial3d(mat.clone()), Transform::from_xyz(ang.cos() * 0.5, 0.75, ang.sin() * 0.5).with_rotation(Quat::from_euler(EulerRot::XYZ, ang.sin() * 0.4, ang, ang.cos() * 0.4))));
                        }
                    }
                });
        }
        NodeKind::BerryBush => {
            commands.spawn((name, n, tf, Visibility::default(), Interactable { prompt: "Pick berries".into(), radius: 1.8 })).with_children(|c| {
                c.spawn((Mesh3d(a.bush.clone()), MeshMaterial3d(a.bush_mat.clone()), Transform::from_xyz(0.0, 0.55, 0.0).with_scale(Vec3::new(1.2, 0.9, 1.2))));
                for i in 0..8 {
                    let ang = i as f32 * 0.8;
                    c.spawn((Mesh3d(a.berry.clone()), MeshMaterial3d(a.red.clone()), Transform::from_xyz(ang.cos() * 0.75, 0.5 + (i % 3) as f32 * 0.2, ang.sin() * 0.75), BerryVisual));
                }
            });
        }
        NodeKind::Pebble => {
            commands.spawn((name, n, tf, Visibility::default(), Interactable { prompt: "Pick up Stone".into(), radius: 1.6 })).with_children(|c| {
                c.spawn((Mesh3d(a.rock.clone()), MeshMaterial3d(a.stone.clone()), Transform::from_xyz(0.0, 0.08, 0.0).with_scale(Vec3::new(0.28, 0.16, 0.22))));
            });
        }
    }
}

#[derive(Component, Debug)]
pub struct BerryVisual;

/// Rolls a batch into the player's bag, logging what arrived ("+3 Wood").
pub fn give_batch(db: &Db, rng: &mut GameRng, inv: &mut Inventory, batch: &[(String, u32, u32, f32)], log: &mut GameLog, time: &Time) {
    for (item, lo, hi, chance) in batch {
        if !rng.chance(f64::from(*chance)) {
            continue;
        }
        let n = rng.range_i32(*lo as i32, *hi as i32) as u32;
        let left = inv.give(db, item, n);
        let name = db.item(item).map_or(item.as_str(), |i| i.name.as_str());
        if left < n {
            log.say(time, format!("+{} {name}", n - left));
        }
        if left > 0 {
            log.say(time, format!("bag full: {left} {name} lost"));
        }
    }
}

/// Damage to a node; gives the batches it earned and hides it when destroyed. Returns the damage dealt.
pub fn damage_node(db: &Db, rng: &mut GameRng, node: &mut ResourceNode, dmg: f32, inv: &mut Inventory, log: &mut GameLog, time: &Time) -> f32 {
    if !node.alive() || node.max_hp <= 0.0 {
        return 0.0;
    }
    let dealt = dmg.min(node.hp);
    node.hp -= dealt;
    let removed = 1.0 - node.hp / node.max_hp;
    let due = (removed / ov::NODE_DROP_STEP + 1e-4).floor() as u32;
    while node.steps < due.min((1.0 / ov::NODE_DROP_STEP) as u32 - 1) {
        node.steps += 1;
        let batch = node.batch.clone();
        give_batch(db, rng, inv, &batch, log, time);
    }
    if node.hp <= 0.0 {
        let last = node.last_batch.clone();
        give_batch(db, rng, inv, &last, log, time);
        node.respawn_at = Some(time.elapsed_secs() + ov::NODE_RESPAWN_S);
        log.say(time, format!("{} destroyed", node.kind.name()));
    }
    dealt
}

fn pick_up(mut ev: MessageReader<Interact>, db: Res<Db>, time: Res<Time>, mut rng: ResMut<WorldRng>, mut log: ResMut<GameLog>, mut nodes: Query<&mut ResourceNode>, mut player: Query<&mut Inventory, With<Player>>) {
    let Ok(mut inv) = player.single_mut() else { return };
    for Interact(e) in ev.read() {
        let Ok(mut n) = nodes.get_mut(*e) else { continue };
        if !n.alive() || !matches!(n.kind, NodeKind::BerryBush | NodeKind::Pebble) {
            continue;
        }
        let batch = n.batch.clone();
        give_batch(&db, &mut rng.0, &mut inv, &batch, &mut log, &time);
        n.respawn_at = Some(time.elapsed_secs() + if n.kind == NodeKind::BerryBush { 60.0 } else { ov::NODE_RESPAWN_S });
    }
}

/// Hidden while gone; restored with full HP when its time comes.
fn respawn(time: Res<Time>, mut q: Query<(&mut ResourceNode, &mut Visibility, Option<&Children>, Has<Collider>, Entity)>, mut berries: Query<&mut Visibility, (With<BerryVisual>, Without<ResourceNode>)>, mut commands: Commands) {
    let now = time.elapsed_secs();
    for (mut n, mut vis, children, has_col, e) in &mut q {
        let gone = n.respawn_at.is_some();
        if let Some(t) = n.respawn_at
            && now >= t
        {
            n.respawn_at = None;
            n.hp = n.max_hp;
            n.steps = 0;
        }
        let alive = n.alive();
        if n.kind == NodeKind::BerryBush {
            for c in children.into_iter().flatten() {
                if let Ok(mut v) = berries.get_mut(*c) {
                    *v = if alive { Visibility::Inherited } else { Visibility::Hidden };
                }
            }
            continue;
        }
        let want = if alive { Visibility::Inherited } else { Visibility::Hidden };
        if *vis != want {
            *vis = want;
        }
        let _ = gone;
        let solid = matches!(n.kind, NodeKind::Tree | NodeKind::Rock | NodeKind::Crystal);
        if alive && solid && !has_col {
            commands.entity(e).insert(if n.kind == NodeKind::Tree { Collider::cylinder(0.3, 8.0) } else { Collider::sphere(1.0) });
        } else if !alive && has_col {
            commands.entity(e).remove::<Collider>();
        }
    }
}
