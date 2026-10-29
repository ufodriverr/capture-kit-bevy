//! Build mode (B -> pick an object): a ghost follows the aim, R rotates, LMB places, Esc/RMB cancels.
//! Materials are taken when the blueprint is placed; the building is finished by work (`work` points: the
//! player holds F, creatures with the craft job and hands help: the `build` task).
//! Placement rules: free space, not too steep, `near_core` buildings inside a camp, at most `max` of a
//! building, camps at least rules.ron `camp_min_distance` apart.

use avian3d::prelude::*;
use bevy::prelude::*;
use ck_core::inventory::Container;

use crate::camera::CameraRig;
use crate::data::Db;
use crate::input::PlayerInput;
use crate::inventory::Inventory;
use crate::kinematic::Mover;
use crate::log::{GameLog, Say};
use crate::player::Player;
use crate::progress::Progress;
use crate::world::Layer;
use ck_core::consts as ov;
use crate::build_models::{Model, model, part_transform, prim_mesh};

/// A placed build object.
#[derive(Component, Debug, Clone)]
pub struct Building {
    pub id: String,
    pub name: String,
}

/// Not finished yet: needs `need` work.
#[derive(Component, Debug, Clone)]
pub struct Construction {
    pub work: f32,
    pub need: f32,
}

/// Item storage (chest, feed box).
#[derive(Component, Debug, Clone)]
pub struct Storage {
    pub items: Container,
    pub food_only: bool,
}

/// A camp centred on its Camp Core.
#[derive(Component, Debug, Clone)]
pub struct BaseCamp {
    pub radius: f32,
    pub level: u32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Shape {
    pub size: Vec3,
    pub solid: bool,
}

/// Footprint of a build object: the bounding box of its model (build_models.rs).
pub fn shape_of(id: &str) -> Shape {
    let m = model(id);
    Shape { size: m.size(), solid: m.solid }
}

/// Spawns a model's parts under `parent` (one visual root, scaled while under construction). With
/// `ghost`, every part uses that material instead of its own colour.
pub fn spawn_model(commands: &mut Commands, meshes: &mut Assets<Mesh>, mats: &mut Assets<StandardMaterial>, parent: Entity, m: &Model, ghost: Option<&Handle<StandardMaterial>>) {
    let mut cache: Vec<(Color, bool, Handle<StandardMaterial>)> = Vec::new();
    let root = commands.spawn((BuildingVisual, Transform::default(), Visibility::default(), ChildOf(parent))).id();
    for p in &m.parts {
        let mat = match ghost {
            Some(g) => g.clone(),
            None => match cache.iter().find(|(c, g, _)| *c == p.color && *g == p.glow) {
                Some((_, _, h)) => h.clone(),
                None => {
                    let emissive = if p.glow { p.color.to_linear() * 2.5 } else { LinearRgba::BLACK };
                    let h = mats.add(StandardMaterial { base_color: p.color, perceptual_roughness: 0.85, emissive, ..default() });
                    cache.push((p.color, p.glow, h.clone()));
                    h
                }
            },
        };
        commands.spawn((Mesh3d(prim_mesh(meshes, p.prim)), MeshMaterial3d(mat), part_transform(p), ChildOf(root)));
    }
    if ghost.is_none()
        && let Some(at) = m.light
    {
        commands.spawn((PointLight { color: Color::srgb(1.0, 0.7, 0.4), intensity: 250_000.0, range: 14.0, ..default() }, Transform::from_translation(at), ChildOf(parent)));
    }
}

/// Build mode state.
#[derive(Resource, Debug, Default)]
pub struct Placing {
    pub id: Option<String>,
    pub rot: f32,
    pub pos: Vec3,
    pub ok: Option<String>,
    pub ghost: Option<Entity>,
    /// The object the ghost was made for (switching objects remakes it).
    pub ghost_for: String,
    /// The ghost's shared material (tinted by the placement rules).
    pub ghost_mat: Option<Handle<StandardMaterial>>,
}

#[derive(Component, Debug)]
pub struct Ghost;

pub struct BuildingPlugin;

impl Plugin for BuildingPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Placing>().init_resource::<Dismantle>().add_systems(Update, (place, construct, dismantle).chain().in_set(crate::Sim::Last).after(crate::menus::open_close));
    }
}

