//! Burn and Poison on characters (Fire / Venom Arrows). Durations, tick damage, immunities and the
//! capture bonus come from rules.ron `statuses` ("burn", "poison"). An arrow hit always applies its
//! status and a new hit restarts the duration; tick damage stops at 1 HP (it never knocks a creature out).

use bevy::prelude::*;

use crate::combat::{Combatant, Damaged};
use crate::data::Db;
use crate::log::{GameLog, Say};
use crate::kinematic::Mover;
use crate::player::Health;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    Burn,
    Poison,
}

impl Status {
    /// The rules.ron `statuses` key.
    pub fn key(self) -> &'static str {
        match self {
            Status::Burn => "burn",
            Status::Poison => "poison",
        }
    }

    pub fn from_key(key: &str) -> Option<Self> {
        [Status::Burn, Status::Poison].into_iter().find(|s| s.key() == key)
    }

    fn def(self, db: &Db) -> Option<&ck_core::data::StatusDef> {
        db.rules().statuses.get(self.key())
    }

    pub fn duration(self, db: &Db) -> f32 {
        self.def(db).map_or(10.0, |d| d.duration)
    }

    pub fn name(self, db: &Db) -> String {
        self.def(db).map_or_else(|| self.key().to_owned(), |d| d.name.clone())
    }

    pub fn color(self, db: &Db) -> Color {
        self.def(db).and_then(|d| Srgba::hex(&d.color).ok()).map_or(Color::WHITE, Color::from)
    }
}

/// A projectile with a status hit `target` (combat.rs writes it, `inflict` applies it after the hit).
#[derive(Message, Debug, Clone, Copy)]
pub struct Inflict {
    pub target: Entity,
    pub attacker: Entity,
    pub status: Status,
}

/// One running status: seconds left, seconds to the next tick, who caused it.
#[derive(Debug, Clone, Copy)]
pub struct Running {
    pub status: Status,
    pub left: f32,
    pub next_tick: f32,
    pub source: Entity,
}

#[derive(Component, Debug, Default, Clone)]
pub struct Afflicted(pub Vec<Running>);

impl Afflicted {
    pub fn has(&self, s: Status) -> bool {
        self.0.iter().any(|r| r.status == s)
    }
}

/// The sum of the running statuses' `capture_bonus` (0 = none).
pub fn capture_bonus(db: &Db, a: Option<&Afflicted>) -> f64 {
    a.map_or(0.0, |a| a.0.iter().filter_map(|r| r.status.def(db)).map(|d| f64::from(d.capture_bonus)).sum())
}

/// The glow and puffs of a running status (child of the character).
#[derive(Component)]
struct Aura {
    owner: Entity,
    status: Status,
    puff: f32,
}

#[derive(Component)]
struct Puff {
    age: f32,
    vel: Vec3,
}

#[derive(Resource)]
struct StatusAssets {
    puff: Handle<Mesh>,
    burn: Handle<StandardMaterial>,
    poison: Handle<StandardMaterial>,
}

pub struct StatusPlugin;

impl Plugin for StatusPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<Inflict>()
            .add_systems(Startup, setup)
            .add_systems(Update, (inflict, tick).chain().in_set(crate::Sim::Combat).after(crate::combat::apply_hits))
            .add_systems(Update, (auras, puffs).after(crate::Sim::Last));
    }
}

fn setup(mut commands: Commands, db: Res<Db>, mut meshes: ResMut<Assets<Mesh>>, mut mats: ResMut<Assets<StandardMaterial>>) {
    let mat = |s: Status, mats: &mut Assets<StandardMaterial>| mats.add(StandardMaterial { base_color: s.color(&db), emissive: s.color(&db).to_linear() * 1.2, unlit: true, alpha_mode: AlphaMode::Blend, ..default() });
    commands.insert_resource(StatusAssets { puff: meshes.add(Sphere::new(0.09).mesh().ico(1).unwrap()), burn: mat(Status::Burn, &mut mats), poison: mat(Status::Poison, &mut mats) });
}

