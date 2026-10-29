//! Bond skills (F with nothing to interact with), from the companion's `bond` (bonds.ron). A `Shield` bond
//! (Mossnut's Shell Guard): the companion holds itself on the player's left arm as a shield for `duration`
//! seconds (F again ends it early), then `cooldown`. Hits from the front half are taken by the companion
//! instead of the player.

use bevy::prelude::*;
use ck_core::data::BondKind;

use crate::companion::Party;
use crate::data::Db;
use crate::input::{PlayerInput, UiFocus};
use crate::interact::Focus;
use crate::kinematic::Mover;
use crate::log::{GameLog, Say};
use crate::creatures::{CreatureActor, Steer};
use crate::player::Player;

/// On the player while the shield is up.
#[derive(Component, Debug, Clone)]
pub struct ShieldGuard {
    pub creature: Entity,
    pub facing: Vec3,
}

impl ShieldGuard {
    /// `push` is the hit's travel direction: it's blocked when it comes at the player's front.
    pub fn blocks(&self, push: Vec3) -> bool {
        push.length_squared() > 1e-4 && push.with_y(0.0).normalize_or_zero().dot(self.facing) < -0.2
    }
}

/// On the companion while it is the shield.
#[derive(Component, Debug)]
pub struct Shielding;

#[derive(Resource, Debug, Default)]
pub struct PartnerState {
    pub remaining: f32,
    pub cooldown: f32,
}

pub struct PartnerPlugin;

impl Plugin for PartnerPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<PartnerState>().add_systems(Update, toggle.in_set(crate::Sim::Creatures).before(crate::companion::companion_ai)).add_systems(Update, hold_shield.in_set(crate::Sim::Creatures).after(crate::creatures::creature_move));
    }
}

/// The companion species' bond: (name, kind).
fn bond_of(db: &Db, species: &str) -> Option<(String, BondKind)> {
    let b = db.bonds.get(db.creatures.get(species)?.bond.as_ref()?)?;
    Some((b.name.clone(), b.kind.clone()))
}

fn toggle(
    mut commands: Commands,
    time: Res<Time>,
    input: Res<PlayerInput>,
    focus: Res<Focus>,
    ui: Res<UiFocus>,
    db: Res<Db>,
    party: Res<Party>,
    mut st: ResMut<PartnerState>,
    mut log: ResMut<GameLog>,
    player: Query<(Entity, &Mover, Option<&ShieldGuard>), With<Player>>,
    actors: Query<(&CreatureActor, &crate::player::Health)>,
) {
    let dt = time.delta_secs();
    st.cooldown = (st.cooldown - dt).max(0.0);
    let Ok((pe, pm, guard)) = player.single() else { return };
    if let Some(g) = guard {
        st.remaining -= dt;
        let creature_ok = actors.get(g.creature).is_ok_and(|(_, h)| h.hp > 0.0);
        if !creature_ok || st.remaining <= 0.0 || (input.partner_pressed && focus.0.is_none() && !ui.0) {
            let bond = actors.get(g.creature).ok().and_then(|(a, _)| bond_of(&db, &a.species));
            commands.entity(pe).remove::<ShieldGuard>();
            commands.entity(g.creature).remove::<Shielding>();
            st.remaining = 0.0;
            st.cooldown = match &bond {
                Some((_, BondKind::Shield { cooldown, .. })) => *cooldown,
                _ => 10.0,
            };
            let name = bond.map_or("Shield".into(), |b| b.0);
            log.say(&time, if creature_ok { format!("{name} ended") } else { format!("{name} broken") });
        }
        return;
    }
    if !input.partner_pressed || focus.0.is_some() || ui.0 {
        return;
    }
    let Some(creature) = party.active_entity else {
        log.say(&time, "bond skill rejected: no creature summoned");
        return;
    };
    let Ok((a, _)) = actors.get(creature) else { return };
    let Some((name, BondKind::Shield { duration, .. })) = bond_of(&db, &a.species) else {
        log.say(&time, format!("bond skill rejected: {} has no bond skill you can trigger", a.species));
        return;
    };
    if st.cooldown > 0.0 {
        log.say(&time, format!("bond skill rejected: cooldown {:.1} s", st.cooldown));
        return;
    }
    st.remaining = duration;
    commands.entity(pe).insert(ShieldGuard { creature, facing: pm.forward() });
    commands.entity(creature).insert(Shielding);
    let who = db.creatures.get(&a.species).map_or(a.species.clone(), |s| s.name.clone());
    log.say(&time, format!("{name}: {who} becomes a shield"));
}

/// Keeps the shield companion on the player's left forearm and the guard's facing current.
fn hold_shield(mut player: Query<(&Mover, &mut ShieldGuard), With<Player>>, mut roster: Query<(&mut Mover, &mut Transform, &mut Steer), (With<Shielding>, Without<Player>)>) {
    let Ok((pm, mut g)) = player.single_mut() else { return };
    g.facing = pm.forward();
    let Ok((mut m, mut tf, mut steer)) = roster.get_mut(g.creature) else { return };
    let fwd = pm.forward();
    let left = Vec3::new(fwd.z, 0.0, -fwd.x);
    m.feet = pm.feet + fwd * 0.45 + left * 0.35 + Vec3::Y * 0.55;
    m.vel = Vec3::ZERO;
    m.yaw = pm.yaw;
    *steer = Steer { locked: true, ..default() };
    tf.translation = m.feet;
    tf.rotation = Quat::from_rotation_y(pm.yaw) * Quat::from_rotation_x(-1.2);
}