/// Where the aim meets the ground or a foundation (within 12 m).
fn aim_point(sq: &SpatialQuery, rig: &CameraRig, pm: &Mover, exclude: &[Entity]) -> Option<(Vec3, Vec3, Entity)> {
    let rot = Quat::from_euler(EulerRot::YXZ, rig.yaw, rig.pitch.min(-0.05), 0.0);
    let dir = Dir3::new(rot * Vec3::NEG_Z).ok()?;
    let eye = pm.feet + Vec3::Y * 1.6;
    let filter = SpatialQueryFilter::from_mask([Layer::Terrain, Layer::Solid]).with_excluded_entities(exclude.iter().copied());
    sq.cast_ray(eye, dir, 14.0, true, &filter).map(|h| (eye + *dir * h.distance, h.normal, h.entity))
}

fn place(
    mut commands: Commands,
    time: Res<Time>,
    input: Res<PlayerInput>,
    db: Res<Db>,
    rig: Res<CameraRig>,
    sq: SpatialQuery,
    progress: Res<Progress>,
    (mut placing, free): (ResMut<Placing>, Res<crate::crafting::FreeCraft>),
    mut log: ResMut<GameLog>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut mats: ResMut<Assets<StandardMaterial>>,
    mut player: Query<(&Mover, &mut Inventory), With<Player>>,
    buildings: Query<(&Building, &GlobalTransform)>,
    bases: Query<(&BaseCamp, &GlobalTransform)>,
    mut ghosts: Query<&mut Transform, With<Ghost>>,
    names: Query<&Name>,
) {
    let Some(id) = placing.id.clone() else { return };
    let Ok((pm, mut inv)) = player.single_mut() else { return };
    let Some(b) = db.build(&id) else {
        placing.id = None;
        return;
    };
    if input.escape_pressed || input.aim {
        if let Some(g) = placing.ghost.take() {
            commands.entity(g).despawn();
        }
        placing.id = None;
        log.say(&time, "build cancelled");
        return;
    }
    if input.rotate_pressed {
        placing.rot += std::f32::consts::FRAC_PI_4;
    }
    let shape = shape_of(&b.id);
    if placing.ghost_for != b.id
        && let Some(g) = placing.ghost.take()
    {
        commands.entity(g).despawn();
    }
    placing.ghost_for = b.id.clone();
    let ghost = match placing.ghost {
        Some(g) => g,
        None => {
            let mat = mats.add(StandardMaterial { base_color: Color::srgba(0.4, 0.9, 1.0, 0.35), alpha_mode: AlphaMode::Blend, unlit: true, ..default() });
            let g = commands.spawn((Ghost, Transform::default(), Visibility::default())).id();
            spawn_model(&mut commands, &mut meshes, &mut mats, g, &model(&b.id), Some(&mat));
            placing.ghost = Some(g);
            placing.ghost_mat = Some(mat);
            g
        }
    };
    let hit = aim_point(&sq, &rig, pm, &[]);
    let Some((mut at, normal, hit_e)) = hit else {
        placing.ok = Some("aim at the ground (within 14 m)".into());
        return;
    };
    // Snap foundations/walls to the 4 m grid of the foundation under the aim.
    let on_building = buildings.get(hit_e).ok();
    let is_frame = b.category == "Foundation";
    if is_frame && let Some((fb, ftf)) = on_building.filter(|(fb, _)| fb.id.starts_with("wood_foundation")) {
        let _ = fb;
        let c = ftf.translation();
        let rel = at - c;
        let snap = |v: f32| (v / 4.0).round() * 4.0;
        let top = c.y + 0.2;
        at = if b.id == "wood_wall" {
            // On a foundation edge.
            let (dx, dz) = (rel.x, rel.z);
            if dx.abs() > dz.abs() { Vec3::new(c.x + 2.0 * dx.signum(), top, c.z) } else { Vec3::new(c.x, top, c.z + 2.0 * dz.signum()) }
        } else {
            Vec3::new(c.x + snap(rel.x), c.y - 0.2, c.z + snap(rel.z))
        };
        if b.id == "wood_wall" {
            placing.rot = if (at.x - c.x).abs() > 0.1 { std::f32::consts::FRAC_PI_2 } else { 0.0 };
        }
    }
    let rot = Quat::from_rotation_y(placing.rot);
    let center = at + Vec3::Y * shape.size.y * 0.5;
    if let Ok(mut tf) = ghosts.get_mut(ghost) {
        *tf = Transform::from_translation(at).with_rotation(rot);
    }
    placing.pos = at;
    // Rules.
    let base_here = bases.iter().find(|(bc, tf)| tf.translation().with_y(0.0).distance(at.with_y(0.0)) < bc.radius);
    let only_hub = b.near_core;
    let ok = if !progress.build_unlocked(&db, &b.id) && b.id != "camp_core" {
        Err(format!("{} is not unlocked (Technology)", b.name))
    } else if normal.y < ck_core::consts::BUILD_MIN_NORMAL_Y && on_building.is_none() {
        Err("too steep".into())
    } else if only_hub && base_here.is_none() {
        Err("must be built inside a camp (near a Camp Core)".into())
    } else if b.id == "camp_core" && bases.iter().any(|(_, tf)| tf.translation().distance(at) < db.rules().camp_min_distance) {
        Err("too close to another camp".into())
    } else if b.max > 0 && buildings.iter().filter(|(x, _)| x.id == b.id).count() >= b.max as usize {
        Err(format!("you can only have {} {}", b.max, b.name))
    } else if let Some(blocker) = sq.shape_intersections(&Collider::cuboid(shape.size.x * 0.9, (shape.size.y * 0.9).max(0.1), shape.size.z * 0.9), center + Vec3::Y * 0.05, rot, &SpatialQueryFilter::from_mask([Layer::Solid])).first().filter(|_| !(is_frame && b.id == "wood_wall")) {
        Err(format!("blocked by {}", names.get(*blocker).map_or("something", |n| n.as_str())))
    } else if let Some((m, _)) = b.materials.iter().find(|(m, n)| !free.0 && inv.count(m) < *n) {
        let need = b.materials.iter().find(|(x, _)| x == m).map_or(0, |x| x.1);
        Err(format!("not enough {} ({}/{need}; F2 = free mode)", db.item(m).map_or(m.as_str(), |i| i.name.as_str()), inv.count(m)))
    } else {
        Ok(())
    };
    placing.ok = ok.clone().err();
    if let Some(h) = &placing.ghost_mat
        && let Some(mut mat) = mats.get_mut(h)
    {
        mat.base_color = if ok.is_ok() { Color::srgba(0.4, 0.9, 1.0, 0.35) } else { Color::srgba(1.0, 0.3, 0.25, 0.35) };
    }
    if !input.attack_pressed {
        return;
    }
    if let Err(why) = ok {
        log.say(&time, format!("build rejected: {why}"));
        return;
    }
    for (m, n) in &b.materials {
        let take = (*n).min(inv.count(m));
        inv.bag.remove(m, take);
    }
    spawn_building(&mut commands, &mut meshes, &mut mats, &db, &b.id, at, rot, false);
    log.say(&time, format!("placed {} (needs {:.0} work)", b.name, b.work));
}

