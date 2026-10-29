//! Capture Kit (Bevy): an open-source creature-capture game kit built with Rust and Bevy.
//!
//! Dev flags: `--screenshot <png> [--frames n]`, `--auto <script>`, `--visible`, `--seed n` (devtools.rs).

mod anim;
mod base;
mod base_tools;
mod build_models;
mod building;
mod camera;
mod crafting;
mod menu_view;
mod menus;
mod capture;
mod combat;
mod companion;
mod data;
mod devtools;
mod drops;
mod fx;
mod hud;
mod hud_more;
mod input;
mod inspector;
mod interact;
mod inventory;
mod kinematic;
mod log;
mod creatures;
mod partner;
mod player;
mod progress;
mod quests;
mod resources;
mod save;
mod scripts;
mod settings;
mod sfx;
mod skills;
mod status;
mod tuning;
mod ui;
mod work;
mod work_hud;
mod world;

use avian3d::prelude::*;
use bevy::prelude::*;

/// Simulation order within `Update`.
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub enum Sim {
    Player,
    Creatures,
    Combat,
    Last,
}

fn main() -> AppExit {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let dev = match devtools::DevArgs::parse(&args) {
        Ok(d) => d,
        Err(e) => {
            eprintln!("{e}");
            return AppExit::error();
        }
    };
    let db = match data::Db::load(dev.easy()) {
        Ok(d) => d,
        Err(e) => {
            eprintln!("game data: {e:#}");
            return AppExit::error();
        }
    };
    // Hidden runs (screenshots, scripts) start windowed and muted and don't touch settings.json.
    let settings = if dev.hidden() { settings::Settings::hidden() } else { settings::Settings::load() };
    let muted = settings.muted;
    let assets_dir = data::assets_dir();
    App::new()
        .add_plugins(
            DefaultPlugins
                .set(WindowPlugin {
                    primary_window: Some(Window { title: "Capture Kit".into(), resolution: settings::WINDOWED.into(), mode: settings.window_mode(), visible: !dev.hidden(), ..default() }),
                    ..default()
                })
                // Sounds and icons: the `assets` folder (repo root or next to the exe).
                .set(bevy::asset::AssetPlugin { file_path: assets_dir.to_string_lossy().into_owned(), ..default() }),
        )
        .add_plugins(PhysicsPlugins::default())
        .insert_resource(Gravity(Vec3::NEG_Y * 9.8))
        .configure_sets(Update, (Sim::Player, Sim::Creatures, Sim::Combat, Sim::Last).chain())
        .insert_resource(db.move_tuning())
        .insert_resource(progress::Progress::start(&db))
        .insert_resource(crafting::FreeCraft(db.easy || settings.free))
        .insert_resource(db)
        .insert_resource(settings)
        .init_resource::<log::GameLog>()
        .add_plugins((input::InputPlugin, world::WorldPlugin, player::PlayerPlugin, camera::CameraPlugin, hud::HudPlugin, devtools::DevToolsPlugin(dev)))
        .add_plugins((interact::InteractPlugin, inventory::InventoryPlugin, resources::ResourcesPlugin, combat::CombatPlugin, drops::DropsPlugin, creatures::CreaturesPlugin, companion::CompanionPlugin, capture::CapturePlugin, partner::PartnerPlugin, progress::ProgressPlugin, hud_more::HudMorePlugin))
        .add_plugins((building::BuildingPlugin, crafting::CraftingPlugin, menus::MenusPlugin, base::BasePlugin, save::SavePlugin, inspector::InspectorPlugin, quests::QuestPlugin, anim::AnimPlugin, fx::FxPlugin, work::WorkPlugin, ui::UiPlugin, work_hud::WorkHudPlugin))
        .add_plugins(sfx::SfxPlugin { muted })
        .add_plugins((base_tools::BaseToolsPlugin, status::StatusPlugin))
        .run()
}