fn inflict(mut commands: Commands, time: Res<Time>, db: Res<Db>, mut log: ResMut<GameLog>, mut ev: MessageReader<Inflict>, mut q: Query<(&Health, &Combatant, Option<&mut Afflicted>)>) {
    for i in ev.read() {
        let Ok((hp, c, aff)) = q.get_mut(i.target) else { continue };
        if hp.hp <= 0.0 {
            continue;
        }
        // Immune elements (rules.ron: Fire and Water creatures don't burn).
        if let Some(el) = i.status.def(&db).and_then(|d| d.immune.iter().find(|e| c.elements.contains(e))) {
            log.say(&time, format!("{} is immune to {} ({} element)", c.name, i.status.name(&db).to_lowercase(), el.name()));
            continue;
        }
        let tick = i.status.def(&db).map_or(1.0, |d| d.tick_s);
        let run = Running { status: i.status, left: i.status.duration(&db), next_tick: tick, source: i.attacker };
        // A re-hit restarts the duration; a new status also gets its glow.
        let fresh = match aff {
            Some(mut a) => match a.0.iter_mut().find(|r| r.status == i.status) {
                Some(r) => {
                    r.left = i.status.duration(&db);
                    r.source = i.attacker;
                    false
                }
                None => {
                    a.0.push(run);
                    true
                }
            },
            None => {
                commands.entity(i.target).insert(Afflicted(vec![run]));
                true
            }
        };
        if fresh {
            log.say(&time, format!("{} is {}", c.name, i.status.name(&db).to_lowercase()));
            commands.spawn((Aura { owner: i.target, status: i.status, puff: 0.0 }, PointLight { color: i.status.color(&db), intensity: 60_000.0, range: 4.0, ..default() }, Transform::default()));
        }
    }
}

fn tick(mut commands: Commands, time: Res<Time>, db: Res<Db>, mut out: MessageWriter<Damaged>, mut q: Query<(Entity, &mut Health, &mut Afflicted)>) {
    let dt = time.delta_secs();
    for (e, mut hp, mut a) in &mut q {
        for r in a.0.iter_mut() {
            r.left -= dt;
            r.next_tick -= dt;
            if r.next_tick <= 0.0 && r.left > -dt && hp.hp > 1.0 {
                let (tick, pct) = r.status.def(&db).map_or((1.0, 1.0), |d| (d.tick_s.max(0.05), d.hp_percent));
                r.next_tick += tick;
                let dmg = (hp.max * pct / 100.0).trunc().max(1.0).min(hp.hp - 1.0);
                hp.hp -= dmg;
                out.write(Damaged { target: e, attacker: r.source, amount: dmg, killed: false, slip: Some(r.status) });
            }
        }
        // Gone when it runs out or the character is down.
        let down = hp.hp <= 0.0;
        a.0.retain(|r| r.left > 0.0 && !down);
        if a.0.is_empty() {
            commands.entity(e).remove::<Afflicted>();
        }
    }
}

fn auras(
    mut commands: Commands,
    time: Res<Time>,
    assets: Option<Res<StatusAssets>>,
    mut q: Query<(Entity, &mut Aura, &mut Transform, &mut PointLight)>,
    owners: Query<(&Mover, Option<&Afflicted>)>,
    mut seed: Local<u32>,
) {
    let Some(assets) = assets else { return };
    let now = time.elapsed_secs();
    for (e, mut au, mut tf, mut light) in &mut q {
        let Ok((m, a)) = owners.get(au.owner) else {
            commands.entity(e).despawn();
            continue;
        };
        if !a.is_some_and(|a| a.has(au.status)) {
            commands.entity(e).despawn();
            continue;
        }
        let mid = m.feet + Vec3::Y * (m.capsule.half_height + m.capsule.radius);
        tf.translation = mid;
        light.intensity = 60_000.0 * (0.75 + 0.25 * (now * if au.status == Status::Burn { 17.0 } else { 5.0 }).sin());
        au.puff -= time.delta_secs();
        if au.puff <= 0.0 {
            au.puff = if au.status == Status::Burn { 0.08 } else { 0.18 };
            *seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            let r = |k: u32| ((*seed >> k) & 0xff) as f32 / 255.0 - 0.5;
            let off = Vec3::new(r(0), r(8) * 0.6, r(16)) * m.capsule.radius * 2.2;
            let mat = if au.status == Status::Burn { assets.burn.clone() } else { assets.poison.clone() };
            let rise = if au.status == Status::Burn { 1.6 } else { 0.6 };
            commands.spawn((Puff { age: 0.0, vel: Vec3::new(r(4) * 0.3, rise, r(12) * 0.3) }, Mesh3d(assets.puff.clone()), MeshMaterial3d(mat), Transform::from_translation(mid + off)));
        }
    }
}

fn puffs(mut commands: Commands, time: Res<Time>, mut q: Query<(Entity, &mut Puff, &mut Transform)>) {
    let dt = time.delta_secs();
    for (e, mut p, mut tf) in &mut q {
        p.age += dt;
        if p.age > 0.6 {
            commands.entity(e).despawn();
            continue;
        }
        tf.translation += p.vel * dt;
        tf.scale = Vec3::splat(1.0 - p.age / 0.6);
    }
}
