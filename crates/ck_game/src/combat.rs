//! Hits and damage: the player's melee swings and bow, projectiles, and applying a hit to a resource node or
//! a character. Damage numbers use `ck_core::stats::damage`; resource nodes use the rules.ron `node_*` rates.

use avian3d::prelude::*;
use bevy::prelude::*;
use ck_core::consts as ov;
use ck_core::stats::{Element, Hit, damage, element_multiplier};

use crate::data::Db;
use crate::input::PlayerInput;
use crate::inventory::Inventory;
use crate::kinematic::{Mover, world_filter};
use crate::log::{GameLog, Say};
use crate::player::{Health, Locomotion, Mode, Player};
use crate::resources::{ResourceNode, Tool, WorldRng, damage_node};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Faction {
    Player,
    /// Owned by the player (companions, camp workers).
    PlayerCreature,
    Wild,
}

impl Faction {
    pub fn hostile_to(self, other: Faction) -> bool {
        matches!((self, other), (Faction::Wild, Faction::Player | Faction::PlayerCreature) | (Faction::Player | Faction::PlayerCreature, Faction::Wild))
    }
}

/// Combat stats of a character.
#[derive(Component, Debug, Clone)]
pub struct Combatant {
    pub faction: Faction,
    pub level: u32,
    pub melee_attack: f32,
    pub shot_attack: f32,
    pub defense: f32,
    pub elements: Vec<Element>,
    /// Multiplier on damage taken from the player's side (1 = normal).
    pub receive_rate: f32,
    pub name: String,
}

/// Where a character can be hit: a vertical capsule from its feet.
#[derive(Component, Debug, Clone, Copy)]
pub struct Hurtbox {
    pub radius: f32,
    pub height: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HitKind {
    Melee,
    Shot,
    Skill,
}

/// A landed hit, applied by `apply_hits`.
#[derive(Message, Debug, Clone)]
pub struct HitMsg {
    pub target: Entity,
    pub attacker: Entity,
    pub power: f32,
    pub kind: HitKind,
    pub element: Element,
    pub tool: Tool,
    /// Knockback direction (stagger), world space.
    pub push: Vec3,
}

/// Something took damage (for AI reactions and UI).
#[derive(Message, Debug, Clone, Copy)]
pub struct Damaged {
    pub target: Entity,
    pub attacker: Entity,
    pub amount: f32,
    pub killed: bool,
    /// Burn / Poison tick (status.rs) rather than a hit.
    pub slip: Option<crate::status::Status>,
}

#[derive(Component, Debug, Default)]
pub struct Swing {
    pub t: f32,
    pub hit_done: bool,
}

/// Bow reload in progress (seconds left).
#[derive(Component, Debug, Default)]
pub struct Reload(pub f32);

#[derive(Component, Debug)]
pub struct Projectile {
    pub vel: Vec3,
    pub attacker: Entity,
    pub power: f32,
    pub kind: HitKind,
    pub element: Element,
    pub life: f32,
    pub gravity: f32,
    pub radius: f32,
    pub faction: Faction,
    /// Status the hit applies (Fire / Poison Arrows).
    pub status: Option<crate::status::Status>,
}

pub struct CombatPlugin;

impl Plugin for CombatPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<HitMsg>().add_message::<Damaged>().add_systems(Startup, setup_player_combat.after(crate::player::spawn_player)).add_systems(Update, (player_attack, projectiles, apply_hits).chain().in_set(crate::Sim::Combat));
    }
}

/// Arrow drop (m/s^2), lighter than real gravity.
const ARROW_GRAVITY: f32 = 4.0;

#[derive(Resource, Debug, Clone)]
pub struct CombatAssets {
    pub arrow: Handle<Mesh>,
    pub arrow_mat: Handle<StandardMaterial>,
    pub arrow_fire_mat: Handle<StandardMaterial>,
    pub arrow_poison_mat: Handle<StandardMaterial>,
    pub ball: Handle<Mesh>,
}

