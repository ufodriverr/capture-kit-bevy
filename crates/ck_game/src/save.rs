//! Quick save / load (F5 / F9) to `saves/quick.json` next to the exe: player (position, HP/SP/hunger, inventory,
//! progress), owned creatures, party order and stance, buildings (blueprint work, station queues, storage,
//! camp level), dropped items and the clock. Wild creatures and resource nodes are not saved: they respawn.
//! A summoned creature is recalled on load.

use bevy::ecs::system::SystemState;
use bevy::prelude::*;
use ck_core::inventory::{Container, Stack};
use ck_core::creature::{Individual, Place};
use serde::{Deserialize, Serialize};

use crate::building::{BaseCamp, Building, Construction, Storage};
use crate::combat::Combatant;
use crate::crafting::{Job, Station};
use crate::data::Db;
use crate::drops::{DropAssets, WorldItem, spawn_drop};
use crate::input::PlayerInput;
use crate::inventory::Inventory;
use crate::kinematic::Mover;
use crate::log::GameLog;
use crate::creatures::{CreatureActor, Creatures, Role, WorldClock};
use crate::player::{Health, Hunger, Locomotion, Mode, Player, Stamina};
use crate::progress::Progress;

pub const VERSION: u32 = 1;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct SaveFile {
    pub version: u32,
    pub clock_minutes: f32,
    pub player: PlayerSave,
    pub progress: ProgressSave,
    pub roster: Vec<Individual>,
    pub next_creature_id: u64,
    pub party_selected: usize,
    pub stance: String,
    pub buildings: Vec<BuildingSave>,
    pub drops: Vec<(String, u32, [f32; 3])>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct PlayerSave {
    pub feet: [f32; 3],
    pub yaw: f32,
    pub hp: f32,
    pub hp_max: f32,
    pub sp_max: f32,
    pub hunger: f32,
    pub melee_attack: f32,
    pub bag: Container,
    pub weapons: Vec<Option<Stack>>,
    pub selected: usize,
    pub armor: Option<Stack>,
    #[serde(default)]
    pub head: Option<Stack>,
    pub glider: Option<Stack>,
    pub orb: String,
    pub loaded: u32,
    #[serde(default = "default_ammo")]
    pub ammo: String,
    pub weight_points: u32,
}

fn default_ammo() -> String {
    "arrow".into()
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ProgressSave {
    pub level: u32,
    pub exp: u32,
    pub tech_points: u32,
    pub status_points: u32,
    pub unlocked: Vec<String>,
    pub captures: Vec<(String, u32)>,
    pub alloc: [u32; 5],
    #[serde(default)]
    pub mission: usize,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct BuildingSave {
    pub id: String,
    pub pos: [f32; 3],
    pub yaw: f32,
    pub work: Option<[f32; 2]>,
    pub queue: Vec<(String, u32, f32, f32)>,
    pub output: Vec<(String, u32)>,
    pub storage: Option<Container>,
    pub base_level: Option<u32>,
}

/// `saves/quick.json` next to the exe (a `dist/` checkpoint keeps its own saves).
pub fn default_path() -> std::path::PathBuf {
    std::env::current_exe().ok().and_then(|p| p.parent().map(|d| d.join("saves/quick.json"))).unwrap_or_else(|| "saves/quick.json".into())
}

pub struct SavePlugin;

impl Plugin for SavePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, hotkeys.in_set(crate::Sim::Last));
    }
}

fn hotkeys(world: &mut World) {
    let (save, load) = {
        let i = world.resource::<PlayerInput>();
        (i.save_pressed, i.load_pressed)
    };
    let path = default_path();
    let t = world.resource::<Time>().elapsed_secs();
    let msg = if save {
        Some(match save_to(world, &path) {
            Ok(n) => format!("saved ({n} bytes) to {}", path.display()),
            Err(e) => format!("save failed: {e}"),
        })
    } else if load {
        Some(match load_from(world, &path) {
            Ok(()) => format!("loaded {}", path.display()),
            Err(e) => format!("load failed: {e}"),
        })
    } else {
        None
    };
    if let Some(m) = msg {
        world.resource_mut::<GameLog>().push(t, m);
    }
}

