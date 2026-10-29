//! Camp work beyond the workbench: what each building asks of a worker (its job slots in buildings.ron:
//! job, rank, how many, player workable, task), the task order (rules.ron `task_order`), production
//! buildings (Stone Quarry, Lumber Yard: `product` work per unit, dropped beside them), fields (`crop`:
//! planting, watering, growing, harvest) and the fire of the Campfire and Smelter (lit while a creature
//! with the kindle job works it).

use bevy::prelude::*;

use crate::building::{Building, Construction};
use crate::data::Db;
use crate::drops::{DropAssets, spawn_drop};
use crate::input::PlayerInput;
use crate::interact::{Focus, Interactable};
use crate::log::{GameLog, Say};
use crate::progress::Progress;

/// One job slot of a building.
pub type AssignRow = ck_core::data::JobSlot;

pub fn assign_rows(db: &Db, build_id: &str) -> Vec<AssignRow> {
    db.build(build_id).map(|b| b.jobs.clone()).unwrap_or_default()
}

/// Position of a task in rules.ron `task_order` (lower = first).
pub fn priority(db: &Db, task: &str) -> usize {
    db.rules().task_order.iter().position(|t| t == task).unwrap_or(99)
}

/// What a worker does for a job (inspector and log wording).
pub fn verb(job: &str) -> &'static str {
    match job {
        "kindle" => "kindling",
        "water" => "watering",
        "plant" => "planting",
        "power" => "powering",
        "craft" => "crafting",
        "gather" => "gathering",
        "chop" => "chopping",
        "mine" => "mining",
        "brew" => "brewing",
        "chill" => "chilling",
        "haul" => "hauling",
        "ranch" => "ranching",
        _ => "working",
    }
}

/// Whether a worker task kind is station work (one of the `verb`s).
pub fn is_facility_work(kind: &str) -> bool {
    ["kindling", "watering", "planting", "powering", "crafting", "gathering", "chopping", "mining", "brewing", "chilling"].contains(&kind)
}

/// A production building: every `need` work makes one `product`.
#[derive(Component, Debug, Clone)]
pub struct Producer {
    pub product: String,
    pub need: f32,
    pub work: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FarmStage {
    Seeding,
    Watering,
    Growing,
    Harvest,
}

impl FarmStage {
    /// The job the stage needs.
    pub fn suit(self) -> Option<&'static str> {
        match self {
            FarmStage::Seeding => Some("plant"),
            FarmStage::Watering => Some("water"),
            FarmStage::Harvest => Some("gather"),
            FarmStage::Growing => None,
        }
    }
}

/// A field and its crop.
#[derive(Component, Debug, Clone)]
pub struct Farm {
    pub stage: FarmStage,
    pub work: f32,
    pub grow: f32,
    pub crop: String,
    pub crop_num: u32,
    pub grow_time: f32,
    pub seeding: f32,
    pub watering: f32,
    pub harvest: f32,
}

impl Farm {
    pub fn need(&self) -> f32 {
        match self.stage {
            FarmStage::Seeding => self.seeding,
            FarmStage::Watering => self.watering,
            FarmStage::Harvest => self.harvest,
            FarmStage::Growing => self.grow_time,
        }
    }
}

pub fn producer_for(db: &Db, build_id: &str) -> Option<Producer> {
    let (product, need) = db.build(build_id)?.product.clone()?;
    Some(Producer { product, need, work: 0.0 })
}

pub fn farm_for(db: &Db, build_id: &str) -> Option<Farm> {
    let c = db.build(build_id)?.crop.as_ref()?;
    Some(Farm { stage: FarmStage::Seeding, work: 0.0, grow: 0.0, crop: c.item.clone(), crop_num: c.count, grow_time: c.grow_s, seeding: c.seed_work, watering: c.water_work, harvest: c.harvest_work })
}

/// A Campfire / Smelter flame: shown while someone works the station.
#[derive(Component, Debug)]
pub struct Flame;

/// One plant on a field.
#[derive(Component, Debug)]
struct Plant;

#[derive(Component, Debug)]
struct Fruit;

pub struct WorkPlugin;

impl Plugin for WorkPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, (produce, farm, flames, farm_visuals).in_set(crate::Sim::Last));
    }
}

fn player_on(input: &PlayerInput, focus: &Focus, e: Entity) -> bool {
    input.interact_held && focus.0.as_ref().is_some_and(|(f, _)| *f == e)
}