fn setup_player_combat(mut commands: Commands, db: Res<Db>, progress: Res<crate::progress::Progress>, q: Query<Entity, With<Player>>, mut meshes: ResMut<Assets<Mesh>>, mut mats: ResMut<Assets<StandardMaterial>>) {
    let r = db.rules();
    for e in &q {
        commands.entity(e).insert((
            Combatant { faction: Faction::Player, level: progress.level, melee_attack: r.player_attack, shot_attack: r.player_attack, defense: r.player_defense, elements: vec![], receive_rate: 1.0, name: "Player".into() },
            Hurtbox { radius: 0.34, height: 1.76 },
        ));
    }
    commands.insert_resource(CombatAssets { arrow: meshes.add(Cuboid::new(0.04, 0.04, 0.7)), arrow_mat: mats.add(Color::srgb(0.6, 0.45, 0.25)),
        arrow_fire_mat: mats.add(StandardMaterial { base_color: Color::srgb(1.0, 0.45, 0.1), emissive: LinearRgba::rgb(4.0, 1.2, 0.2), ..default() }),
        arrow_poison_mat: mats.add(StandardMaterial { base_color: Color::srgb(0.55, 0.2, 0.75), emissive: LinearRgba::rgb(1.2, 0.3, 1.8), ..default() }), ball: meshes.add(Sphere::new(1.0).mesh().ico(2).unwrap()) });
}

/// Characters whose hurtbox overlaps an orb.
pub fn overlap_characters<'a>(center: Vec3, radius: f32, q: impl Iterator<Item = (Entity, &'a Mover, &'a Hurtbox)>) -> Vec<Entity> {
    q.filter(|(_, m, h)| {
        let y = center.y.clamp(m.feet.y, m.feet.y + h.height);
        center.distance(Vec3::new(m.feet.x, y, m.feet.z)) < radius + h.radius
    })
    .map(|(e, _, _)| e)
    .collect()
}

/// Rough radius of a node for melee contact.
fn node_radius(n: &ResourceNode, scale: Vec3) -> f32 {
    match n.kind {
        crate::resources::NodeKind::Tree => 0.45,
        _ => scale.x.max(scale.z) * 1.0,
    }
}