pub fn snapshot(world: &mut World) -> Result<SaveFile, String> {
    let mut pq = world.query_filtered::<(&Mover, &Health, &Stamina, &Hunger, &Combatant, &Inventory), With<Player>>();
    let (m, h, s, hu, c, inv) = pq.iter(world).next().ok_or("no player")?;
    let player = PlayerSave {
        feet: m.feet.to_array(),
        yaw: m.yaw,
        hp: h.hp,
        hp_max: h.max,
        sp_max: s.max,
        hunger: hu.value,
        melee_attack: c.melee_attack,
        bag: inv.bag.clone(),
        weapons: inv.weapons.clone(),
        selected: inv.selected,
        armor: inv.armor.clone(),
        head: inv.head.clone(),
        glider: inv.glider.clone(),
        orb: inv.orb.clone(),
        loaded: inv.loaded,
        ammo: inv.ammo.clone(),
        weight_points: inv.weight_points,
    };
    let p = world.resource::<Progress>();
    let progress = ProgressSave { level: p.level, exp: p.exp, tech_points: p.tech_points, status_points: p.status_points, unlocked: p.unlocked.iter().cloned().collect(), captures: p.captures.iter().map(|(k, v)| (k.clone(), *v)).collect(), alloc: p.alloc, mission: p.mission };
    let roster = world.resource::<Creatures>();
    let owned: Vec<Individual> = roster.0.all.values().filter(|p| p.owned).cloned().collect();
    let next_creature_id = roster.0.next_id;
    let party = world.resource::<crate::companion::Party>();
    let (party_selected, stance) = (party.selected, format!("{:?}", party.stance));
    let mut bq = world.query::<(&Building, &Transform, Option<&Construction>, Option<&Station>, Option<&Storage>, Option<&BaseCamp>)>();
    let buildings = bq
        .iter(world)
        .map(|(b, tf, c, st, sto, bc)| BuildingSave {
            id: b.id.clone(),
            pos: tf.translation.to_array(),
            yaw: tf.rotation.to_euler(EulerRot::YXZ).0,
            work: c.map(|c| [c.work, c.need]),
            queue: st.map(|s| s.queue.iter().map(|j| (j.recipe.clone(), j.remaining, j.work, j.need)).collect()).unwrap_or_default(),
            output: st.map(|s| s.output.clone()).unwrap_or_default(),
            storage: sto.map(|s| s.items.clone()),
            base_level: bc.map(|b| b.level),
        })
        .collect();
    let mut dq = world.query::<(&WorldItem, &Transform)>();
    let drops = dq.iter(world).map(|(w, t)| (w.item.clone(), w.count, t.translation.to_array())).collect();
    let clock_minutes = world.get_resource::<WorldClock>().map_or(0.0, |c| c.minutes);
    Ok(SaveFile { version: VERSION, clock_minutes, player, progress, roster: owned, next_creature_id, party_selected, stance, buildings, drops })
}

pub fn save_to(world: &mut World, path: &std::path::Path) -> Result<usize, String> {
    let s = snapshot(world)?;
    let json = serde_json::to_string_pretty(&s).map_err(|e| e.to_string())?;
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    std::fs::write(path, &json).map_err(|e| e.to_string())?;
    Ok(json.len())
}

pub fn load_from(world: &mut World, path: &std::path::Path) -> Result<(), String> {
    let text = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
    let s: SaveFile = serde_json::from_str(&text).map_err(|e| e.to_string())?;
    if s.version != VERSION {
        return Err(format!("save version {} (expected {VERSION})", s.version));
    }
    apply(world, s)
}

