//! The kit event log: accepted/rejected actions and state changes with their reasons, shown on screen
//! and printed to the console (so scripted runs leave a trail).

use bevy::prelude::*;

#[derive(Resource, Debug, Default)]
pub struct GameLog {
    pub lines: Vec<(f32, String)>,
    /// Lines pushed so far (`lines` keeps only the last 200).
    pub total: usize,
}

impl GameLog {
    pub fn push(&mut self, t: f32, msg: impl Into<String>) {
        let msg = msg.into();
        info!("[{t:7.2}] {msg}");
        self.lines.push((t, msg));
        self.total += 1;
        if self.lines.len() > 200 {
            self.lines.remove(0);
        }
    }
}

/// Short-hand for systems: `log.say(&time, "...")`.
pub trait Say {
    fn say(&mut self, time: &Time, msg: impl Into<String>);
}

impl Say for GameLog {
    fn say(&mut self, time: &Time, msg: impl Into<String>) {
        self.push(time.elapsed_secs(), msg);
    }
}