/// Spawns a build object (as a blueprint unless `finished`).
pub fn spawn_building(commands: &mut Commands, meshes: &mut Assets<Mesh>, mats: &mut Assets<StandardMaterial>, db: &Db, id: &str, at: Vec3, rot: Quat, finished: bool) -> Entity {
    let b = db.build(id).expect("known build object");
    let shape = shape_of(&b.id);
    let tf = Transform::from_translation(at).with_rotation(rot);
    let mut e = commands.spawn((Name::new(b.name.clone()), Building { id: b.id.clone(), name: b.name.clone() }, tf, Visibility::default()));
    if shape.solid {
        e.insert((RigidBody::Static, Collider::compound(vec![(Vec3::Y * shape.size.y / 2.0, Quat::IDENTITY, Collider::cuboid(shape.size.x, shape.size.y, shape.size.z))]), CollisionLayers::new(Layer::Solid, LayerMask::ALL)));
    }
    let parent = e.id();
    spawn_model(commands, meshes, mats, parent, &model(&b.id), None);
    let mut e = commands.entity(parent);
    let lower = b.id.to_ascii_lowercase();
    e.insert(crate::interact::Interactable { prompt: format!("{} (hold to build)", b.name), radius: shape.size.x.max(shape.size.z) * 0.5 + 1.8 });
    let ent = e.id();
    if !finished && b.work > 0.0 {
        commands.entity(ent).insert(Construction { work: 0.0, need: b.work });
    } else {
        finish(commands, db, ent, &lower, &b.name);
    }
    ent
}