fn player_attack(
    mut commands: Commands,
    time: Res<Time>,
    input: Res<PlayerInput>,
    db: Res<Db>,
    assets: Res<CombatAssets>,
    rig: Res<crate::camera::CameraRig>,
    focus: Res<crate::input::UiFocus>,
    mut log: ResMut<GameLog>,
    mut hits: MessageWriter<HitMsg>,
    mut player: Query<(Entity, &Mover, &mut Locomotion, &mut Inventory, Option<&mut Swing>, Option<&mut Reload>), With<Player>>,
    others: Query<(Entity, &Mover, &Hurtbox), Without<Player>>,
    nodes: Query<(Entity, &GlobalTransform, &ResourceNode)>,
    (placing, dis): (Res<crate::building::Placing>, Res<crate::building::Dismantle>),
    (sq, cam): (SpatialQuery, Query<&GlobalTransform, With<crate::camera::MainCamera>>),
    (mut r_hold, mut picker): (Local<f32>, ResMut<crate::inventory::ArrowPicker>),
) {
    let Ok((pe, m, mut loco, mut inv, swing, reload)) = player.single_mut() else { return };
    let dt = time.delta_secs();
    let weapon = inv.weapon().cloned();
    let item = weapon.as_ref().and_then(|s| db.item(&s.item));
    let is_bow = item.is_some_and(|i| i.sub == "Bow");
    let busy = !matches!(loco.mode, Mode::Ground | Mode::Air) || focus.0 || placing.id.is_some() || dis.on;

    // Bow: aim + click shoots when loaded; reload from the chosen arrow type (Arrow, Fire, Poison).
    // Holding R for rules.ron `long_press` (0.25 s) opens the arrow list; the mouse wheel moves the pick,
    // releasing R takes it.
    if is_bow {
        if input.reload_held && !busy {
            *r_hold += dt;
            if *r_hold >= db.rules().long_press && !picker.open {
                picker.open = true;
                picker.sel = crate::inventory::ARROWS.iter().position(|a| *a == inv.ammo).unwrap_or(0);
            }
            if picker.open && input.weapon_scroll != 0 {
                picker.sel = (picker.sel as i32 + input.weapon_scroll).rem_euclid(crate::inventory::ARROWS.len() as i32) as usize;
            }
        } else {
            if picker.open {
                picker.open = false;
                let want = crate::inventory::ARROWS[picker.sel];
                if want != inv.ammo {
                    let name = db.item(want).map_or(want, |i| i.name.as_str());
                    if inv.select_ammo(&db, want) {
                        commands.entity(pe).remove::<Reload>();
                        log.say(&time, format!("arrows: {name} x{}", inv.count(want)));
                    } else {
                        log.say(&time, format!("arrows rejected: no {name} in the bag"));
                    }
                }
            }
            *r_hold = 0.0;
        }
        inv.ensure_ammo();
        let ammo = inv.ammo.clone();
        if let Some(mut r) = reload {
            r.0 -= dt;
            if r.0 <= 0.0 {
                commands.entity(pe).remove::<Reload>();
                if inv.bag.remove(&ammo, 1) {
                    inv.loaded = 1;
                }
            }
        } else if inv.loaded == 0 && inv.count(&ammo) > 0 {
            commands.entity(pe).insert(Reload(ov::BOW_RELOAD_S));
        }
        if input.attack_pressed && !busy {
            let broken = weapon.as_ref().and_then(|s| s.durability).is_some_and(|d| d <= 0.0);
            if broken {
                log.say(&time, "shot rejected: weapon broken");
            } else if inv.loaded == 0 {
                log.say(&time, if inv.count(&ammo) == 0 { "shot rejected: no arrows" } else { "shot rejected: reloading" });
            } else {
                inv.loaded = 0;
                wear(&mut inv);
                let from = m.feet + Vec3::Y * 1.45 + kinematic_right(rig.yaw) * 0.25;
                // Toward the point under the crosshair (the camera sits off to the side, so a ray parallel to
                // the view from the bow would miss it), lifted for the arrow's drop.
                let dir = match cam.single() {
                    Ok(ctf) => launch_dir(from, crosshair(&sq, ctf, pe, m.feet + Vec3::Y * 1.2, others.iter()).0, ov::ARROW_SPEED, ARROW_GRAVITY),
                    Err(_) => Quat::from_euler(EulerRot::YXZ, rig.yaw, rig.pitch, 0.0) * Vec3::NEG_Z,
                };
                // The arrow's element and status come from its item (items.ron); the status picks the colour.
                let ammo_item = db.item(&ammo);
                let element = ammo_item.and_then(|i| i.element).unwrap_or(Element::None);
                let status = ammo_item.and_then(|i| i.status.as_deref()).and_then(crate::status::Status::from_key);
                let mat = match status {
                    Some(crate::status::Status::Burn) => &assets.arrow_fire_mat,
                    Some(crate::status::Status::Poison) => &assets.arrow_poison_mat,
                    None if element == Element::Fire => &assets.arrow_fire_mat,
                    None => &assets.arrow_mat,
                };
                commands.spawn((
                    Projectile { vel: dir * ov::ARROW_SPEED, attacker: pe, power: item.map_or(0.0, |i| i.attack), kind: HitKind::Shot, element, life: 4.0, gravity: ARROW_GRAVITY, radius: 0.08, faction: Faction::Player, status },
                    Mesh3d(assets.arrow.clone()),
                    MeshMaterial3d(mat.clone()),
                    Transform::from_translation(from).looking_to(dir, Vec3::Y),
                ));
            }
        }
        return;
    }

    match swing {
        None => {
            if input.attack_pressed && !busy && !input.aim {
                if weapon.as_ref().and_then(|s| s.durability).is_some_and(|d| d <= 0.0) {
                    log.say(&time, "attack rejected: weapon broken");
                } else {
                    commands.entity(pe).insert(Swing::default());
                    loco.action_lock = loco.action_lock.max(ov::SWING_HIT_AT_S);
                }
            }
        }
        Some(mut s) => {
            s.t += dt;
            if !s.hit_done && s.t >= ov::SWING_HIT_AT_S {
                s.hit_done = true;
                let power = item.map_or(ov::FIST_ATTACK, |i| i.attack);
                let tool = weapon.as_ref().map_or(Tool::None, |w| Tool::of(&w.item));
                // The swing goes where the crosshair is: toward its point (pitch included), and whatever the
                // crosshair is on is hit when it is within reach.
                let chest = m.feet + Vec3::Y * 1.2;
                let (aim_at, aim_on) = cam.single().map_or((chest + m.forward() * 10.0, None), |ctf| crosshair(&sq, ctf, pe, chest, others.iter()));
                let dir = (aim_at - chest).normalize_or(m.forward());
                let center = chest + dir * ov::MELEE_REACH_M;
                let direct = aim_on.filter(|_| aim_at.distance(chest) < ov::MELEE_REACH_M + ov::MELEE_RADIUS_M + 0.6);
                let push = dir.with_y(0.0).normalize_or(m.forward());
                let mut struck: Vec<Entity> = overlap_characters(center, ov::MELEE_RADIUS_M, others.iter());
                for (e, tf, n) in &nodes {
                    if !n.alive() || n.max_hp <= 0.0 {
                        continue;
                    }
                    let (scale, _, pos) = tf.to_scale_rotation_translation();
                    let c = Vec3::new(pos.x, center.y.clamp(pos.y - 0.5, pos.y + 3.0), pos.z);
                    if c.distance(center) < ov::MELEE_RADIUS_M + node_radius(n, scale) {
                        struck.push(e);
                    }
                }
                if let Some(e) = direct
                    && !struck.contains(&e)
                    && (others.contains(e) || nodes.get(e).is_ok_and(|(_, _, n)| n.alive() && n.max_hp > 0.0))
                {
                    struck.push(e);
                }
                let any = !struck.is_empty();
                for e in struck {
                    let push = if nodes.contains(e) { Vec3::ZERO } else { push };
                    hits.write(HitMsg { target: e, attacker: pe, power, kind: HitKind::Melee, element: Element::None, tool, push });
                }
                if any {
                    wear(&mut inv);
                }
            }
            if s.t >= ov::SWING_TIME_S {
                commands.entity(pe).remove::<Swing>();
            }
        }
    }
}

