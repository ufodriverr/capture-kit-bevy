//! The party and the active companion: selection (1/3), summon/recall (E) near the player with a short
//! cooldown (rules.ron `recall_cooldown`), follow (stops within 1 m, catches up at up to 3x the player's
//! speed, warps beyond 65 m), the stance order (Default / Warlike / NotCombat, key 4) and the direct attack
//! order (middle mouse). Selected slot, active slot and the individual are separate state.

use avian3d::prelude::*;
use bevy::prelude::*;

use ck_core::creature::CreatureId;

use crate::camera::CameraRig;
use crate::combat::{Combatant, Damaged, Faction};
use crate::data::Db;
use crate::input::PlayerInput;
use crate::kinematic::Mover;
use crate::log::{GameLog, Say};
use crate::creatures::{AiState, Brain, CreatureActor, CreatureAssets, CreatureBody, Creatures, Role, Steer, spawn_creature_actor};
use crate::player::{Health, Locomotion, Player};
use crate::world::Terrain;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stance {
    Default,
    Warlike,
    NotCombat,
}

impl Stance {
    pub fn name(self) -> &'static str {
        match self {
            Stance::Default => "Default (assist)",
            Stance::Warlike => "Warlike (attack nearby)",
            Stance::NotCombat => "NotCombat (don't fight)",
        }
    }
    /// From the `{:?}` form stored in a save.
    pub fn parse(s: &str) -> Self {
        match s {
            "Warlike" => Stance::Warlike,
            "NotCombat" => Stance::NotCombat,
            _ => Stance::Default,
        }
    }
}

#[derive(Resource, Debug, Clone)]
pub struct Party {
    pub selected: usize,
    pub active_id: Option<CreatureId>,
    pub active_entity: Option<Entity>,
    pub stance: Stance,
    pub direct_target: Option<Entity>,
    /// Seconds until summoning is allowed again after a recall.
    pub cooldown: f32,
}

impl Default for Party {
    fn default() -> Self {
        Self { selected: 0, active_id: None, active_entity: None, stance: Stance::Default, direct_target: None, cooldown: 0.0 }
    }
}

/// The player's current fight: the last wild creature the player hit or was hit by (assist target).
#[derive(Resource, Debug, Default)]
pub struct PlayerBattle {
    pub target: Option<Entity>,
    pub until: f32,
}

pub struct CompanionPlugin;

impl Plugin for CompanionPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Party>().init_resource::<PlayerBattle>().add_systems(Update, (party_input, track_battle).chain().in_set(crate::Sim::Creatures).before(companion_ai));
    }
}

/// Where a summoned creature appears: beside the player, on the ground, not inside something.
pub fn spawn_point_near(sq: &SpatialQuery, terrain: &Terrain, pm: &Mover) -> Option<Vec3> {
    let right = Vec3::new(-pm.forward().z, 0.0, pm.forward().x);
    for off in [right * 1.6 + pm.forward() * 0.8, -right * 1.6 + pm.forward() * 0.8, pm.forward() * 2.0, -pm.forward() * 1.8] {
        let p = pm.feet + off;
        let y = terrain.height(p.x, p.z).max(p.y - 1.0);
        let at = Vec3::new(p.x, y + 0.05, p.z);
        let blocked = !sq.shape_intersections(&Collider::sphere(0.35), at + Vec3::Y * 0.45, Quat::IDENTITY, &SpatialQueryFilter::from_mask([crate::world::Layer::Solid])).is_empty();
        if !blocked {
            return Some(at);
        }
    }
    None
}