#[allow(clippy::too_many_arguments)]
fn produce(
    mut commands: Commands,
    time: Res<Time>,
    db: Res<Db>,
    input: Res<PlayerInput>,
    focus: Res<Focus>,
    progress: Res<Progress>,
    drops: Option<Res<DropAssets>>,
    contrib: Res<crate::base::WorkContributions>,
    mut rng: ResMut<crate::base::BaseRng>,
    mut log: ResMut<GameLog>,
    mut q: Query<(Entity, &GlobalTransform, &Building, &mut Producer, &mut Interactable), Without<Construction>>,
) {
    let Some(da) = drops else { return };
    let dt = time.delta_secs();
    for (e, tf, b, mut p, mut it) in &mut q {
        let mut rate = contrib.rate(e);
        if player_on(&input, &focus, e) && assign_rows(&db, &b.id).first().is_none_or(|r| r.player) {
            rate += crate::crafting::player_work_speed(&db, &progress);
        }
        p.work += rate * dt;
        if p.work >= p.need {
            p.work -= p.need;
            let at = tf.translation() + tf.rotation() * Vec3::new(1.8, 0.6, 0.0);
            spawn_drop(&mut commands, &da, &db, &p.product, 1, at, &mut rng.0);
            log.say(&time, format!("{} produced {}", b.name, db.item(&p.product).map_or(p.product.as_str(), |i| i.name.as_str())));
        }
        let name = db.item(&p.product).map_or(p.product.clone(), |i| i.name.clone());
        it.prompt = format!("{}: next {name} {:.0}% (hold F to work, or a creature)", b.name, p.work / p.need * 100.0);
    }
}

#[allow(clippy::too_many_arguments)]
fn farm(
    mut commands: Commands,
    time: Res<Time>,
    db: Res<Db>,
    input: Res<PlayerInput>,
    focus: Res<Focus>,
    progress: Res<Progress>,
    drops: Option<Res<DropAssets>>,
    contrib: Res<crate::base::WorkContributions>,
    mut rng: ResMut<crate::base::BaseRng>,
    mut log: ResMut<GameLog>,
    mut q: Query<(Entity, &GlobalTransform, &Building, &mut Farm, &mut Interactable), Without<Construction>>,
) {
    let Some(da) = drops else { return };
    let dt = time.delta_secs();
    for (e, tf, b, mut f, mut it) in &mut q {
        if f.stage == FarmStage::Growing {
            f.grow += dt;
            if f.grow >= f.grow_time {
                f.stage = FarmStage::Harvest;
                f.work = 0.0;
                log.say(&time, format!("{}: ready to harvest", b.name));
            }
        } else {
            let mut rate = contrib.rate(e);
            if player_on(&input, &focus, e) {
                rate += crate::crafting::player_work_speed(&db, &progress);
            }
            f.work += rate * dt;
            if f.work >= f.need() {
                f.work = 0.0;
                f.stage = match f.stage {
                    FarmStage::Seeding => FarmStage::Watering,
                    FarmStage::Watering => {
                        f.grow = 0.0;
                        FarmStage::Growing
                    }
                    _ => {
                        let n = f.crop_num;
                        spawn_drop(&mut commands, &da, &db, &f.crop.clone(), n, tf.translation() + Vec3::Y * 0.8, &mut rng.0);
                        log.say(&time, format!("{}: harvested {} x{n}", b.name, db.item(&f.crop).map_or(f.crop.as_str(), |i| i.name.as_str())));
                        FarmStage::Seeding
                    }
                };
            }
        }
        let what = match f.stage {
            FarmStage::Seeding => "needs planting",
            FarmStage::Watering => "needs watering",
            FarmStage::Growing => "growing",
            FarmStage::Harvest => "ready to harvest",
        };
        let pct = if f.stage == FarmStage::Growing { f.grow / f.grow_time.max(1.0) } else { f.work / f.need().max(1.0) } * 100.0;
        it.prompt = format!("{}: {what} {pct:.0}%{}", b.name, if f.stage == FarmStage::Growing { "" } else { " (hold F, or a creature)" });
    }
}