#[derive(Component, Debug)]
pub struct BuildingVisual;

/// Turns a finished object into its function (its `role`, `product` or `crop` in buildings.ron).
pub fn finish(commands: &mut Commands, db: &Db, e: Entity, lower: &str, name: &str) {
    use ck_core::data::Role;
    let mut ec = commands.entity(e);
    ec.remove::<Construction>();
    let r = shape_of(lower).size;
    let prompt = |p: &str| crate::interact::Interactable { prompt: format!("{name}: {p}"), radius: r.x.max(r.z) * 0.5 + 1.8 };
    match db.build(lower).and_then(|b| b.role.clone()) {
        Some(Role::Station) => {
            ec.insert((crate::crafting::Station::default(), prompt("open (F), hold F to work")));
        }
        Some(Role::Storage { slots, food_only }) => {
            ec.insert((Storage { items: Container::new(slots), food_only }, prompt("open")));
        }
        Some(Role::Core { radius }) => {
            ec.insert((BaseCamp { radius, level: 1 }, prompt("open Camp Core")));
        }
        Some(Role::Ranch) => {
            ec.insert((crate::base::Ranch::default(), prompt("Ranch")));
        }
        Some(Role::Repair) => {
            ec.insert((crate::base_tools::RepairBench, prompt("repair (F)")));
        }
        Some(Role::JobBoard) => {
            ec.insert((crate::base_tools::PriorityBoard, prompt("job priorities (F)")));
        }
        Some(Role::Bed) => {
            ec.insert((crate::base_tools::Bed { creature: false }, prompt("sleep (F, at night)")));
        }
        Some(Role::CreatureBed) => {
            ec.insert((crate::base_tools::Bed { creature: true }, prompt("a camp creature sleeps here at night")));
        }
        None => {
            if let Some(p) = crate::work::producer_for(db, lower) {
                ec.insert((p, prompt("hold F to work")));
            } else if let Some(f) = crate::work::farm_for(db, lower) {
                ec.insert((f, prompt("hold F to work")));
            } else {
                ec.remove::<crate::interact::Interactable>();
            }
        }
    }
}

/// Player construction work: hold F at a blueprint (work per second = the player's work speed).
fn construct(
    mut commands: Commands,
    time: Res<Time>,
    input: Res<PlayerInput>,
    db: Res<Db>,
    focus: Res<crate::interact::Focus>,
    mut progress: ResMut<Progress>,
    mut log: ResMut<GameLog>,
    mut q: Query<(Entity, &Building, &mut Construction, &Children)>,
    mut vis: Query<&mut Transform, With<BuildingVisual>>,
    workers: Res<crate::base::WorkContributions>,
) {
    let dt = time.delta_secs();
    for (e, b, mut c, children) in &mut q {
        let mut rate = workers.rate(e);
        if input.interact_held && focus.0.as_ref().is_some_and(|(f, _)| *f == e) {
            rate += crate::crafting::player_work_speed(&db, &progress);
        }
        c.work += rate * dt;
        let frac = (c.work / c.need).clamp(0.0, 1.0);
        for ch in children {
            if let Ok(mut t) = vis.get_mut(*ch) {
                t.scale = Vec3::new(1.0, 0.15 + 0.85 * frac, 1.0);
            }
        }
        if c.work >= c.need {
            let lower = b.id.to_ascii_lowercase();
            finish(&mut commands, &db, e, &lower, &b.name);
            if let Some(bo) = db.build(&b.id) {
                let xp = db.rules().build_exp * bo.exp;
                progress.pending_exp.push((xp.round() as u32, format!("built {}", b.name)));
            }
            log.say(&time, format!("{} finished", b.name));
        }
    }
}

/// Dismantle mode: X or the build menu turns it on, the object under the crosshair is marked red, holding LMB
/// takes it apart, Esc / Backspace leaves the mode. Hold time and reach: DISMANTLE_HOLD_S / DISMANTLE_RANGE_M.
#[derive(Resource, Debug, Default)]
pub struct Dismantle {
    pub on: bool,
    pub target: Option<Entity>,
    pub hold: f32,
    /// HUD text: what is aimed at and what comes back.
    pub label: String,
}