/// Where the crosshair points: the camera's forward ray against characters (upright cylinders) and the
/// world. Only things past the player count (not a wall between the camera and the player). Returns the
/// point (120 m out when nothing is hit) and what was hit.
pub fn crosshair<'a>(sq: &SpatialQuery, cam: &GlobalTransform, player: Entity, chest: Vec3, chars: impl Iterator<Item = (Entity, &'a Mover, &'a Hurtbox)>) -> (Vec3, Option<Entity>) {
    const FAR: f32 = 120.0;
    let o = cam.translation();
    let d = cam.forward();
    let start = ((chest - o).dot(*d) - 0.3).max(0.0);
    let mut best: (f32, Option<Entity>) = (FAR, None);
    if let Some(h) = sq.cast_ray(o + *d * start, d, FAR - start, true, &world_filter(&[player])) {
        best = (start + h.distance, Some(h.entity));
    }
    for (e, m, hb) in chars {
        if e != player
            && let Some(t) = ray_cylinder(o, *d, m.feet, hb.radius, hb.height)
            && t >= start
            && t < best.0
        {
            best = (t, Some(e));
        }
    }
    (o + *d * best.0, best.1)
}

/// Distance along a ray to the side of an upright cylinder (base centre, radius, height), if it hits.
pub fn ray_cylinder(o: Vec3, d: Vec3, base: Vec3, r: f32, h: f32) -> Option<f32> {
    let p = Vec2::new(o.x - base.x, o.z - base.z);
    let v = Vec2::new(d.x, d.z);
    let a = v.dot(v);
    if a < 1e-8 {
        return None;
    }
    let b = 2.0 * p.dot(v);
    let c = p.dot(p) - r * r;
    let disc = b * b - 4.0 * a * c;
    if disc < 0.0 {
        return None;
    }
    let s = disc.sqrt();
    [(-b - s) / (2.0 * a), (-b + s) / (2.0 * a)].into_iter().find(|t| *t >= 0.0 && (base.y..=base.y + h).contains(&(o.y + d.y * t)))
}