pub fn apply(world: &mut World, s: SaveFile) -> Result<(), String> {
    let db = world.resource::<Db>().clone();
    // Clear what the save replaces.
    let mut gone: Vec<Entity> = Vec::new();
    gone.extend(world.query_filtered::<Entity, With<Building>>().iter(world));
    gone.extend(world.query_filtered::<Entity, With<WorldItem>>().iter(world));
    gone.extend(world.query_filtered::<Entity, With<crate::building::Ghost>>().iter(world));
    gone.extend(world.query::<(Entity, &CreatureActor)>().iter(world).filter(|(_, a)| !matches!(a.role, Role::Wild { .. })).map(|(e, _)| e));
    for e in gone {
        if world.get_entity(e).is_ok() {
            world.despawn(e);
        }
    }
    *world.resource_mut::<crate::building::Placing>() = default();
    {
        let mut party = world.resource_mut::<crate::companion::Party>();
        party.active_id = None;
        party.active_entity = None;
        party.direct_target = None;
        party.selected = s.party_selected.min(ck_core::creature::PARTY_SIZE - 1);
        party.stance = crate::companion::Stance::parse(&s.stance);
    }
    *world.resource_mut::<crate::partner::PartnerState>() = default();
    // Owned creatures: replace; wild ones stay (their actors are live).
    {
        let mut roster = world.resource_mut::<Creatures>();
        roster.0.all.retain(|_, p| !p.owned);
        let max_id = s.roster.iter().map(|p| p.id).max().unwrap_or(0);
        for p in s.roster {
            roster.0.all.insert(p.id, p);
        }
        roster.0.next_id = roster.0.next_id.max(s.next_creature_id).max(max_id);
    }
    // Player.
    let mut pq = world.query_filtered::<(&mut Mover, &mut Locomotion, &mut Health, &mut Stamina, &mut Hunger, &mut Combatant, &mut Inventory), With<Player>>();
    let (mut m, mut l, mut h, mut st, mut hu, mut c, mut inv) = pq.single_mut(world).map_err(|e| e.to_string())?;
    let p = &s.player;
    m.feet = Vec3::from_array(p.feet) + Vec3::Y * 0.05;
    m.vel = Vec3::ZERO;
    m.yaw = p.yaw;
    m.grounded = false;
    l.mode = Mode::Air;
    l.fall_origin = m.feet.y;
    h.hp = p.hp;
    h.max = p.hp_max;
    st.max = p.sp_max;
    st.sp = p.sp_max;
    hu.value = p.hunger;
    c.melee_attack = p.melee_attack;
    *inv = Inventory { bag: p.bag.clone(), weapons: p.weapons.clone(), selected: p.selected, armor: p.armor.clone(), head: p.head.clone(), glider: p.glider.clone(), orb: p.orb.clone(), loaded: p.loaded, ammo: p.ammo.clone(), weight_points: p.weight_points, bond_weight: 0.0 };
    {
        let mut pr = world.resource_mut::<Progress>();
        let q = &s.progress;
        pr.level = q.level;
        pr.exp = q.exp;
        pr.tech_points = q.tech_points;
        pr.status_points = q.status_points;
        pr.unlocked = q.unlocked.iter().cloned().collect();
        pr.captures = q.captures.iter().cloned().collect();
        pr.alloc = q.alloc;
        pr.mission = q.mission;
        pr.pending_exp.clear();
    }
    if let Some(mut clock) = world.get_resource_mut::<WorldClock>() {
        clock.minutes = s.clock_minutes;
    }
    // Buildings and drops.
    let mut state: SystemState<(Commands, ResMut<Assets<Mesh>>, ResMut<Assets<StandardMaterial>>, Option<Res<DropAssets>>, ResMut<crate::resources::WorldRng>)> = SystemState::new(world);
    let mut spawned = Vec::new();
    {
        let (mut commands, mut meshes, mut mats, drops, mut rng) = state.get_mut(world).map_err(|e| e.to_string())?;
        for b in &s.buildings {
            if db.build(&b.id).is_none() {
                continue;
            }
            let e = crate::building::spawn_building(&mut commands, &mut meshes, &mut mats, &db, &b.id, Vec3::from_array(b.pos), Quat::from_rotation_y(b.yaw), b.work.is_none());
            spawned.push((e, b.clone()));
        }
        if let Some(da) = drops {
            for (item, n, at) in &s.drops {
                spawn_drop(&mut commands, &da, &db, item, *n, Vec3::from_array(*at), &mut rng.0);
            }
        }
    }
    state.apply(world);
    for (e, b) in spawned {
        if let (Some(w), Some(mut c)) = (b.work, world.get_mut::<Construction>(e)) {
            c.work = w[0];
            c.need = w[1];
        }
        if let Some(mut st) = world.get_mut::<Station>(e) {
            st.queue = b.queue.iter().map(|(r, n, w, need)| Job { recipe: r.clone(), remaining: *n, work: *w, need: *need }).collect();
            st.output = b.output.clone();
        }
        if let (Some(items), Some(mut sto)) = (b.storage.clone(), world.get_mut::<Storage>(e)) {
            sto.items = items;
        }
        if let (Some(lv), Some(mut bc)) = (b.base_level, world.get_mut::<BaseCamp>(e)) {
            bc.level = lv;
        }
    }
    // Owned creatures that were summoned or working are back in their place; nothing is "out".
    let mut roster = world.resource_mut::<Creatures>();
    for p in roster.0.all.values_mut().filter(|p| p.owned) {
        if p.place == Place::Wild {
            p.place = Place::Storage;
        }
    }
    Ok(())
}
