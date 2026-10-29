//! Unattended checks: `--screenshot <png> [--frames n]`, `--auto <script>` (scripted input with a fixed
//! 1/60 s step, logged state and screenshots), `--visible` to show the window (hidden by default for both).

use bevy::prelude::*;
use bevy::render::view::screenshot::{Screenshot, save_to_disk};
use bevy::time::TimeUpdateStrategy;

use crate::input::{InputSource, PlayerInput};

#[derive(Resource, Debug, Clone, Default)]
pub struct DevArgs {
    pub screenshot: Option<String>,
    pub frames: u32,
    pub auto: Option<String>,
    pub visible: bool,
    pub seed: u64,
    /// `--sandbox`: material costs capped at 1, max level, free mode (default off).
    pub easy: Option<bool>,
}

impl DevArgs {
    pub fn parse(args: &[String]) -> Result<Self, String> {
        let mut d = DevArgs { frames: 90, seed: 1, ..default() };
        let mut it = args.iter();
        while let Some(a) = it.next() {
            let mut val = |name: &str| it.next().cloned().ok_or(format!("{name} needs a value"));
            match a.as_str() {
                "--screenshot" => d.screenshot = Some(val("--screenshot")?),
                "--frames" => d.frames = val("--frames")?.parse().map_err(|e| format!("--frames: {e}"))?,
                "--auto" => d.auto = Some(val("--auto")?),
                "--seed" => d.seed = val("--seed")?.parse().map_err(|e| format!("--seed: {e}"))?,
                "--visible" => d.visible = true,
                "--sandbox" => d.easy = Some(true),
                other => return Err(format!("unknown argument {other}")),
            }
        }
        Ok(d)
    }

    pub fn easy(&self) -> bool {
        // Scripts check the real values, except the camp-work demo (Fluffin with every job).
        self.easy.unwrap_or(false)
    }

    pub fn hidden(&self) -> bool {
        (self.screenshot.is_some() || self.auto.is_some()) && !self.visible
    }
}

/// One scripted step: hold this input for `secs`, then optionally screenshot / check.
#[derive(Debug, Clone, Default)]
pub struct Step {
    pub secs: f32,
    pub input: PlayerInput,
    /// Teleport the player's feet here (y = None: onto the terrain) before the step.
    pub teleport: Option<(Vec2, Option<f32>)>,
    pub yaw: Option<f32>,
    pub pitch: Option<f32>,
    pub shot: Option<&'static str>,
    pub label: &'static str,
    /// Run arbitrary world setup before the step.
    pub setup: Option<fn(&mut World)>,
}

#[derive(Resource, Debug, Default)]
pub struct Script {
    pub name: String,
    pub steps: Vec<Step>,
    pub index: usize,
    pub t: f32,
    pub started: bool,
    pub frame: u32,
    pub exit_in: Option<u32>,
}

pub struct DevToolsPlugin(pub DevArgs);

impl Plugin for DevToolsPlugin {
    fn build(&self, app: &mut App) {
        let d = self.0.clone();
        app.insert_resource(d.clone()).init_resource::<crate::scripts::SlopeMark>().init_resource::<crate::scripts::StepMark>();
        if let Some(name) = &d.auto {
            let steps = crate::scripts::script(name).unwrap_or_else(|| {
                eprintln!("unknown script {name}; known: {}", crate::scripts::NAMES.join(", "));
                std::process::exit(2)
            });
            app.insert_resource(InputSource::Script)
                .insert_resource(TimeUpdateStrategy::ManualDuration(std::time::Duration::from_secs_f64(1.0 / 60.0)))
                .insert_resource(Script { name: name.clone(), steps, ..default() })
                .add_systems(PreUpdate, run_script.after(bevy::input::InputSystems));
        } else if d.screenshot.is_some() {
            app.add_systems(Update, screenshot_after_frames);
        }
    }
}

fn shot_path(name: &str) -> std::path::PathBuf {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/shots");
    let _ = std::fs::create_dir_all(&dir);
    dir.join(name)
}

fn screenshot_after_frames(mut commands: Commands, args: Res<DevArgs>, mut n: Local<u32>, mut exit: MessageWriter<AppExit>) {
    *n += 1;
    if *n == args.frames {
        let p = args.screenshot.clone().unwrap_or_default();
        commands.spawn(Screenshot::primary_window()).observe(save_to_disk(p));
    }
    if *n == args.frames + 5 {
        exit.write(AppExit::Success);
    }
}

fn run_script(world: &mut World) {
    let dt = 1.0 / 60.0;
    let mut script = world.remove_resource::<Script>().expect("script");
    script.frame += 1;
    if let Some(n) = script.exit_in.as_mut() {
        *n = n.saturating_sub(1);
        if *n == 0 {
            world.write_message(AppExit::Success);
        }
        world.insert_resource(PlayerInput::default());
        world.insert_resource(script);
        return;
    }
    // Let the world settle for a few frames before the first step.
    if script.frame < 20 {
        world.insert_resource(PlayerInput::default());
        world.insert_resource(script);
        return;
    }
    if script.index >= script.steps.len() {
        crate::scripts::report(world, &script.name, "end");
        script.exit_in = Some(10);
        world.insert_resource(script);
        return;
    }
    let step = script.steps[script.index].clone();
    let mut input = step.input.clone();
    if !script.started {
        script.started = true;
        if let Some(f) = step.setup {
            f(world);
        }
        if let Some((p, y)) = step.teleport {
            crate::scripts::teleport(world, p, y);
        }
        if let Some(yaw) = step.yaw {
            world.resource_mut::<crate::camera::CameraRig>().yaw = yaw;
        }
        if let Some(pitch) = step.pitch {
            world.resource_mut::<crate::camera::CameraRig>().pitch = pitch;
        }
    } else {
        // Edges only on the first frame of a step.
        input.jump_pressed = false;
        input.crouch_pressed = false;
        input.roll_pressed = false;
        input.attack_pressed = false;
        input.interact_tapped = false;
        input.interact_pressed = false;
        input.throw_pressed = false;
        input.summon_pressed = false;
        input.partner_pressed = false;
        input.party_next = false;
        input.party_prev = false;
        input.orb_next = false;
        input.menu_pressed = false;
        input.build_pressed = false;
        input.tech_pressed = false;
        input.command_pressed = false;
        input.direct_attack_pressed = false;
        input.weapon_scroll = 0;
        input.rotate_pressed = false;
        input.escape_pressed = false;
        input.inspector_pressed = false;
        input.save_pressed = false;
        input.load_pressed = false;
        input.cheat_pressed = false;
        input.mute_pressed = false;
        input.dismantle_pressed = false;
    }
    script.t += dt;
    if script.t >= step.secs {
        // Last frame of the step: release edge-held buttons the step asked for.
        if step.input.throw_held {
            input.throw_released = true;
            input.throw_held = false;
        }
        crate::scripts::report(world, &script.name, step.label);
        if let Some(name) = step.shot {
            let path = shot_path(&format!("{}_{}.png", script.name, name));
            world.spawn(Screenshot::primary_window()).observe(save_to_disk(path));
        }
        script.index += 1;
        script.t = 0.0;
        script.started = false;
    }
    world.insert_resource(input);
    world.insert_resource(script);
}