/// Launch direction for a projectile at `speed` under gravity `g` to pass through `to` (the low arc; 45
/// degrees when out of range).
pub fn launch_dir(from: Vec3, to: Vec3, speed: f32, g: f32) -> Vec3 {
    let delta = to - from;
    let flat = delta.with_y(0.0);
    let x = flat.length();
    if x < 0.05 || g <= 0.0 {
        return delta.normalize_or(Vec3::NEG_Z);
    }
    let v2 = speed * speed;
    let disc = v2 * v2 - g * (g * x * x + 2.0 * delta.y * v2);
    let tan = if disc >= 0.0 { (v2 - disc.sqrt()) / (g * x) } else { 1.0 };
    (flat / x + Vec3::Y * tan).normalize()
}

fn kinematic_right(yaw: f32) -> Vec3 {
    let f = crate::kinematic::yaw_forward(yaw);
    Vec3::new(-f.z, 0.0, f.x)
}

/// Durability loss on the equipped weapon.
fn wear(inv: &mut Inventory) {
    let sel = inv.selected;
    if let Some(Some(w)) = inv.weapons.get_mut(sel)
        && let Some(d) = w.durability.as_mut()
    {
        *d = (*d - ov::DURABILITY_PER_HIT).max(0.0);
    }
}

fn projectiles(
    mut commands: Commands,
    time: Res<Time>,
    sq: SpatialQuery,
    (mut hits, mut inflict): (MessageWriter<HitMsg>, MessageWriter<crate::status::Inflict>),
    mut q: Query<(Entity, &mut Projectile, &mut Transform)>,
    chars: Query<(Entity, &Mover, &Hurtbox, &Combatant)>,
    nodes: Query<(Entity, &ResourceNode)>,
) {
    let dt = time.delta_secs();
    for (e, mut p, mut tf) in &mut q {
        p.life -= dt;
        p.vel.y -= p.gravity * dt;
        let from = tf.translation;
        let step = p.vel * dt;
        let to = from + step;
        // Characters first (sampled along the segment).
        let mut hit_char = None;
        for i in 0..=4 {
            let at = from.lerp(to, i as f32 / 4.0);
            let found = overlap_characters(at, p.radius, chars.iter().filter(|(ce, _, _, c)| *ce != p.attacker && p.faction.hostile_to(c.faction)).map(|(a, b, c, _)| (a, b, c)));
            if let Some(c) = found.first() {
                hit_char = Some(*c);
                break;
            }
        }
        if let Some(c) = hit_char {
            hits.write(HitMsg { target: c, attacker: p.attacker, power: p.power, kind: p.kind, element: p.element, tool: Tool::None, push: p.vel.normalize_or_zero() });
            if let Some(status) = p.status {
                inflict.write(crate::status::Inflict { target: c, attacker: p.attacker, status });
            }
            commands.entity(e).despawn();
            continue;
        }
        if let Ok(dir) = Dir3::new(step.normalize_or_zero())
            && let Some(h) = sq.cast_ray(from, dir, step.length(), true, &world_filter(&[]))
        {
            if nodes.contains(h.entity) && p.faction == Faction::Player {
                hits.write(HitMsg { target: h.entity, attacker: p.attacker, power: p.power, kind: p.kind, element: p.element, tool: Tool::None, push: Vec3::ZERO });
            }
            commands.entity(e).despawn();
            continue;
        }
        tf.translation = to;
        if p.vel.length_squared() > 0.01 {
            tf.look_to(p.vel, Vec3::Y);
        }
        if p.life <= 0.0 {
            commands.entity(e).despawn();
        }
    }
}

