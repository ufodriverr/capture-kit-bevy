//! Sound effects. The clips are original sounds generated for this project with ElevenLabs
//! (`assets/sfx`, made by tools/sfx). Most sounds follow the event log, the same
//! accepted/rejected lines the kit already prints, so gameplay code stays unaware of audio. Hits,
//! swings, throws, footsteps and UI clicks come from their own state. F3 mutes; hidden runs start muted.

use std::collections::HashMap;

use bevy::audio::Volume;
use bevy::prelude::*;

use crate::capture::OrbBall;
use crate::combat::{Damaged, HitMsg, Swing};
use crate::input::{PlayerInput, UiFocus};
use crate::kinematic::Mover;
use crate::log::GameLog;
use crate::creatures::CreatureActor;
use crate::player::{Locomotion, Mode, Player};
use crate::resources::{NodeKind, ResourceNode};

const CLIPS: &[&str] = &[
    "ui_click", "ui_open", "swing", "hit", "creature_cry", "pickaxe", "axe", "node_break", "pickup", "throw", "shake", "capture", "breakfree", "build_place", "build_done", "craft_done", "levelup", "step", "glide", "splash", "summon", "eat",
];

/// Sounds further than this from the player are not played.
const HEAR_M: f32 = 40.0;

#[derive(Resource, Default)]
struct Clips(HashMap<&'static str, Handle<AudioSource>>);

#[derive(Resource, Default)]
struct Mixer {
    /// Log lines already turned into sounds (`GameLog::total`).
    seen: usize,
    /// Last play time per clip, so bursts (ten pickups in one frame) don't stack.
    last: HashMap<&'static str, f32>,
    /// The player rig's footfall count last frame.
    footfalls: u32,
    rng: u32,
    muted: bool,
    /// Sounds chosen per clip (printed on exit, so muted scripted runs show what would have played).
    counts: HashMap<&'static str, u32>,
}

impl Mixer {
    /// Uniform in [0, 1) (xorshift; its own state so the world RNG and scripted runs stay unchanged).
    fn rand(&mut self) -> f32 {
        let mut x = self.rng.max(1);
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.rng = x;
        (x >> 8) as f32 / (1u32 << 24) as f32
    }
}

/// One sound to play: clip, volume, speed (pitch).
struct Play(&'static str, f32, f32);

pub struct SfxPlugin {
    pub muted: bool,
}

impl Plugin for SfxPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Clips>()
            .insert_resource(Mixer { rng: 0x9E37_79B9, muted: self.muted, ..default() })
            .insert_resource(GlobalVolume::new(if self.muted { Volume::SILENT } else { Volume::Linear(1.0) }))
            .add_systems(Startup, load)
            .add_systems(Update, (toggle, play).chain().after(crate::Sim::Last))
            .add_systems(Last, report);
    }
}

fn load(assets: Res<AssetServer>, mut clips: ResMut<Clips>) {
    for c in CLIPS {
        clips.0.insert(c, assets.load(format!("sfx/{c}.ogg")));
    }
}

fn toggle(input: Res<PlayerInput>, mut mixer: ResMut<Mixer>, mut global: ResMut<GlobalVolume>, time: Res<Time>, mut log: ResMut<GameLog>, mut settings: ResMut<crate::settings::Settings>) {
    if input.mute_pressed {
        mixer.muted = !mixer.muted;
        global.volume = if mixer.muted { Volume::SILENT } else { Volume::Linear(1.0) };
        settings.muted = mixer.muted;
        settings.save();
        crate::log::Say::say(&mut *log, &time, if mixer.muted { "sound off (F3)" } else { "sound on (F3)" });
    }
}

pub fn is_muted(world: &World) -> bool {
    world.resource::<Mixer>().muted
}

pub fn set_muted(world: &mut World, muted: bool) {
    world.resource_mut::<Mixer>().muted = muted;
    world.resource_mut::<GlobalVolume>().volume = if muted { Volume::SILENT } else { Volume::Linear(1.0) };
}