#[derive(Component, Debug)]
struct DismantleMarker;

/// Distance along the ray to a box (`half` extents, centred at `center`, rotated by `rot`), if it hits.
pub fn ray_box(origin: Vec3, dir: Vec3, center: Vec3, rot: Quat, half: Vec3) -> Option<f32> {
    let inv = rot.inverse();
    let o = inv * (origin - center);
    let d = inv * dir;
    let (mut lo, mut hi) = (0.0f32, f32::MAX);
    for i in 0..3 {
        if d[i].abs() < 1e-6 {
            if o[i].abs() > half[i] {
                return None;
            }
        } else {
            let (a, b) = ((-half[i] - o[i]) / d[i], (half[i] - o[i]) / d[i]);
            lo = lo.max(a.min(b));
            hi = hi.min(a.max(b));
            if lo > hi {
                return None;
            }
        }
    }
    Some(lo)
}

/// What a dismantled object gives back (all its materials), plus chest contents, station
/// output and the materials of jobs still queued.
fn refund(db: &Db, b: &Building, storage: Option<&Storage>, station: Option<&crate::crafting::Station>) -> Vec<(String, u32)> {
    let mut out: Vec<(String, u32)> = db.build(&b.id).map(|bo| bo.materials.clone()).unwrap_or_default();
    if let Some(s) = storage {
        out.extend(s.items.slots.iter().flatten().map(|st| (st.item.clone(), st.count)));
    }
    if let Some(st) = station {
        out.extend(st.output.iter().cloned());
        for j in &st.queue {
            if let Some(r) = db.recipe(&j.recipe) {
                out.extend(r.materials.iter().map(|(m, n)| (m.clone(), n * j.remaining)));
            }
        }
    }
    out
}

