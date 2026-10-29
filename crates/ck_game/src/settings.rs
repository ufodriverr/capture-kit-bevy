//! Player settings (Esc menu): fullscreen or windowed, sound, free mode. Kept in `settings.json` next to the exe;
//! hidden runs (scripts, screenshots) use a fixed windowed, muted setup and never write the file.

use bevy::prelude::*;
use bevy::window::{MonitorSelection, PrimaryWindow, WindowMode, WindowPosition};
use serde::{Deserialize, Serialize};

/// Window size when not fullscreen (also the hidden runs' size).
pub const WINDOWED: (u32, u32) = (1600, 900);

#[derive(Resource, Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub fullscreen: bool,
    pub muted: bool,
    /// Free mode (F2): crafting, building and technology ignore their costs.
    pub free: bool,
    /// False for hidden runs: nothing is written.
    #[serde(skip)]
    pub persist: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self { fullscreen: true, muted: false, free: false, persist: true }
    }
}

impl Settings {
    pub fn hidden() -> Self {
        Self { fullscreen: false, muted: true, free: false, persist: false }
    }

    pub fn path() -> std::path::PathBuf {
        std::env::current_exe().ok().and_then(|p| p.parent().map(|d| d.join("settings.json"))).unwrap_or_else(|| "settings.json".into())
    }

    /// The saved settings, or the defaults (fullscreen, sound on) when there are none.
    pub fn load() -> Self {
        std::fs::read_to_string(Self::path()).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default()
    }

    pub fn save(&self) {
        if !self.persist {
            return;
        }
        if let Ok(s) = serde_json::to_string_pretty(self)
            && let Err(e) = std::fs::write(Self::path(), s)
        {
            eprintln!("settings: {e}");
        }
    }

    pub fn window_mode(&self) -> WindowMode {
        if self.fullscreen { WindowMode::BorderlessFullscreen(MonitorSelection::Current) } else { WindowMode::Windowed }
    }
}

/// Switch the window and remember the choice.
pub fn set_fullscreen(world: &mut World, on: bool) {
    let mut s = world.resource_mut::<Settings>();
    s.fullscreen = on;
    let s = s.clone();
    s.save();
    let mut q = world.query_filtered::<&mut Window, With<PrimaryWindow>>();
    if let Ok(mut w) = q.single_mut(world) {
        w.mode = s.window_mode();
        if !on {
            w.resolution.set(WINDOWED.0 as f32, WINDOWED.1 as f32);
            w.position = WindowPosition::Centered(MonitorSelection::Current);
        }
    }
}

/// Free mode on / off (F2 does the same) and remember it.
pub fn set_free(world: &mut World, on: bool) {
    world.resource_mut::<crate::crafting::FreeCraft>().0 = on;
    let mut s = world.resource_mut::<Settings>();
    s.free = on;
    s.clone().save();
}

/// Sound on / off (F3 does the same) and remember it.
pub fn set_muted(world: &mut World, muted: bool) {
    crate::sfx::set_muted(world, muted);
    let mut s = world.resource_mut::<Settings>();
    s.muted = muted;
    s.clone().save();
}