/// The sound for an event-log line, if any.
fn for_line(line: &str) -> Option<Play> {
    let has = |s: &str| line.contains(s);
    Some(if line.starts_with("captured ") {
        Play("capture", 0.8, 1.0)
    } else if line.starts_with("shake ") && !line.starts_with("shake rolls") {
        Play("shake", 0.6, 1.0)
    } else if has(" broke free") || line.starts_with("capture failed") {
        Play("breakfree", 0.7, 1.0)
    } else if line.starts_with("player level") || line.starts_with("camp level") {
        Play("levelup", 0.7, 1.0)
    } else if has(" reached level ") {
        Play("levelup", 0.4, 1.15)
    } else if line.starts_with("slept ") || line.starts_with("repaired ") {
        Play("craft_done", 0.6, 0.9)
    } else if line.starts_with("lifted ") || line.starts_with("put ") {
        Play("summon", 0.35, 1.4)
    } else if line.starts_with("dismantled ") {
        Play("build_place", 0.6, 0.75)
    } else if line.starts_with("placed ") {
        Play("build_place", 0.6, 1.0)
    } else if line.ends_with(" finished") {
        Play("build_done", 0.6, 1.0)
    } else if (has(" ready") && has(": ")) || has(": ready to harvest") {
        Play("craft_done", 0.55, 1.0)
    } else if line.starts_with("summoned ") {
        Play("summon", 0.6, 1.0)
    } else if line.starts_with("recalled ") {
        Play("summon", 0.45, 1.35)
    } else if line.starts_with("ate ") {
        Play("eat", 0.6, 1.0)
    } else if has(" ate ") {
        Play("eat", 0.3, 1.2)
    } else if line.ends_with(" destroyed") {
        Play("node_break", 0.6, 1.0)
    } else if line.ends_with(" defeated") || line.ends_with(" is incapacitated") {
        Play("creature_cry", 0.5, 0.8)
    } else if line.ends_with("-> Glide") {
        Play("glide", 0.5, 1.0)
    } else if line.ends_with("-> Swim") {
        Play("splash", 0.6, 1.0)
    } else if (line.starts_with('+') && !line.ends_with(')')) || has(": harvested ") {
        // "+3 Wood" (an XP gain ends with "(why)").
        Play("pickup", 0.4, 1.0)
    } else if has("rejected") {
        Play("ui_click", 0.28, 0.6)
    } else if line.starts_with("weapon slot") || line.starts_with("free crafting") || line.starts_with("order: ") {
        Play("ui_click", 0.32, 1.0)
    } else {
        return None;
    })
}