#[allow(clippy::type_complexity, clippy::too_many_arguments)]
fn dismantle(
    mut commands: Commands,
    time: Res<Time>,
    input: Res<PlayerInput>,
    db: Res<Db>,
    ui: Res<crate::input::UiFocus>,
    (mut dis, mut placing): (ResMut<Dismantle>, ResMut<Placing>),
    (mut log, mut rng, drops): (ResMut<GameLog>, ResMut<crate::resources::WorldRng>, Option<Res<crate::drops::DropAssets>>),
    (mut meshes, mut mats): (ResMut<Assets<Mesh>>, ResMut<Assets<StandardMaterial>>),
    cam: Query<&GlobalTransform, With<crate::camera::MainCamera>>,
    mut player: Query<(&Mover, &mut Inventory), With<Player>>,
    things: Query<(Entity, &Building, &GlobalTransform, Option<&Storage>, Option<&crate::crafting::Station>)>,
    mut marker: Query<(&mut Transform, &mut Visibility), With<DismantleMarker>>,
) {
    if input.dismantle_pressed && !ui.0 {
        dis.on = !dis.on;
        if dis.on {
            if let Some(g) = placing.ghost.take() {
                commands.entity(g).despawn();
            }
            placing.id = None;
        }
        log.say(&time, if dis.on { "dismantle mode: hold LMB on an object (Esc to leave)" } else { "dismantle mode off" });
    }
    if dis.on && (input.escape_pressed || input.aim || placing.id.is_some()) {
        dis.on = false;
        log.say(&time, "dismantle mode off");
    }
    let hide = |marker: &mut Query<(&mut Transform, &mut Visibility), With<DismantleMarker>>| {
        if let Ok((_, mut v)) = marker.single_mut() {
            *v = Visibility::Hidden;
        }
    };
    if !dis.on || ui.0 {
        dis.target = None;
        dis.hold = 0.0;
        hide(&mut marker);
        return;
    }
    let (Ok(ctf), Ok((pm, mut inv))) = (cam.single(), player.single_mut()) else { return };
    // The object under the crosshair (the camera's forward ray), nearest first.
    let (origin, dir) = (ctf.translation(), *ctf.forward());
    let pick = things
        .iter()
        .filter(|(_, _, tf, ..)| tf.translation().with_y(0.0).distance(pm.feet.with_y(0.0)) < ov::DISMANTLE_RANGE_M)
        .filter_map(|(e, b, tf, ..)| {
            let size = shape_of(&b.id).size.max(Vec3::splat(0.3));
            let (_, rot, pos) = tf.to_scale_rotation_translation();
            ray_box(origin, dir, pos + rot * Vec3::Y * size.y / 2.0, rot, size / 2.0).map(|t| (e, t))
        })
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .map(|(e, _)| e);
    if pick != dis.target {
        dis.target = pick;
        dis.hold = 0.0;
    }
    let Some((e, b, tf, storage, station)) = pick.and_then(|e| things.get(e).ok()) else {
        dis.label = "Aim at a building to dismantle it".into();
        hide(&mut marker);
        return;
    };
    let items = refund(&db, b, storage, station);
    let name = |id: &str| db.item(id).map_or(id.to_owned(), |i| i.name.clone());
    let back = items.iter().map(|(m, n)| format!("{} x{n}", name(m))).collect::<Vec<_>>().join(", ");
    let camp_core = b.id.eq_ignore_ascii_case("camp_core");
    dis.label = if camp_core { format!("{}: the Camp Core can't be dismantled (the camp would be lost)", b.name) } else { format!("{}  ->  {}", b.name, if back.is_empty() { "nothing back".to_owned() } else { back.clone() }) };
    // Red box over the target.
    let size = shape_of(&b.id).size;
    let (_, rot, pos) = tf.to_scale_rotation_translation();
    let mtf = Transform::from_translation(pos + rot * Vec3::Y * size.y / 2.0).with_rotation(rot).with_scale(size * 1.04 + Vec3::splat(0.04));
    match marker.single_mut() {
        Ok((mut t, mut v)) => {
            *t = mtf;
            *v = Visibility::Inherited;
        }
        Err(_) => {
            let mat = mats.add(StandardMaterial { base_color: Color::srgba(1.0, 0.25, 0.2, 0.3), alpha_mode: AlphaMode::Blend, unlit: true, ..default() });
            commands.spawn((DismantleMarker, Mesh3d(prim_mesh(&mut meshes, crate::build_models::Prim::Cube)), MeshMaterial3d(mat), mtf));
        }
    }
    if camp_core && input.attack_pressed {
        log.say(&time, "dismantle rejected: the Camp Core can't be dismantled here");
    }
    if !input.attack_held || camp_core {
        dis.hold = 0.0;
        return;
    }
    dis.hold += time.delta_secs();
    if dis.hold < ov::DISMANTLE_HOLD_S {
        return;
    }
    dis.hold = 0.0;
    let mut lost = Vec::new();
    for (m, n) in &items {
        let left = inv.give(&db, m, *n);
        if left > 0 {
            lost.push((m.clone(), left));
        }
    }
    if let Some(a) = drops.as_deref() {
        for (m, n) in &lost {
            crate::drops::spawn_drop(&mut commands, a, &db, m, *n, pos + Vec3::Y * 0.5, &mut rng.0);
        }
    }
    commands.entity(e).despawn();
    dis.target = None;
    hide(&mut marker);
    log.say(&time, format!("dismantled {} (+{}){}", b.name, if back.is_empty() { "nothing".to_owned() } else { back }, if lost.is_empty() { "" } else { "; bag full, the rest dropped" }));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ray_box_hits_and_misses() {
        let half = Vec3::new(1.0, 0.5, 0.5);
        assert!((ray_box(Vec3::new(0.0, 0.0, 5.0), Vec3::NEG_Z, Vec3::ZERO, Quat::IDENTITY, half).unwrap() - 4.5).abs() < 1e-4);
        assert!(ray_box(Vec3::new(3.0, 0.0, 5.0), Vec3::NEG_Z, Vec3::ZERO, Quat::IDENTITY, half).is_none());
        // Turned 90 degrees the long side runs along z, so x = 0.8 misses.
        assert!(ray_box(Vec3::new(0.8, 0.0, 5.0), Vec3::NEG_Z, Vec3::ZERO, Quat::from_rotation_y(std::f32::consts::FRAC_PI_2), half).is_none());
        assert!(ray_box(Vec3::new(0.8, 0.0, 5.0), Vec3::NEG_Z, Vec3::ZERO, Quat::IDENTITY, half).is_some());
    }
}
