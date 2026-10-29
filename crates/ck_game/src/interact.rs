//! Context interaction (F): the nearest interactable in front of the player gets the focus and a prompt;
//! pressing F sends `Interact(entity)`. Things with a menu (stations, chests, the Camp Core) open on a tap of F
//! instead, so holding F at a station works it without opening the menu. With nothing in focus, F is the
//! bond skill.

use bevy::prelude::*;

use crate::input::PlayerInput;
use crate::kinematic::Mover;
use crate::player::Player;

#[derive(Component, Debug, Clone)]
pub struct Interactable {
    pub prompt: String,
    pub radius: f32,
}

#[derive(Message, Debug, Clone, Copy)]
pub struct Interact(pub Entity);

#[derive(Resource, Debug, Default)]
pub struct Focus(pub Option<(Entity, String)>);

pub struct InteractPlugin;

impl Plugin for InteractPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<Interact>().init_resource::<Focus>().add_systems(Update, find_focus.in_set(crate::Sim::Player).after(crate::player::locomotion));
    }
}

/// Interactables that open a menu (sent on a tap, not on the press).
type HasMenu = Or<(With<crate::crafting::Station>, With<crate::building::Storage>, With<crate::building::BaseCamp>, With<crate::base_tools::RepairBench>, With<crate::base_tools::PriorityBoard>)>;

fn find_focus(input: Res<PlayerInput>, mut focus: ResMut<Focus>, mut out: MessageWriter<Interact>, player: Query<&Mover, With<Player>>, q: Query<(Entity, &GlobalTransform, &Interactable, &InheritedVisibility)>, menus: Query<(), HasMenu>) {
    let Ok(m) = player.single() else { return };
    let fwd = m.forward();
    let mut best: Option<(f32, Entity, &str)> = None;
    for (e, tf, it, vis) in &q {
        if !vis.get() {
            continue;
        }
        let d = tf.translation() - m.feet;
        let flat = d.with_y(0.0);
        let dist = flat.length();
        if dist > it.radius + m.capsule.radius || d.y.abs() > 2.5 {
            continue;
        }
        let facing = if dist > 0.01 { flat.dot(fwd) / dist } else { 1.0 };
        if facing < 0.2 {
            continue;
        }
        let score = dist - facing;
        if best.is_none_or(|b| score < b.0) {
            best = Some((score, e, &it.prompt));
        }
    }
    focus.0 = best.map(|(_, e, p)| (e, p.to_owned()));
    if let Some((e, _)) = &focus.0
        && (if menus.contains(*e) { input.interact_tapped } else { input.interact_pressed })
    {
        out.write(Interact(*e));
    }
}