#[allow(clippy::type_complexity, clippy::too_many_arguments)]
fn play(
    mut commands: Commands,
    time: Res<Time>,
    clips: Res<Clips>,
    log: Res<GameLog>,
    ui: Res<UiFocus>,
    mut mixer: ResMut<Mixer>,
    (mut hits, mut damaged): (MessageReader<HitMsg>, MessageReader<Damaged>),
    (player, rig): (Query<(&Mover, &Locomotion), With<Player>>, Query<&crate::anim::PlayerRig>),
    spots: Query<&GlobalTransform>,
    (nodes, roster): (Query<&ResourceNode>, Query<(), With<CreatureActor>>),
    swings: Query<(), Added<Swing>>,
    throws: Query<(), Added<OrbBall>>,
    buttons: Query<&Interaction, (Changed<Interaction>, With<Button>)>,
    mut was_ui: Local<bool>,
) {
    let now = time.elapsed_secs();
    let mut out: Vec<Play> = Vec::new();
    let me = player.single().ok().zip(rig.single().ok()).map(|((m, l), r)| (m, l, r));
    let feet = me.map(|(m, ..)| m.feet);
    // Quieter with distance, silent beyond HEAR_M.
    let near = |e: Entity| -> f32 {
        match (feet, spots.get(e)) {
            (Some(f), Ok(t)) => (1.0 - t.translation().distance(f) / HEAR_M).clamp(0.0, 1.0),
            _ => 1.0,
        }
    };

    let new = (log.total - mixer.seen).min(log.lines.len());
    mixer.seen = log.total;
    for (_, line) in &log.lines[log.lines.len() - new..] {
        out.extend(for_line(line));
    }

    for h in hits.read() {
        let v = near(h.target);
        if let Ok(n) = nodes.get(h.target) {
            match n.kind {
                NodeKind::Tree => out.push(Play("axe", 0.6 * v, 1.0)),
                NodeKind::Rock | NodeKind::Crystal | NodeKind::Ore | NodeKind::Pebble => out.push(Play("pickaxe", 0.55 * v, 1.0)),
                NodeKind::BerryBush => {}
            }
        } else {
            out.push(Play("hit", 0.6 * v, 1.0));
        }
    }
    for d in damaged.read() {
        if !d.killed && d.slip.is_none() && roster.contains(d.target) {
            out.push(Play("creature_cry", 0.35 * near(d.target), 1.0));
        }
    }
    if !swings.is_empty() {
        out.push(Play("swing", 0.55, 1.0));
    }
    if !throws.is_empty() {
        out.push(Play("throw", 0.6, 1.0));
    }
    if buttons.iter().any(|i| *i == Interaction::Pressed) {
        out.push(Play("ui_click", 0.4, 1.0));
    }
    if ui.0 != *was_ui {
        *was_ui = ui.0;
        out.push(if ui.0 { Play("ui_open", 0.55, 1.0) } else { Play("ui_open", 0.4, 0.85) });
    }

    // Footsteps when the walk animation plants a foot (anim.rs counts them), softer when crouched.
    if let Some((m, loco, rig)) = me {
        let new = rig.footfalls.wrapping_sub(mixer.footfalls);
        mixer.footfalls = rig.footfalls;
        if new > 0 && matches!(loco.mode, Mode::Ground) && m.grounded {
            out.push(Play("step", if loco.crouched { 0.12 } else { 0.25 }, 1.0));
        }
    }

    for Play(clip, vol, speed) in out {
        let gap = match clip {
            "step" => 0.0,
            "creature_cry" => 0.7,
            _ => 0.08,
        };
        if vol <= 0.01 || mixer.last.get(clip).is_some_and(|t| now - t < gap) {
            continue;
        }
        let Some(h) = clips.0.get(clip) else { continue };
        mixer.last.insert(clip, now);
        *mixer.counts.entry(clip).or_default() += 1;
        if mixer.muted {
            continue;
        }
        // A little pitch spread so repeated sounds don't feel copy-pasted.
        let speed = speed * (0.93 + 0.14 * mixer.rand());
        commands.spawn((AudioPlayer::new(h.clone()), PlaybackSettings::DESPAWN.with_volume(Volume::Linear(vol)).with_speed(speed)));
    }
}

fn report(mut exit: MessageReader<AppExit>, mixer: Res<Mixer>) {
    if exit.read().next().is_some() {
        let mut c: Vec<_> = mixer.counts.iter().collect();
        c.sort();
        info!("sfx played: {}", c.iter().map(|(k, n)| format!("{k} {n}")).collect::<Vec<_>>().join(", "));
    }
}

#[cfg(test)]
mod tests {
    use super::for_line;

    fn clip(line: &str) -> Option<&'static str> {
        for_line(line).map(|p| p.0)
    }

    #[test]
    fn log_lines_map_to_sounds() {
        assert_eq!(clip("+3 Wood"), Some("pickup"));
        assert_eq!(clip("+12 XP (captured)"), None);
        assert_eq!(clip("captured Fluffin Lv3 (p=80.0%) -> party slot 2"), Some("capture"));
        assert_eq!(clip("shake 2..."), Some("shake"));
        assert_eq!(clip("shake rolls [0.1] must be below [0.5] -> 1 pass"), None);
        assert_eq!(clip("Fluffin broke free after 1 shake(s) (p=10.0%)"), Some("breakfree"));
        assert_eq!(clip("Campfire: Baked Berries x1 ready"), Some("craft_done"));
        assert_eq!(clip("Camp Core finished"), Some("build_done"));
        assert_eq!(clip("movement: Air -> Glide"), Some("glide"));
        assert_eq!(clip("build rejected: not enough Wood (0/10)"), Some("ui_click"));
        assert_eq!(clip("player level 2 (+6 technology points)"), Some("levelup"));
    }
}