/// Flames on the Campfire and Smelter while worked (kindle).
fn flames(mut commands: Commands, mut meshes: ResMut<Assets<Mesh>>, mut mats: ResMut<Assets<StandardMaterial>>, contrib: Res<crate::base::WorkContributions>, q: Query<(Entity, &Building, Option<&Children>), (With<crate::crafting::Station>, Without<Construction>)>, mut flames: Query<&mut Visibility, With<Flame>>, mut assets: Local<Option<(Handle<Mesh>, Handle<StandardMaterial>)>>) {
    let (mesh, mat) = assets.get_or_insert_with(|| (meshes.add(Sphere::new(1.0).mesh().ico(2).unwrap()), mats.add(StandardMaterial { base_color: Color::srgb(1.0, 0.55, 0.1), emissive: LinearRgba::rgb(9.0, 3.5, 0.5), ..default() }))).clone();
    for (e, b, children) in &q {
        let id = b.id.to_ascii_lowercase();
        if id != "campfire" && id != "smelter" {
            continue;
        }
        let lit = contrib.rate(e) > 0.0;
        let mut have = false;
        for ch in children.into_iter().flatten() {
            if let Ok(mut v) = flames.get_mut(*ch) {
                have = true;
                *v = if lit { Visibility::Inherited } else { Visibility::Hidden };
            }
        }
        if !have {
            let y = crate::building::shape_of(&b.id).size.y;
            commands.spawn((Flame, Mesh3d(mesh.clone()), MeshMaterial3d(mat.clone()), Transform::from_xyz(0.0, y + 0.15, 0.0).with_scale(Vec3::new(0.3, 0.45, 0.3)), Visibility::Hidden, ChildOf(e))).with_children(|c| {
                c.spawn((PointLight { color: Color::srgb(1.0, 0.6, 0.25), intensity: 150_000.0, range: 10.0, ..default() }, Transform::default()));
            });
        }
    }
}

/// Plants on a field: hidden before planting, sprouts, growing, berries when ripe.
#[allow(clippy::type_complexity)]
fn farm_visuals(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut mats: ResMut<Assets<StandardMaterial>>,
    q: Query<(Entity, &Farm, Option<&Children>)>,
    mut plants: Query<(&mut Transform, &mut Visibility, Option<&Children>), (With<Plant>, Without<Fruit>)>,
    mut fruits: Query<&mut Visibility, (With<Fruit>, Without<Plant>)>,
    mut assets: Local<Option<(Handle<Mesh>, Handle<StandardMaterial>, Handle<StandardMaterial>)>>,
) {
    let (mesh, leaf, berry) = assets
        .get_or_insert_with(|| (meshes.add(Sphere::new(1.0).mesh().ico(1).unwrap()), mats.add(Color::srgb(0.25, 0.6, 0.2)), mats.add(StandardMaterial { base_color: Color::srgb(0.85, 0.1, 0.15), emissive: LinearRgba::rgb(0.3, 0.0, 0.0), ..default() })))
        .clone();
    for (e, f, children) in &q {
        let mut any = false;
        let size = match f.stage {
            FarmStage::Seeding => 0.0,
            FarmStage::Watering => 0.25,
            FarmStage::Growing => 0.25 + 0.75 * (f.grow / f.grow_time.max(1.0)),
            FarmStage::Harvest => 1.0,
        };
        for ch in children.into_iter().flatten() {
            let Ok((mut tf, mut v, kids)) = plants.get_mut(*ch) else { continue };
            any = true;
            *v = if size > 0.0 { Visibility::Inherited } else { Visibility::Hidden };
            tf.scale = Vec3::splat(0.28 * size.max(0.01));
            for k in kids.into_iter().flatten() {
                if let Ok(mut fv) = fruits.get_mut(*k) {
                    *fv = if f.stage == FarmStage::Harvest { Visibility::Inherited } else { Visibility::Hidden };
                }
            }
        }
        if !any {
            for i in 0..9 {
                let at = Vec3::new((i % 3) as f32 - 1.0, 0.35, (i / 3) as f32 - 1.0) * Vec3::new(0.9, 1.0, 0.9);
                commands.spawn((Plant, Mesh3d(mesh.clone()), MeshMaterial3d(leaf.clone()), Transform::from_translation(at).with_scale(Vec3::splat(0.01)), Visibility::Hidden, ChildOf(e))).with_children(|c| {
                    for j in 0..3 {
                        let a = j as f32 * 2.1;
                        c.spawn((Fruit, Mesh3d(mesh.clone()), MeshMaterial3d(berry.clone()), Transform::from_xyz(a.cos() * 0.8, 0.4, a.sin() * 0.8).with_scale(Vec3::splat(0.3)), Visibility::Hidden));
                    }
                });
            }
        }
    }
}