fn party_input(
    mut commands: Commands,
    time: Res<Time>,
    input: Res<PlayerInput>,
    db: Res<Db>,
    sq: SpatialQuery,
    terrain: Res<Terrain>,
    rig: Res<CameraRig>,
    assets: Option<Res<CreatureAssets>>,
    mut party: ResMut<Party>,
    mut roster: ResMut<Creatures>,
    mut log: ResMut<GameLog>,
    player: Query<&Mover, With<Player>>,
    actors: Query<(Entity, &CreatureActor, &Mover, &Health), Without<Player>>,
    shield: Query<(), With<crate::partner::ShieldGuard>>,
) {
    let dt = time.delta_secs();
    party.cooldown = (party.cooldown - dt).max(0.0);
    // The active actor vanished (incapacitated, despawned): clear it.
    if let Some(e) = party.active_entity
        && actors.get(e).is_err()
    {
        party.active_entity = None;
        party.active_id = None;
    }
    let Ok(pm) = player.single() else { return };
    let slots = roster.0.party();
    let step = |from: usize, dir: i32| -> usize { (0..5).map(|k| ((from as i32 + dir * (k + 1)).rem_euclid(5)) as usize).find(|i| slots[*i].is_some()).unwrap_or(from) };
    if input.party_next {
        party.selected = step(party.selected, 1);
    }
    if input.party_prev {
        party.selected = step(party.selected, -1);
    }
    if input.command_pressed {
        party.stance = match party.stance {
            Stance::Default => Stance::Warlike,
            Stance::Warlike => Stance::NotCombat,
            Stance::NotCombat => Stance::Default,
        };
        log.say(&time, format!("order: {}", party.stance.name()));
    }
    if input.direct_attack_pressed {
        // The wild creature closest to the camera's aim line within 40 m.
        let rot = Quat::from_euler(EulerRot::YXZ, rig.yaw, rig.pitch, 0.0);
        let dir = rot * Vec3::NEG_Z;
        let eye = pm.feet + Vec3::Y * 1.5;
        let best = actors
            .iter()
            .filter(|(_, a, _, h)| matches!(a.role, Role::Wild { .. }) && h.hp > 0.0)
            .filter_map(|(e, _, m, _)| {
                let to = m.feet + Vec3::Y * 0.4 - eye;
                let along = to.dot(dir);
                (along > 0.0 && along < 40.0).then(|| (e, (to - dir * along).length() / along))
            })
            .filter(|(_, ang)| *ang < 0.15)
            .min_by(|a, b| a.1.total_cmp(&b.1));
        match (best, party.active_entity) {
            (Some((e, _)), Some(_)) => {
                party.direct_target = Some(e);
                log.say(&time, "order: attack that target");
            }
            (_, None) => log.say(&time, "attack order rejected: no creature summoned"),
            (None, _) => log.say(&time, "attack order rejected: no target in sight"),
        }
    }
    if !input.summon_pressed {
        return;
    }
    if !shield.is_empty() {
        log.say(&time, "summon rejected: bond skill active (CanThrowPal false)");
        return;
    }
    let selected = slots[party.selected];
    // Recall the active creature first.
    if let (Some(e), Some(id)) = (party.active_entity, party.active_id) {
        commands.entity(e).despawn();
        party.active_entity = None;
        party.active_id = None;
        party.direct_target = None;
        party.cooldown = db.rules().recall_cooldown;
        let name = roster.0.get(id).map(|p| db.creatures[&p.species].name.clone()).unwrap_or_default();
        log.say(&time, format!("recalled {name}"));
        if selected == Some(id) {
            return;
        }
    }
    let Some(id) = selected else {
        log.say(&time, "summon rejected: empty party slot");
        return;
    };
    if party.cooldown > 0.0 {
        log.say(&time, format!("summon rejected: cooldown {:.1} s", party.cooldown));
        return;
    }
    let Some(ind) = roster.0.get(id) else { return };
    if ind.incapacitated {
        log.say(&time, "summon rejected: the creature is incapacitated (recover it in the Camp Core)");
        return;
    }
    let Some(at) = spawn_point_near(&sq, &terrain, pm) else {
        log.say(&time, "summon rejected: no room beside the player");
        return;
    };
    let Some(assets) = assets else { return };
    let e = spawn_creature_actor(&mut commands, &db, &roster.0, &assets, id, at, Role::Companion, pm.yaw);
    party.active_entity = Some(e);
    party.active_id = Some(id);
    let name = db.creatures[&roster.0.all[&id].species].name.clone();
    log.say(&time, format!("summoned {name} (slot {})", party.selected + 1));
    let _ = &mut roster;
}

/// Remembers the player's fight for the companion's Default stance.
fn track_battle(time: Res<Time>, mut ev: MessageReader<Damaged>, mut battle: ResMut<PlayerBattle>, player: Query<Entity, With<Player>>, wild: Query<&Combatant>) {
    let Ok(pe) = player.single() else { return };
    for d in ev.read() {
        let other = if d.attacker == pe {
            d.target
        } else if d.target == pe {
            d.attacker
        } else {
            continue;
        };
        if wild.get(other).is_ok_and(|c| c.faction == Faction::Wild) && !d.killed {
            battle.target = Some(other);
            battle.until = time.elapsed_secs() + 15.0;
        }
    }
}