pub fn apply_hits(
    time: Res<Time>,
    db: Res<Db>,
    mut rng: ResMut<WorldRng>,
    mut log: ResMut<GameLog>,
    mut ev: MessageReader<HitMsg>,
    mut out: MessageWriter<Damaged>,
    mut targets: Query<(&mut Health, &Combatant)>,
    attackers: Query<&Combatant>,
    mut nodes: Query<&mut ResourceNode>,
    mut player_inv: Query<&mut Inventory, With<Player>>,
    shield: Query<&crate::partner::ShieldGuard>,
) {
    for h in ev.read() {
        // Resource nodes: only the player's hits give items.
        if let Ok(mut n) = nodes.get_mut(h.target) {
            let Ok(mut inv) = player_inv.single_mut() else { continue };
            let rate = if h.tool != Tool::None && h.tool == n.kind.tool() {
                db.rules().node_tool_rate
            } else if h.kind == HitKind::Skill {
                db.rules().node_skill_rate
            } else {
                db.rules().node_weapon_rate
            };
            let dmg = (h.power * rate - n.defense).max(db.rules().node_min_damage).floor();
            damage_node(&db, &mut rng.0, &mut n, dmg, &mut inv, &mut log, &time);
            continue;
        }
        let Ok(att) = attackers.get(h.attacker).cloned() else { continue };
        // Shield bond skill (Shell Guard): the shielding companion takes a hit from the front (partner.rs).
        let mut target = h.target;
        if let Ok(s) = shield.get(h.target)
            && s.blocks(h.push)
        {
            target = s.creature;
            log.say(&time, format!("{}'s hit caught by the shield", att.name));
        }
        let Ok((mut hp, tgt)) = targets.get_mut(target) else { continue };
        if hp.hp <= 0.0 {
            continue;
        }
        let attack = if h.kind == HitKind::Shot || (h.kind == HitKind::Skill && att.faction != Faction::Player) { att.shot_attack } else { att.melee_attack };
        let hit = Hit {
            power: h.power,
            attack,
            defense: tgt.defense,
            element_match: h.element != Element::None && att.elements.contains(&h.element),
            element_mult: element_multiplier(db.rules(), h.element, &tgt.elements),
            is_skill: h.kind == HitKind::Skill,
            creature_to_player: att.faction != Faction::Player && tgt.faction == Faction::Player,
            weak_point: false,
            target_asleep: false,
            extra_rate: if att.faction != Faction::Wild { tgt.receive_rate } else { 1.0 },
        };
        let dmg = damage(db.rules(), &hit);
        hp.hp = (hp.hp - dmg).max(0.0);
        let killed = hp.hp <= 0.0;
        log.say(&time, format!("{} hit {} for {dmg:.0}{}", att.name, tgt.name, if killed { " (down)" } else { "" }));
        out.write(Damaged { target, attacker: h.attacker, amount: dmg, killed, slip: None });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Flies a projectile like `projectiles` does (60 fps) and returns its closest pass to `to`.
    fn miss(from: Vec3, to: Vec3, speed: f32, g: f32) -> f32 {
        let (mut p, mut v) = (from, launch_dir(from, to, speed, g) * speed);
        let mut best = f32::MAX;
        for _ in 0..600 {
            v.y -= g / 60.0;
            let q = p + v / 60.0;
            // Distance from the target to this frame's segment.
            let seg = q - p;
            let t = ((to - p).dot(seg) / seg.length_squared()).clamp(0.0, 1.0);
            best = best.min((p + seg * t).distance(to));
            p = q;
        }
        best
    }

    #[test]
    fn launch_dir_lands_on_the_target() {
        let from = Vec3::new(0.0, 1.45, 0.0);
        for to in [Vec3::new(0.0, 1.0, -30.0), Vec3::new(12.0, 4.0, -20.0), Vec3::new(-5.0, 0.0, 6.0)] {
            assert!(miss(from, to, ov::ARROW_SPEED, ARROW_GRAVITY) < 0.25, "arrow to {to}");
            assert!(miss(from, to, crate::capture::THROW_SPEED, crate::capture::THROW_GRAVITY) < 0.35, "orb to {to}");
        }
    }

    #[test]
    fn ray_cylinder_side_hits() {
        let base = Vec3::new(0.0, 0.0, -10.0);
        assert!((ray_cylinder(Vec3::new(0.0, 1.0, 0.0), Vec3::NEG_Z, base, 0.5, 1.8).unwrap() - 9.5).abs() < 1e-4);
        assert!(ray_cylinder(Vec3::new(0.0, 2.5, 0.0), Vec3::NEG_Z, base, 0.5, 1.8).is_none());
        assert!(ray_cylinder(Vec3::new(0.8, 1.0, 0.0), Vec3::NEG_Z, base, 0.5, 1.8).is_none());
    }
}
