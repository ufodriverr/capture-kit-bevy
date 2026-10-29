//! Inspector (F1): the numbers behind what's on screen. Player movement state and tuning, the creature
//! under the crosshair (or nearest) with its AI state, reason, skills and cooldowns, stats and talents,
//! the capture breakdown while aiming an orb, and the camp workers' tasks.

use bevy::prelude::*;

use crate::base::WorkerTask;
use crate::camera::{CameraRig, MainCamera};
use crate::capture::CapturePreview;
use crate::combat::Combatant;
use crate::data::Db;
use crate::hud::text;
use crate::input::PlayerInput;
use crate::kinematic::Mover;
use crate::creatures::{Brain, CreatureActor, Creatures};
use crate::player::{Health, Locomotion, Player, Stamina};
use crate::skills::SkillSlots;
use crate::tuning::MoveTuning;

#[derive(Resource, Debug, Default)]
pub struct Inspector {
    pub on: bool,
}

#[derive(Component, Debug)]
struct InspectorText;

pub struct InspectorPlugin;

impl Plugin for InspectorPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Inspector>().add_systems(Startup, spawn).add_systems(Update, update.after(crate::Sim::Last));
    }
}

fn spawn(mut commands: Commands) {
    commands.spawn((text("", 13.0), InspectorText, Node { position_type: PositionType::Absolute, right: px(16.0), top: px(220.0), width: px(660.0), padding: UiRect::all(px(8.0)), ..default() }, BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.55)), Visibility::Hidden, GlobalZIndex(5)));
}

fn update(
    input: Res<PlayerInput>,
    mut ins: ResMut<Inspector>,
    db: Res<Db>,
    tuning: Res<MoveTuning>,
    rig: Res<CameraRig>,
    preview: Res<CapturePreview>,
    roster: Res<Creatures>,
    player: Query<(&Mover, &Locomotion, &Stamina, &Health), With<Player>>,
    actors: Query<(Entity, &CreatureActor, &Mover, &Brain, &SkillSlots, &Health, &Combatant, Option<&WorkerTask>), Without<Player>>,
    cam: Query<&GlobalTransform, With<MainCamera>>,
    mut t: Query<(&mut Text, &mut Visibility), With<InspectorText>>,
) {
    if input.inspector_pressed {
        ins.on = !ins.on;
    }
    let Ok((mut t, mut vis)) = t.single_mut() else { return };
    *vis = if ins.on { Visibility::Inherited } else { Visibility::Hidden };
    if !ins.on {
        return;
    }
    let Ok((pm, loc, st, hp)) = player.single() else { return };
    let mut out = Vec::new();
    let h = pm.vel.with_y(0.0).length();
    out.push("INSPECTOR (F1)".to_owned());
    out.push(format!("player {:?} grounded={} h={h:.2} m/s vy={:.2} yaw={:.0} deg", loc.mode, pm.grounded, pm.vel.y, pm.yaw.to_degrees()));
    out.push(format!("  crouch={} sprint={} aim={} overweight={} glider={} roll cd {:.2}", loc.crouched, loc.sprinting, loc.aiming, loc.overweight, loc.has_glider, loc.roll_cooldown));
    out.push(format!("  SP {:.1}/{:.0} delay {:.2} exhausted={}  HP {:.0}/{:.0}  fall origin {:.1}", st.sp, st.max, st.delay, st.exhausted, hp.hp, hp.max, loc.fall_origin));
    out.push(format!("  tuning: walk {:.2} sprint {:.2} crouch {:.2} jump {:.2} gravity {:.2}", tuning.walk_speed, tuning.sprint_speed, tuning.crouch_speed, tuning.jump_speed, tuning.gravity));
    out.push(format!("  camera arm {:.2} fov {:.0} pitch {:.0} deg", rig.arm, rig.fov_deg, rig.pitch.to_degrees()));
    // The creature under the crosshair, else the nearest.
    let eye = cam.single().map(|c| (c.translation(), c.forward())).ok();
    let pick = actors
        .iter()
        .filter(|(.., m, _, _, _, _, _)| m.feet.distance(pm.feet) < 40.0)
        .min_by(|a, b| {
            let score = |m: &Mover| match eye {
                Some((o, f)) => {
                    let to = m.feet + Vec3::Y * 0.5 - o;
                    let ang = to.normalize_or_zero().angle_between(*f);
                    ang * 10.0 + to.length() * 0.02
                }
                None => m.feet.distance(pm.feet),
            };
            score(a.2).total_cmp(&score(b.2))
        });
    if let Some((e, a, m, b, slots, h, c, task)) = pick {
        out.push(String::new());
        out.push(format!("creature {} #{} {:?} {e}", c.name, a.id, a.role));
        out.push(format!("  AI {} timer {:.1} aware={} note: {}", b.state.name(), b.timer, b.aware, b.note));
        out.push(format!("  target {:?} goal {:?} d={:.1} m speed {:.2}", b.target, b.goal.map(|g| (g.x.round(), g.z.round())), m.feet.distance(pm.feet), m.vel.with_y(0.0).length()));
        out.push(format!("  HP {:.0}/{:.0} melee {:.0} shot {:.0} def {:.0} lv {}", h.hp, h.max, c.melee_attack, c.shot_attack, c.defense, c.level));
        if let Some(ind) = roster.0.get(a.id) {
            out.push(format!("  talents HP {} atk {} def {}  food {:.0} mood {:.0}", ind.talents.hp, ind.talents.shot, ind.talents.defense, ind.fullness, ind.mood));
        }
        for w in &slots.equipped {
            let name = db.skills.get(w).map_or(w.as_str(), |z| z.name.as_str());
            let cd = slots.cooldowns.get(w).copied().unwrap_or(0.0).max(0.0);
            out.push(format!("  skill {name}: {}", if cd > 0.0 { format!("cooldown {cd:.1} s") } else { "ready".into() }));
        }
        if let Some(cast) = &slots.casting {
            out.push(format!("  casting {} {:?} t={:.2} hit={}", cast.skill, cast.phase, cast.t, cast.hit_done));
        }
        if let Some(t) = task {
            out.push(format!("  worker task {} carrying {:?}", t.kind, t.carrying));
        }
    }
    if let Some((name, bd, sneak)) = &preview.target {
        out.push(String::new());
        out.push(format!("capture {name}: {:.2}%  back bonus={sneak}", bd.result * 100.0));
        out.push(format!("  hp factor {:.3}  level factor {:.3}", bd.hp_factor, bd.level_factor));
    }
    t.0 = out.join("\n");
}