pub fn companion_ai(
    time: Res<Time>,
    sq: SpatialQuery,
    mut party: ResMut<Party>,
    battle: Res<PlayerBattle>,
    player: Query<(Entity, &Mover, &Locomotion), With<Player>>,
    mut q: Query<(Entity, &CreatureActor, &mut Mover, &mut Brain, &mut Steer, &CreatureBody, &crate::skills::SkillSlots), Without<Player>>,
    targets: Query<(Entity, &Mover, &Combatant, &Health), (Without<Player>, Without<Steer>)>,
    wild_actors: Query<(Entity, &CreatureActor, &Health)>,
    shielding: Query<(), With<crate::partner::Shielding>>,
) {
    let Ok((_pe, pm, ploco)) = player.single() else { return };
    let now = time.elapsed_secs();
    let dt = time.delta_secs();
    // Wild creatures' positions (they have Steer, so read them from the actor query below via a copy).
    let wild_pos: Vec<(Entity, Vec3, f32)> = q.iter().filter(|(_, a, ..)| matches!(a.role, Role::Wild { .. })).map(|(e, _, m, ..)| (e, m.feet, 0.0)).collect();
    let wild_hp = |e: Entity| wild_actors.get(e).ok().map(|(_, _, h)| h.hp);
    let _ = &targets;
    for (_e, actor, mut m, mut b, mut steer, body, slots) in &mut q {
        if actor.role != Role::Companion || shielding.contains(_e) {
            continue;
        }
        *steer = Steer::default();
        if slots.casting.is_some() {
            continue;
        }
        let pos_of = |t: Entity| wild_pos.iter().find(|(e, ..)| *e == t).map(|(_, p, _)| *p);
        // Who to fight.
        if party.direct_target.is_some_and(|t| wild_hp(t).is_none_or(|h| h <= 0.0)) {
            party.direct_target = None;
        }
        let assist = (battle.until > now).then_some(battle.target).flatten().filter(|t| wild_hp(*t).is_some_and(|h| h > 0.0));
        let warlike = (party.stance == Stance::Warlike)
            .then(|| wild_pos.iter().filter(|(_, p, _)| p.distance(pm.feet) < db_search_radius()).min_by(|a, b| a.1.distance(m.feet).total_cmp(&b.1.distance(m.feet))).map(|(e, ..)| *e))
            .flatten();
        let target = match party.stance {
            Stance::NotCombat => None,
            _ => party.direct_target.or(assist).or(warlike),
        };
        let dist_player = m.feet.distance(pm.feet);
        // Warp when far (beyond 65 m).
        if dist_player > 65.0 {
            m.feet = pm.feet - pm.forward() * 2.0 + Vec3::Y * 0.2;
            m.vel = Vec3::ZERO;
            b.note = "warped to the player".into();
            continue;
        }
        match target.and_then(|t| pos_of(t).map(|p| (t, p))) {
            Some((t, tp)) if tp.distance(pm.feet) < 45.0 => {
                b.state = AiState::Battle;
                b.target = Some(t);
                crate::skills::battle_steer(&sq, &m, &mut steer, body, slots, tp, &mut b, dt);
                // In battle a companion never moves faster than its run speed.
                steer.speed = steer.speed.min(body.run);
            }
            _ => {
                b.state = AiState::Follow;
                b.target = None;
                // A spot behind and to the right of the player.
                let fwd = pm.forward();
                let right = Vec3::new(-fwd.z, 0.0, fwd.x);
                let slot = pm.feet - fwd * 1.8 + right * 1.4;
                let to = (slot - m.feet).with_y(0.0);
                let d = to.length();
                if d > 1.0 {
                    let player_speed = pm.vel.with_y(0.0).length().max(body.walk);
                    // Catch up faster the further behind, up to 3x the player's speed.
                    let speed = if d > 6.0 { (player_speed * (d / 4.0)).min(player_speed * 3.0).max(body.run) } else { player_speed.max(body.walk * 3.0) };
                    steer.dir = to / d;
                    steer.speed = speed;
                    steer.jump = ploco.mode == crate::player::Mode::Air && pm.vel.y > 2.0 && d < 4.0;
                    b.note = format!("follow {d:.1} m");
                } else {
                    steer.face = Some(fwd);
                    b.note = "at the player's side".into();
                }
            }
        }
    }
}

/// Warlike stance: wild creatures within 20 m of the player are fair game.
fn db_search_radius() -> f32 {
    20.0
}

